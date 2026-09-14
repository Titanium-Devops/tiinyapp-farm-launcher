// The window. Everything it knows, it asked the engine for; everything it
// shows, the engine said. There is no second idea of what an install means
// living in here.

const { invoke } = window.__TAURI__.core;
const { listen } = window.__TAURI__.event;

const $ = (id) => document.getElementById(id);
const el = (tag, attrs = {}, ...kids) => {
  const node = document.createElement(tag);
  for (const [key, value] of Object.entries(attrs)) {
    if (value === null || value === undefined || value === false) continue;
    if (key === "class") node.className = value;
    else if (key === "text") node.textContent = value;
    else if (key.startsWith("on")) node.addEventListener(key.slice(2), value);
    else node.setAttribute(key, value === true ? "" : String(value));
  }
  for (const kid of kids) if (kid) node.append(kid);
  return node;
};

const farm = {
  info: null,
  settings: { autostart: false, farmOnPath: false, manualBase: null },
  device: { configured: false, base: null },
  catalog: [],
  installed: [],
  running: [],
  manifests: new Map(),
  working: new Map(), // id -> { phase, line, fraction }
  trouble: new Map(), // id -> { message, commentary }
  open: null,
};

// The words the engine used, or the words it would have used. An error that
// crosses the bridge as a plain string still reads like a sentence.
function sentence(error) {
  if (!error) return "Something went wrong and nothing said what.";
  if (typeof error === "string") return error;
  if (error.message) return error.message;
  return String(error);
}

let toastTimer = null;
function say(words) {
  const toast = $("toast");
  toast.textContent = words;
  toast.hidden = false;
  clearTimeout(toastTimer);
  toastTimer = setTimeout(() => { toast.hidden = true; }, 4200);
}

// Views -----------------------------------------------------------------
// The launcher and the command line tool share one directory, so what is
// installed can change without the window doing anything. Reading it again when
// somebody comes back to the window, or moves to a pane that shows it, is what
// keeps the Running pane from saying nothing is planted while three apps are.
let lastRead = 0;
async function reread(force) {
  if (!farm.info) return;
  if (!force && Date.now() - lastRead < 8000) return;
  lastRead = Date.now();
  await refresh();
}

function show(view) {
  for (const name of ["farm", "running", "settings"]) {
    $(`view-${name}`).hidden = name !== view;
    document.querySelector(`nav button[data-view="${name}"]`).setAttribute("aria-pressed", String(name === view));
  }
}
for (const button of document.querySelectorAll("nav button")) {
  button.addEventListener("click", () => { show(button.dataset.view); reread(); });
}
window.addEventListener("focus", () => reread());
$("go-farm").addEventListener("click", () => show("farm"));
for (const button of document.querySelectorAll("[data-close]")) {
  button.addEventListener("click", () => $(button.dataset.close).close());
}

// The badge on a card, the same four the site computes ------------------
function badge(row, manifest) {
  if (row.release === "none" || row.release === "pending") return { text: "No release yet", kind: "" };
  if (manifest && manifest.entry === null) return { text: "Library", kind: "" };
  if (manifest && manifest.verified) return { text: "Reviewed", kind: "ok" };
  const added = manifest && manifest.addedAt ? Date.parse(manifest.addedAt) : NaN;
  if (!Number.isNaN(added) && Date.now() - added < 30 * 24 * 3600 * 1000) return { text: "New", kind: "warm" };
  return null;
}

function art(manifest, which) {
  const path = manifest && manifest.media ? manifest.media[which] : null;
  if (!path) return null;
  return path.startsWith("http") ? path : farm.info.site + path;
}

// The catalog grid --------------------------------------------------------
function renderCatalog() {
  const grid = $("catalog");
  grid.replaceChildren();
  const rows = farm.catalog.filter((row) => row.release !== "none");
  $("catalog-empty").hidden = rows.length > 0;
  grid.hidden = rows.length === 0;
  for (const row of rows) {
    const manifest = farm.manifests.get(row.id);
    const mark = badge(row, manifest);
    const header = art(manifest, "header");
    const icon = art(manifest, "icon");
    const installed = row.installed;
    const tile = el("button", { class: "tile", type: "button", onclick: () => openCard(row.id) },
      el("div", { class: "art", style: header ? `background-image:url("${header}")` : null },
        icon ? el("img", { class: "icon", src: icon, alt: "" }) : null),
      el("div", { class: "body" },
        el("div", { class: "name", text: row.name }),
        el("p", { class: "pitch", text: row.pitch }),
        el("div", { class: "foot" },
          el("div", { class: "chips" },
            el("span", { class: "chip", text: installed ? `Planted ${installed}` : row.version }),
            mark ? el("span", { class: `chip ${mark.kind}`, text: mark.text }) : null),
          el("span", { class: "btn small", text: installed ? "Open the card" : "Plant it" }))));
    grid.append(tile);
  }
}

// The Running pane --------------------------------------------------------
function renderInstalled() {
  const list = $("installed");
  list.replaceChildren();
  const running = new Map(farm.running.map((row) => [row.id, row]));
  $("tally").textContent = String(farm.running.length);
  $("installed-empty").hidden = farm.installed.length > 0;
  list.hidden = farm.installed.length === 0;

  for (const row of farm.installed) {
    const live = running.get(row.id);
    const manifest = farm.manifests.get(row.id);
    const icon = art(manifest, "icon");
    const working = farm.working.get(row.id);
    const bad = farm.trouble.get(row.id);

    const facts = el("div", { class: "m" });
    if (working) facts.append(el("span", { text: working.line }));
    else if (live) {
      facts.append(el("span", { text: live.port ? `Port ${live.port}` : "Running" }));
      facts.append(el("span", { text: `Up ${Math.round(live.uptime / 60)} min` }));
      if (live.health === "unavailable") facts.append(el("span", { text: "Not answering yet" }));
      if (live.restartToUpdate) facts.append(el("span", { text: `Restart to reach ${live.installed}` }));
    } else {
      facts.append(el("span", { text: `Version ${row.version}` }));
      facts.append(el("span", { text: "Stopped" }));
    }
    if (row.updateAvailable) facts.append(el("span", { text: `Update ${row.updateAvailable} is ready` }));
    if (bad) facts.append(el("span", { text: bad.message }));

    const actions = el("div", { class: "act" });
    if (working) {
      actions.append(el("span", { class: "chip", text: "Working" }));
    } else if (live) {
      actions.append(el("button", { class: "btn small", type: "button", onclick: () => open(row.id) }, document.createTextNode("Open")));
      actions.append(el("button", { class: "btn quiet", type: "button", onclick: () => openInBrowser(row.id) }, document.createTextNode("In browser")));
      actions.append(el("button", { class: "btn ghost small", type: "button", onclick: () => stop(row.id) }, document.createTextNode("Stop")));
    } else {
      const runnable = !manifest || manifest.entry !== null;
      if (runnable) actions.append(el("button", { class: "btn small", type: "button", onclick: () => start(row.id) }, document.createTextNode("Start")));
      else actions.append(el("span", { class: "chip", text: "A library, nothing to start" }));
    }
    if (row.updateAvailable && !working) {
      actions.append(el("button", { class: "btn ghost small", type: "button", onclick: () => update(row.id) }, document.createTextNode("Update")));
    }
    actions.append(el("button", { class: "btn quiet", type: "button", onclick: () => showLog(row.id) }, document.createTextNode("Log")));
    actions.append(el("button", { class: "btn quiet", type: "button", onclick: () => remove(row.id) }, document.createTextNode("Remove")));

    const middle = el("div", {},
      el("div", { class: "n", text: row.name }),
      facts,
      working ? el("div", { style: "margin-top:10px;max-width:420px" },
        el("div", { class: "bar" }, el("i", { style: `width:${Math.round(working.fraction * 100)}%` }))) : null);

    list.append(el("div", { class: "approw" },
      el("img", { src: icon || "farm-mark.png", alt: "" }),
      middle,
      actions));
  }
}

// One app's card ----------------------------------------------------------
async function openCard(id) {
  farm.open = id;
  const dialog = $("card");
  const row = farm.catalog.find((r) => r.id === id) || {};
  $("card-name").textContent = row.name || id;
  $("card-pitch").textContent = row.pitch || "";
  $("card-sub").textContent = "";
  $("card-needs").textContent = "Reading the catalog.";
  $("card-permissions").textContent = "";
  $("card-description").textContent = "";
  $("card-icon").src = "farm-mark.png";
  renderCardState();
  if (!dialog.open) dialog.showModal();

  let manifest = farm.manifests.get(id);
  if (!manifest) {
    try {
      manifest = await invoke("farm_manifest", { id });
      farm.manifests.set(id, manifest);
    } catch (error) {
      $("card-needs").textContent = sentence(error);
      return;
    }
  }
  if (farm.open !== id) return;
  const icon = art(manifest, "icon");
  if (icon) $("card-icon").src = icon;
  $("card-name").textContent = manifest.name;
  $("card-sub").textContent = [
    `Version ${manifest.version}`,
    manifest.author && manifest.author.name ? `by ${manifest.author.name}` : null,
    manifest.license,
  ].filter(Boolean).join(" · ");
  $("card-pitch").textContent = manifest.pitch;
  $("card-description").textContent = manifest.description || manifest.pitch;
  $("card-needs").textContent = needsSentence(manifest);
  $("card-permissions").textContent = permissionsSentence(manifest);
  renderCardState();
}

// The same two sentences the command line tool prints before it installs.
const PERMISSION_WORDS = {
  files: "files in its own folder",
  network: "the network",
  device: "your Tiiny",
  camera: "the camera",
  microphone: "the microphone",
  screen: "the screen",
};

function needsSentence(manifest) {
  const requires = manifest.requires || {};
  const parts = [];
  if (requires.python) parts.push(`Python ${requires.python} or newer`);
  const ports = requires.ports || [];
  if (ports.length) parts.push((ports.length === 1 ? "port " : "ports ") + ports.join(", "));
  const device = requires.device || {};
  if ((device.models || []).length) parts.push("your Tiiny, for " + device.models.join(", "));
  if (device.npuUnits) parts.push(`${device.npuUnits} NPU units`);
  const said = parts.length ? parts.join(", ") : "nothing beyond Python";
  return `It needs ${said}. The launcher carries its own Python ${farm.info.python}, so there is nothing to install first.`;
}

function permissionsSentence(manifest) {
  const words = (manifest.permissions || []).map((name) => PERMISSION_WORDS[name]).filter(Boolean);
  if (!words.length) return "It declares no access to anything on this computer.";
  if (words.length === 1) return `It can reach ${words[0]}.`;
  return `It can reach ${words.slice(0, -1).join(", ")} and ${words[words.length - 1]}.`;
}

function renderCardState() {
  const id = farm.open;
  if (!id) return;
  const actions = $("card-actions");
  actions.replaceChildren();
  const working = farm.working.get(id);
  const bad = farm.trouble.get(id);
  const installedRow = farm.installed.find((r) => r.id === id);
  const live = farm.running.find((r) => r.id === id);
  const manifest = farm.manifests.get(id);

  $("card-progress").hidden = !working;
  if (working) {
    $("card-bar").style.width = `${Math.round(working.fraction * 100)}%`;
    $("card-step").textContent = working.line;
  }

  const trouble = $("card-trouble");
  trouble.hidden = !bad;
  if (bad) {
    $("card-trouble-head").textContent = bad.head;
    $("card-trouble-text").textContent = bad.also ? `${bad.message} ${bad.also}` : bad.message;
    const row = $("card-trouble-actions");
    row.replaceChildren();
    if (bad.offerPort) {
      const input = el("input", { type: "number", value: String(bad.offerPort), min: "1", max: "65535",
        style: "width:120px;border:1px solid #2a3542;background:#0b1017;color:var(--ink);border-radius:10px;padding:8px 10px;font:15px var(--body)" });
      row.append(input);
      row.append(el("button", { class: "btn small", type: "button", onclick: () => start(id, Number(input.value)) },
        document.createTextNode("Try that port")));
    }
    if (bad.offerLog) {
      row.append(el("button", { class: "btn ghost small", type: "button", onclick: () => showLog(id) },
        document.createTextNode("Show the log")));
    }
    row.append(el("button", { class: "btn quiet", type: "button", onclick: () => { farm.trouble.delete(id); renderCardState(); renderInstalled(); } },
      document.createTextNode("Put it away")));
  }

  if (working) {
    actions.append(el("span", { class: "chip", text: working.line }));
    return;
  }
  if (!installedRow) {
    const row = farm.catalog.find((r) => r.id === id) || {};
    const plantable = row.release === "ready";
    actions.append(el("button", { class: "btn", type: "button", disabled: !plantable, onclick: () => install(id) },
      document.createTextNode(plantable ? "Plant it" : "No release yet")));
    return;
  }
  if (live) {
    actions.append(el("button", { class: "btn", type: "button", onclick: () => open(id) }, document.createTextNode("Open")));
    actions.append(el("button", { class: "btn quiet", type: "button", onclick: () => openInBrowser(id) }, document.createTextNode("In browser")));
    actions.append(el("button", { class: "btn ghost", type: "button", onclick: () => stop(id) }, document.createTextNode("Stop")));
  } else if (!manifest || manifest.entry !== null) {
    actions.append(el("button", { class: "btn", type: "button", onclick: () => start(id) }, document.createTextNode("Start")));
  } else {
    actions.append(el("span", { class: "chip", text: "A library, so there is nothing to start" }));
  }
  if (installedRow.updateAvailable) {
    actions.append(el("button", { class: "btn ghost", type: "button", onclick: () => update(id) },
      document.createTextNode(`Update to ${installedRow.updateAvailable}`)));
  }
  actions.append(el("button", { class: "btn quiet", type: "button", onclick: () => remove(id) }, document.createTextNode("Remove")));
}

// Failure, in words -------------------------------------------------------
// The engine says what went wrong and the Rust side says what to offer next,
// so nothing here reads an error message looking for words in it.
function readTrouble(id, error) {
  const said = typeof error === "object" && error && error.trouble ? error.trouble : {};
  farm.trouble.set(id, {
    message: sentence(error),
    head: said.head || "That did not work",
    also: said.also || "",
    offerPort: said.offerPort ?? null,
    offerLog: Boolean(said.offerLog),
    commentary: (typeof error === "object" && error && error.commentary) || [],
  });
}

// Actions ----------------------------------------------------------------
async function install(id) {
  farm.trouble.delete(id);
  farm.working.set(id, { line: "Looking it up in the catalog", fraction: 0.05 });
  renderCardState(); renderInstalled();
  try {
    await invoke("farm_install", { id });
    say(`${nameOf(id)} is planted.`);
  } catch (error) {
    readTrouble(id, error);
  } finally {
    farm.working.delete(id);
    await refresh();
    renderCardState();
  }
}

async function update(id) {
  farm.trouble.delete(id);
  farm.working.set(id, { line: "Looking it up in the catalog", fraction: 0.05 });
  renderCardState(); renderInstalled();
  try {
    const answer = await invoke("farm_update", { id });
    say(answer.updated ? `${nameOf(id)} is on ${answer.version}.` : `${nameOf(id)} was already up to date.`);
  } catch (error) {
    readTrouble(id, error);
  } finally {
    farm.working.delete(id);
    await refresh();
    renderCardState();
  }
}

async function start(id, port) {
  farm.trouble.delete(id);
  farm.working.set(id, { line: "Starting", fraction: 0.5 });
  renderCardState(); renderInstalled();
  try {
    const answer = await invoke("farm_start", { id, port: port ?? null });
    say(answer.already ? `${nameOf(id)} was already running.` : `${nameOf(id)} is running on port ${answer.port}.`);
  } catch (error) {
    readTrouble(id, error);
  } finally {
    farm.working.delete(id);
    await refresh();
    renderCardState();
  }
}

async function stop(id) {
  try {
    await invoke("farm_stop", { id });
    say(`${nameOf(id)} is stopped.`);
  } catch (error) {
    readTrouble(id, error);
  }
  await refresh();
  renderCardState();
}

async function remove(id) {
  try {
    const said = await invoke("farm_remove", { id, purge: false });
    say(said || `${nameOf(id)} is removed.`);
    farm.trouble.delete(id);
    if (farm.open === id) $("card").close();
  } catch (error) {
    readTrouble(id, error);
  }
  await refresh();
}

// Open puts the app in its own window, with its own name on it, rather than
// taking a tab from whatever somebody was doing. The browser is still there as
// a second way out, and a setting makes it the first.
async function open(id) {
  try {
    const answer = await invoke("open_app", { id, name: nameOf(id) });
    if (answer.where === "browser") say(`${nameOf(id)} is open in your browser.`);
  } catch (error) {
    readTrouble(id, error);
    renderCardState();
    renderInstalled();
    say(sentence(error));
  }
}

async function openInBrowser(id) {
  const live = farm.running.find((r) => r.id === id);
  if (!live || !live.url) { say("That app has not said which port it took yet."); return; }
  try { await invoke("open_external", { url: live.url }); }
  catch (error) { say(sentence(error)); }
}

async function showLog(id) {
  $("logs-name").textContent = `${nameOf(id)} · farm.log`;
  try {
    const body = await invoke("farm_log", { id, lines: 60 });
    $("logs-body").textContent = body || "The log is empty.";
  } catch (error) {
    $("logs-body").textContent = sentence(error);
  }
  $("logs").showModal();
}

function nameOf(id) {
  const row = farm.catalog.find((r) => r.id === id) || farm.installed.find((r) => r.id === id);
  return row ? row.name : id;
}

// The Tiiny ---------------------------------------------------------------
// The engine does the looking. All this does is draw what it found and let
// somebody pick one.

const VIA_WORDS = {
  cable: "over the USB cable",
  network: "on this network",
  "TiinyOS client": "through the TiinyOS client",
};

let chosenBase = null;

function lookingState(words) {
  $("device-found").replaceChildren(el("p", { class: "lede", style: "margin:0", text: words }));
}

async function lookForTiinys() {
  lookingState("Looking on every USB cable, on this network, and at the TiinyOS client.");
  let answer;
  try {
    answer = await invoke("device_find");
  } catch (error) {
    lookingState(sentence(error));
    return;
  }
  const box = $("device-found");
  box.replaceChildren();

  // macOS refusing this app the local network is not the same finding as an
  // absent Tiiny, and saying the second when it is the first sends somebody to
  // check a cable that was never the problem.
  if (answer.blocked) {
    box.append(el("div", { class: "trouble" },
      el("h3", { text: "macOS has not let this app onto your local network yet" }),
      el("p", { text: "A box asking about it appears the first time the farm looks. If you have not answered it, answer it and press Look again. If you said no, it is System Settings, then Privacy and Security, then Local Network, then Tiiny App Farm." }),
      el("p", { class: "lede", style: "margin:0;font-size:13px", text: `The farm was looking with ${answer.python}.` })));
  }

  if (answer.unsupported) {
    box.append(el("p", { class: "lede", style: "margin:0",
      text: "The farm inside this copy of the launcher is older than the command that looks for a Tiiny. Type the address in yourself for now; the next launcher update carries a farm that can look." }));
    $("device-manual").hidden = false;
    $("device-key-step").hidden = false;
    return;
  }

  const found = answer.found || [];
  if (!found.length) {
    if (!answer.blocked) {
      box.append(el("p", { class: "lede", style: "margin:0",
        text: "No Tiiny answered. Switch it on, plug the cable in or put it on this network, then press Look again. You can also type its address in yourself." }));
    }
    return;
  }

  for (const row of found) {
    const name = row.name || "A Tiiny";
    const where = VIA_WORDS[row.via] || row.via;
    const facts = el("div", { class: "m" },
      el("span", { text: `${row.address}, ${where}` }),
      row.serial ? el("span", { text: row.serial }) : null);
    box.append(el("div", { class: "approw", style: "margin-top:10px" },
      el("img", { src: "farm-mark.png", alt: "" }),
      el("div", {}, el("div", { class: "n", text: name }), facts),
      el("div", { class: "act" },
        el("button", { class: "btn small", type: "button", onclick: () => useTiiny(row) },
          document.createTextNode("Use this one")))));
  }
}

function useTiiny(row) {
  chosenBase = row.base;
  $("device-base").value = row.base;
  $("device-key-label").textContent = `API key for ${row.name || row.serial || row.address}`;
  $("device-key-step").hidden = false;
  $("device-key").focus();
}

async function saveDevice() {
  const base = ($("device-base").value || chosenBase || "").trim();
  const key = $("device-key").value;
  if (!base) { say("Pick a Tiiny, or type its address in yourself."); return; }
  if (!key.trim()) { say("Paste the key. It is in TiinyOS, under Settings, API Key."); return; }
  try {
    farm.device = await invoke("device_save", { base, key });
    $("device-key").value = "";
    say("Saved. The key is on this computer now, and it is not shown again.");
    await refresh();
  } catch (error) {
    say(sentence(error));
  }
}

$("device-save").addEventListener("click", saveDevice);
$("device-look").addEventListener("click", lookForTiinys);
$("device-manual-toggle").addEventListener("click", () => {
  const manual = $("device-manual");
  manual.hidden = !manual.hidden;
  if (!manual.hidden) {
    $("device-key-step").hidden = false;
    $("device-key-label").textContent = "API key for your Tiiny";
    $("device-base").focus();
  }
});
$("settings-device-change").addEventListener("click", () => {
  show("farm");
  $("device-banner").hidden = false;
  lookForTiinys();
});
$("catalog-retry").addEventListener("click", () => refresh());

// Settings ----------------------------------------------------------------
function renderSettings() {
  $("settings-device").textContent = farm.device.configured
    ? `The Tiiny on file is at ${farm.device.base}. The key is in ${farm.info.configDir}, at mode 0600.`
    : "No Tiiny is on file yet, so no app can reach one.";
  $("toggle-autostart").setAttribute("aria-checked", String(farm.settings.autostart));
  $("toggle-path").setAttribute("aria-checked", String(farm.settings.farmOnPath));
  $("toggle-browser").setAttribute("aria-checked", String(farm.settings.openInBrowser));

  const mb = (n) => `${(n / 1024 / 1024).toFixed(1)} MB`;
  const about = $("about");
  about.replaceChildren();
  const facts = [
    ["Launcher", farm.info.version],
    ["Engine", farm.info.farmFrom ? `farm ${farm.info.farm}, built from ${farm.info.farmFrom}` : `farm ${farm.info.farm}`],
    ["Python", `CPython ${farm.info.python}`],
    ["Interpreter", farm.info.pythonPath],
    ["Built for", farm.info.target],
    ["Runtime on disk", `${mb(farm.info.runtimeBytes)} in ${farm.info.runtimeFiles} files`],
    ["Apps", farm.info.appsDir],
    ["Settings", farm.info.configDir],
    ["Catalog", farm.info.catalog],
  ];
  for (const [term, value] of facts) {
    about.append(el("dt", { text: term }), el("dd", { text: value }));
  }
}

$("toggle-autostart").addEventListener("click", () => flip("autostart"));
$("toggle-path").addEventListener("click", () => flip("farmOnPath"));
$("toggle-browser").addEventListener("click", () => flip("openInBrowser"));

async function flip(key) {
  const next = { ...farm.settings, [key]: !farm.settings[key] };
  try {
    farm.settings = await invoke("settings_write", { next });
    if (key === "farmOnPath") {
      $("path-note").textContent = farm.settings.farmOnPath
        ? "Saved. Putting the command on the PATH lands in a later version; the switch remembers your answer."
        : "Adds the command line tool to your shell. Nothing here needs it.";
    }
    renderSettings();
  } catch (error) {
    say(sentence(error));
  }
}

// What the catalog has that this computer does not. The engine works it out;
// the window only says it, and the Update buttons in the Running pane come from
// the same comparison.
$("settings-updates").addEventListener("click", async () => {
  const box = $("doctor-findings");
  box.replaceChildren(el("p", { class: "lede", text: "Looking." }));
  try {
    const answer = await invoke("farm_check");
    box.replaceChildren();
    const updates = answer.updates || [];
    if (!updates.length) {
      box.append(el("p", { class: "lede", text: "Everything you have installed is the newest the catalog has." }));
    }
    for (const row of updates) {
      box.append(el("div", { class: "finding" },
        el("i", {}),
        el("div", {},
          el("div", { text: `${row.name} ${row.installed} can go to ${row.available}.` }),
          row.notes ? el("div", { class: "fix", text: row.notes }) : null)));
    }
    for (const id of answer.unreachable || []) {
      box.append(el("div", { class: "finding bad" },
        el("i", {}),
        el("div", {}, el("div", { text: `${id} is installed here and the catalog did not answer about it.` }))));
    }
    await refresh();
  } catch (error) {
    box.replaceChildren(el("p", { class: "lede", text: sentence(error) }));
  }
});

$("settings-doctor").addEventListener("click", async () => {
  const box = $("doctor-findings");
  box.replaceChildren(el("p", { class: "lede", text: "Checking." }));
  try {
    const answer = await invoke("farm_doctor");
    box.replaceChildren();
    for (const finding of answer.findings || []) {
      // A finding with nothing of its own to say carries its fix line as its
      // message, so that the JSON and the printed output say the same words.
      // Rendering both would say them twice.
      const repeats = finding.fix && finding.message === `Fix: ${finding.fix}`;
      box.append(el("div", { class: `finding${finding.ok ? "" : " bad"}` },
        el("i", {}),
        el("div", {},
          repeats ? null : el("div", { text: finding.message }),
          finding.fix ? el("div", { class: "fix", text: `Fix: ${finding.fix}` }) : null)));
    }
    if (!answer.findings || !answer.findings.length) box.append(el("p", { class: "lede", text: "Nothing to report." }));
  } catch (error) {
    box.replaceChildren(el("p", { class: "lede", text: sentence(error) }));
  }
});

// Reading everything again ------------------------------------------------
async function refresh() {
  try {
    const [listed, status, device] = await Promise.all([
      invoke("farm_list"),
      invoke("farm_status"),
      invoke("device_current"),
    ]);
    farm.catalog = listed.catalog || [];
    farm.installed = listed.installed || [];
    farm.running = status.running || [];
    farm.device = device;
  } catch (error) {
    $("catalog-empty-note").textContent = sentence(error);
    farm.catalog = [];
  }
  const wasHidden = $("device-banner").hidden;
  $("device-banner").hidden = farm.device.configured;
  // The first time the pane appears, start looking without being asked. The
  // search is also what makes macOS ask about the local network, and this is
  // the screen where that question makes sense.
  if (!farm.device.configured && wasHidden) lookForTiinys();
  lastRead = Date.now();
  renderCatalog();
  renderInstalled();
  renderSettings();
  // The card art and the badges need the whole manifest, which the list does
  // not carry. Fetched once each, after the grid is already on screen.
  const wanted = farm.catalog.filter((row) => !farm.manifests.has(row.id)).map((row) => row.id);
  if (wanted.length) {
    await Promise.all(wanted.map(async (id) => {
      try { farm.manifests.set(id, await invoke("farm_manifest", { id })); } catch { /* the grid still draws */ }
    }));
    renderCatalog();
    renderInstalled();
  }
}

listen("install:step", (event) => {
  const { id, line, fraction } = event.payload;
  const held = farm.working.get(id);
  if (held && held.fraction > fraction) return;
  farm.working.set(id, { line, fraction });
  if (farm.open === id) renderCardState();
  renderInstalled();
});

listen("apps:changed", () => { refresh(); });

listen("deep-link:install", (event) => { show("farm"); openCard(event.payload); });

// Boot ---------------------------------------------------------------------
(async () => {
  farm.info = await invoke("launcher_info");
  farm.settings = await invoke("settings_read");
  await refresh();
  const waiting = await invoke("take_deep_link");
  if (waiting) { show("farm"); openCard(waiting); }
  // The status of a running app moves on its own: it can become ready, or
  // stop. Once every eight seconds is often enough to notice and quiet enough
  // that nothing flickers.
  setInterval(async () => {
    if (farm.working.size) return;
    try {
      const status = await invoke("farm_status");
      farm.running = status.running || [];
      renderInstalled();
      if (farm.open) renderCardState();
    } catch { /* the next one will do */ }
  }, 8000);
})();
