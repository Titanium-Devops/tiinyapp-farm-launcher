// The About window.
//
// Everything it says about the app, it asked Rust for: the version is the one
// cargo compiled and the date is the one CHANGELOG.md gives that version, so
// there is no number in this window that somebody has to remember to change.
//
// Every link goes out to the person's own browser and never into this web view.
// The list of addresses that are allowed to be opened lives in
// src-tauri/src/about.rs and is enforced there; the page holds the same list
// only so that a link it should never have carried is refused here as well as
// there, and so scripts/check-ui.mjs can hold the two halves together.

const { invoke } = window.__TAURI__.core;

const $ = (id) => document.getElementById(id);

// The words the launcher used. An error that crosses the bridge as a plain
// string still reads like a sentence.
function sentence(error) {
  if (!error) return "Something went wrong and nothing said what.";
  if (typeof error === "string") return error;
  if (error.message) return error.message;
  return String(error);
}

// The line under the two buttons. It keeps its place in the layout whether or
// not it has anything in it, so an answer never pushes the credits about.
function answer(words, tone) {
  const line = $("answer");
  line.textContent = words;
  line.className = tone ? `answer ${tone}` : "answer";
}

// What the launcher will open. Empty until Rust has answered, which is the
// right way round: a click that beats the answer opens nothing rather than
// opening something unchecked.
let allowed = new Set();

for (const button of document.querySelectorAll("[data-open]")) {
  button.addEventListener("click", async () => {
    const url = button.getAttribute("data-open");
    if (!allowed.has(url)) {
      answer(`The launcher does not open ${url}.`, "bad");
      return;
    }
    try {
      await invoke("open_external", { url });
    } catch (error) {
      answer(sentence(error), "bad");
    }
  });
}

// Look for a newer launcher, because somebody asked rather than because four
// hours went by. The answer is said here in words; installing it is the
// banner's job, in the farm window, where the notes and the progress bar are.
let looking = false;
$("check").addEventListener("click", async () => {
  if (looking) return;
  looking = true;
  const button = $("check");
  button.disabled = true;
  answer("Asking the farm.");
  try {
    const found = await invoke("update_look");
    if (found) {
      answer(`Tiiny App Farm ${found.version} is ready. The farm window has the button that installs it.`, "good");
    } else {
      answer("You are on the newest version.", "good");
    }
  } catch (error) {
    answer(sentence(error), "bad");
  } finally {
    looking = false;
    button.disabled = false;
  }
});

// The launcher's own log, which only exists once something has been worth
// writing down. A launcher that has had nothing to say says so.
$("log").addEventListener("click", async () => {
  try {
    await invoke("open_log");
    answer("The log is showing in your file manager.", "good");
  } catch (error) {
    answer(sentence(error), "bad");
  }
});

// Escape closes it. The window closes itself in Rust rather than being handed
// the permission to close windows, which nothing else in this app needs.
window.addEventListener("keydown", (event) => {
  if (event.key === "Escape") invoke("about_close");
});

(async () => {
  const about = await invoke("about_info");
  allowed = new Set(about.links || []);
  $("version").textContent = about.released
    ? `Version ${about.version}, released ${about.released}`
    : `Version ${about.version}`;
  $("licence-name").textContent = `${about.license} licence`;
})();
