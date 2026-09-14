#!/usr/bin/env node
// Drive the built app's engine against a real Tiiny, in a scratch home, and
// say what happened in sentences.
//
//   node scripts/verify-launcher.mjs \
//     --app "$HOME/Applications/Tiiny App Farm.app" \
//     --home /tmp/farm-gate \
//     --base http://172.17.7.177/v1 \
//     --key-file "$HOME/.tiiny_1_api_key" \
//     --app-id story-lantern
//
// --device-only stops after the device is saved, which is how a scratch home
// is prepared for something else to drive.
//
// The key is read by this process and written to the child's standard input.
// It is never an argument, never an environment variable, never printed and
// never in a log. That is the same path the launcher's own device pane takes,
// because it is the same command: `farm device --base <url> --key-stdin`.
//
// --home is not optional on purpose. A gate that runs against somebody's real
// ~/tiinyapps can install, start and remove their apps.

import { execFileSync, spawnSync } from "node:child_process";
import fs from "node:fs";
import path from "node:path";

const argv = process.argv.slice(2);
const value = (name, fallback = null) => {
  const at = argv.indexOf(name);
  return at === -1 ? fallback : argv[at + 1];
};

const app = value("--app");
const home = value("--home");
const base = value("--base");
const keyFile = value("--key-file");
const appId = value("--app-id", "story-lantern");

if (!app || !home) {
  process.stderr.write("Both --app and --home are needed. See the top of this file.\n");
  process.exit(2);
}

const python = path.join(app, "Contents", "Resources", "runtime", "bin", "python3.11");
if (!fs.existsSync(python)) {
  process.stderr.write(`No bundled interpreter at ${python}.\n`);
  process.exit(2);
}

fs.mkdirSync(home, { recursive: true });

const env = {
  ...process.env,
  HOME: home,
  USERPROFILE: home,
  PYTHONDONTWRITEBYTECODE: "1",
  PYTHONNOUSERSITE: "1",
  PYTHONUNBUFFERED: "1",
};
// Whatever the shell was carrying, the child must not pick a device up from it.
delete env.TIINY_BASE;
delete env.TIINY_KEY;

const log = (line) => process.stdout.write(line + "\n");

function farm(args, { stdin = null, timeout = 180_000 } = {}) {
  return spawnSync(python, ["-m", "farm.farm", "--no-update-check", ...args], {
    env,
    input: stdin ?? undefined,
    encoding: "utf8",
    timeout,
  });
}

function farmJson(args, options) {
  const done = farm([...args, "--json"], options);
  if (done.error) throw new Error(`farm ${args[0]} could not run: ${done.error.message}`);
  const raw = (done.stdout || "").trim();
  if (!raw) throw new Error(`farm ${args[0]} answered nothing. It said: ${(done.stderr || "").trim().split("\n").pop()}`);
  const answer = JSON.parse(raw);
  // A refusal is an object with an error in it. Reading the fields off it
  // anyway prints "port undefined" and hides the reason, so it is raised here
  // with the engine's own sentence.
  if (answer && answer.error) {
    throw new Error(answer.error.message || `farm ${args[0]} refused.`);
  }
  return { answer, commentary: (done.stderr || "").trim().split("\n").filter(Boolean), code: done.status };
}

const results = [];
const record = (what, ok, detail) => {
  results.push({ what, ok, detail });
  log(`${ok ? "yes" : "NO "}  ${what}${detail ? `: ${detail}` : ""}`);
};

// 0. The engine is inside the bundle and answers ------------------------
const version = execFileSync(python, ["-m", "farm.farm", "--version"], { env, encoding: "utf8" }).trim();
record("the engine answers from inside the bundle", version.startsWith("farm "), version);

// 1. The device, with the key going down standard input ------------------
if (base && keyFile) {
  const key = fs.readFileSync(keyFile, "utf8").trim();
  const done = farm(["device", "--base", base, "--key-stdin"], { stdin: key });
  // `key` goes out of scope here and is never written anywhere else.
  const saved = (done.stdout || "") + (done.stderr || "");
  record("the Tiiny's key is saved from standard input", done.status === 0, saved.trim().split("\n")[0] || "");
  const settings = path.join(home, ".tiinyapps", "device.json");
  if (fs.existsSync(settings)) {
    const mode = (fs.statSync(settings).mode & 0o777).toString(8);
    const onFile = JSON.parse(fs.readFileSync(settings, "utf8"));
    record("the device file is private", mode === "600", `mode ${mode}`);
    record("the address on file is the one asked for", onFile.base === base, onFile.base);
    // The key's presence is proved by its length and nothing else.
    record("the key is on file", typeof onFile.key === "string" && onFile.key.length > 0, `${String(onFile.key).length} characters, not shown`);
  } else {
    record("the device file was written", false, settings);
  }
}

if (argv.includes("--device-only")) {
  log("");
  log("The scratch home has a Tiiny on file and nothing installed.");
  process.exit(results.some((r) => !r.ok) ? 1 : 0);
}

// 2. The catalog ----------------------------------------------------------
const listed = farmJson(["list"]);
record("the catalog answers", Array.isArray(listed.answer.catalog) && listed.answer.catalog.length > 0,
  `${(listed.answer.catalog || []).length} apps`);

// 3. Install --------------------------------------------------------------
const installed = farmJson(["install", appId, "--yes"], { timeout: 900_000 });
record(`${appId} installs`, installed.answer.installed === true, `version ${installed.answer.version}`);
log("    the engine said:");
for (const line of installed.commentary) log(`      ${line}`);

// 4. Start ----------------------------------------------------------------
let started = null;
try {
  started = farmJson(["start", appId]);
  record(`${appId} starts`, started.answer.running === true, `port ${started.answer.port}, ${started.answer.url}`);
} catch (error) {
  record(`${appId} starts`, false, error.message);
}

// 5. Status, and whether it is answering ---------------------------------
try {
  const status = farmJson(["status"]);
  const row = (status.answer.running || []).find((r) => r.id === appId);
  record(`${appId} is in the status list`, Boolean(row), row ? `pid ${row.pid}, up ${row.uptime}s, health ${row.health ?? "no health path"}` : "");
} catch (error) {
  record(`${appId} is in the status list`, false, error.message);
}

// 6. Stop -----------------------------------------------------------------
try {
  const stopped = farmJson(["stop", appId]);
  record(`${appId} stops`, stopped.answer.stopped === true, "");
} catch (error) {
  record(`${appId} stops`, false, error.message);
}

log("");
const bad = results.filter((r) => !r.ok);
log(bad.length === 0 ? `All ${results.length} checks passed.` : `${bad.length} of ${results.length} checks failed.`);
// macOS grants local network access to an application, not to a file. The
// interpreter in the bundle has it when the launcher started it and does not
// have it when a terminal did, and since farm 0.1.14 a start asks the Tiiny
// what it has loaded. So this gate cannot finish those checks from a shell on
// macOS. It still fails rather than passing, because a skipped check that
// reads as a pass is worse than a failure somebody has to read.
if (bad.some((r) => /could not ask your Tiiny|local network/i.test(r.detail || ""))) {
  // Do not guess at the cause: ask the engine why it cannot see the device.
  const asked = farm(["models"]);
  const said = `${asked.stdout || ""}${asked.stderr || ""}`;
  if (/local network/i.test(said)) {
    log("");
    log(said.trim().split("\n").find((line) => /local network/i.test(line)) || "");
    log("That grant belongs to the launcher, not to the file: macOS gives local");
    log("network access to an application, and this interpreter only counts as");
    log("part of one when the launcher started it. Since farm 0.1.14 a start");
    log("asks the Tiiny what it has loaded, so the last three checks can only");
    log("be run by driving the built app itself.");
  }
}
process.exit(bad.length === 0 ? 0 : 1);
