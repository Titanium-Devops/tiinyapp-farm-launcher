#!/usr/bin/env node
// The parts of the window that only exist in the page.
//
//   node scripts/check-ui.mjs
//
// Most of what the window shows is decided in Rust and tested there, on
// purpose. What is left is the drawing, and one piece of it is worth a test of
// its own: the banner that says a newer launcher is ready. It appears on an
// event that arrives once every four hours at most, it has to go away and stay
// away when somebody says Not now, and the only way to see it before the next
// release is to fake the event. A test can do that in a second.
//
// ui/index.html and ui/app.js are loaded as they ship. Only the bridge is
// faked, so what this exercises is the real page.

import fs from "node:fs";
import path from "node:path";
import { fileURLToPath } from "node:url";

import { JSDOM } from "jsdom";

const root = path.join(path.dirname(fileURLToPath(import.meta.url)), "..");
const ui = path.join(root, "ui");

let failed = 0;
const ok = (said, truth) => {
  if (truth) {
    process.stdout.write(`  ok    ${said}\n`);
  } else {
    failed += 1;
    process.stderr.write(`  FAIL  ${said}\n`);
  }
};

// What the page asked the bridge for, so a test can say what it expects to have
// been asked rather than only what it expects to see.
const asked = [];
const listeners = new Map();

const ANSWERS = {
  launcher_info: {
    site: "https://tiinyapp.farm",
    python: "3.11.16",
    pythonPath: "/x",
    target: "aarch64-apple-darwin",
    runtimeBytes: 1,
    runtimeFiles: 1,
    appsDir: "/x",
    configDir: "/x",
    catalog: "https://tiinyapp.farm/manifests/",
  },
  settings_read: { autostart: false, farmOnPath: false, manualBase: null, openInBrowser: false },
  farm_list: { catalog: [], installed: [] },
  farm_status: { running: [] },
  device_current: { configured: true, base: "http://openai.api.tiiny/v1" },
  farm_models: { models: null, shortBy: {}, needs: {} },
  app_needs: { models: null, shortBy: {}, needs: {} },
  catalog_marks: {},
  social_counts: {},
  take_deep_link: null,
  models_watch: null,
  update_pending: null,
  update_dismiss: null,
  open_app_windows: [],
};

const dom = new JSDOM(fs.readFileSync(path.join(ui, "index.html"), "utf8"), {
  url: "http://localhost/",
  runScripts: "outside-only",
  pretendToBeVisual: true,
});
const { window } = dom;

// A dialog in jsdom has no showModal, and the card is opened with one. Nothing
// here opens a card, but the page holds a reference to it at load.
for (const dialog of window.document.querySelectorAll("dialog")) {
  dialog.showModal = function () { this.setAttribute("open", ""); };
  dialog.close = function () { this.removeAttribute("open"); };
}

window.__TAURI__ = {
  core: {
    invoke: (name, args) => {
      asked.push({ name, args });
      if (name === "update_install") return Promise.reject("The signature on that update is not ours.");
      if (name in ANSWERS) return Promise.resolve(ANSWERS[name]);
      return Promise.reject(new Error(`no stub for ${name}`));
    },
  },
  event: {
    listen: (name, handler) => {
      listeners.set(name, handler);
      return Promise.resolve(() => {});
    },
  },
};

window.eval(fs.readFileSync(path.join(ui, "app.js"), "utf8"));

const $ = (id) => window.document.getElementById(id);
const emit = (name, payload) => {
  const handler = listeners.get(name);
  if (!handler) throw new Error(`the page never listened for ${name}`);
  handler({ payload });
};
const settle = () => new Promise((done) => window.setTimeout(done, 0));

process.stdout.write("the update banner\n");
await settle();
await settle();

ok("is not there before anything says there is an update", $("update-banner").hidden === true);
ok("the page listens for the ready event", listeners.has("update:ready"));
ok("the page listens for the progress event", listeners.has("update:progress"));
ok("the page asks once at boot for anything already found", asked.some((a) => a.name === "update_pending"));

emit("update:ready", { version: "0.1.3", notes: "Tiiny App Farm 0.1.3", date: "2026-09-18T14:00:00Z" });
ok("appears when a version arrives", $("update-banner").hidden === false);
ok("names that version", $("update-line").textContent === "Tiiny App Farm 0.1.3 is ready.");
ok("says what the release said about itself", $("update-notes").textContent === "Tiiny App Farm 0.1.3");

// A release with nothing to say gets no empty line under the sentence.
emit("update:ready", { version: "0.1.4", notes: null, date: null });
ok("a release with no notes leaves no empty line", $("update-notes").hidden === true);
ok("and still names its version", $("update-line").textContent === "Tiiny App Farm 0.1.4 is ready.");

// The bar only moves once somebody has pressed the button.
emit("update:progress", 0.5);
ok("progress before the button is pressed draws nothing", $("update-bar").hidden === true);

// The download, while it is still going. The handler asks Rust before it
// awaits anything, so this is the real order: pressed, downloading, and only
// then whatever the answer turns out to be.
$("update-go").dispatchEvent(new window.Event("click"));
ok("pressing it asks Rust to install", asked.some((a) => a.name === "update_install"));
ok("and the button says what it is doing", $("update-go").textContent === "Downloading");
emit("update:progress", 0.42);
ok("the bar shows how far the download has got", $("update-bar-fill").style.width === "42%");
ok("and the bar is on screen", $("update-bar").hidden === false);

// The stub refuses with a signature failure, which has to come back in words
// and leave the button usable rather than a dead Downloading label.
await settle();
ok("a refused update is said in words", $("toast").textContent.includes("signature"));
ok("and the button goes back to what it said", $("update-go").textContent === "Update and restart");
ok("and is usable again", $("update-go").disabled === false);
ok("and the half drawn bar is taken away", $("update-bar").hidden === true);

// Progress arriving after the failure belongs to nothing and draws nothing.
emit("update:progress", 0.9);
ok("progress after it stopped draws nothing", $("update-bar").hidden === true);

$("update-later").dispatchEvent(new window.Event("click"));
await settle();
ok("Not now hides it", $("update-banner").hidden === true);
const dismissed = asked.filter((a) => a.name === "update_dismiss").pop();
ok("and tells Rust which version to keep quiet about", dismissed && dismissed.args.version === "0.1.4");

// The page keeps an eight second poll running and jsdom keeps a frame loop, so
// the process would sit here for ever waiting for a window nobody is looking
// at. Close it and say what happened.
window.close();

process.stdout.write("\n");
if (failed) {
  process.stderr.write(`${failed} failed\n`);
  process.exit(1);
}
process.stdout.write("the window draws the update banner the way it is meant to\n");
process.exit(0);
