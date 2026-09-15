#!/usr/bin/env node
// Take a signed run's artifacts and put them where people download them.
//
//   node scripts/publish-release.mjs                    # dry run, tip of main
//   node scripts/publish-release.mjs --publish
//   node scripts/publish-release.mjs --sha <commit> --publish
//
// The workflows build, sign, notarise and stop at GitHub Actions artifacts.
// This is the step after. It finds the runs for one commit, pulls what they
// made, refuses anything that is not what it claims to be, and puts the files
// in the farm's R2 bucket under launcher/.
//
// Two workflows, so two runs: macos.yml and windows.yml both have to have
// succeeded for the same commit. A macOS release without the Windows installer
// is a release half the people who click Download cannot use.
//
// KEYS ARE FLAT. The farm's worker serves GET /launcher/<file> out of
// launcher/<file> in R2, and launcherType() in its worker/main.mjs refuses any
// name with a slash in it before it ever looks in the bucket. The version lives
// inside the filename, which is also what earns an object its one year cache;
// a name without a version is cached for five minutes, which is what the two
// stable download names want.
//
// Every installer goes up twice:
//   launcher/Tiiny-App-Farm_<version>_<arch>.dmg   immutable, for the feed
//   launcher/Tiiny-App-Farm.dmg                    stable, for the button
// The site's launcher.json takes one bare filename per platform, so the button
// needs a name that does not change between releases.
//
// latest.json goes last. It is the file every installed launcher reads, and a
// feed naming a download that is not there yet turns all of them into launchers
// that cannot update.
//
// What it cannot check, and says so rather than implying otherwise: the
// Authenticode signature on the Windows installer, which only Windows can
// verify. The Windows workflow refuses to upload an installer whose signature
// Windows itself called anything but Valid, so that has been checked on a
// machine that could do it, and what is checked here is that the bytes are the
// ones that machine hashed.
//
// Needs: gh, logged in. wrangler, logged in to the account that owns the
// bucket, which is checked with `wrangler whoami` before the first upload. No
// key, token or password is read, printed or copied by this script.

import { execFileSync } from "node:child_process";
import crypto from "node:crypto";
import fs from "node:fs";
import os from "node:os";
import path from "node:path";
import { fileURLToPath } from "node:url";

const here = path.dirname(fileURLToPath(import.meta.url));
const repoRoot = path.join(here, "..");

// wrangler is run from the farm's checkout, because that is where wrangler.toml
// and therefore the account and the bucket binding live.
const FARM = "/Users/sem/code/tiinyapp-farm-main";
const BUCKET = "farm-seeds";
const PREFIX = "launcher/";
const SITE = "https://tiinyapp.farm";
const STABLE_MAC = "Tiiny-App-Farm.dmg";
const STABLE_MAC_INTEL = "Tiiny-App-Farm-Intel.dmg";
const STABLE_WINDOWS = "Tiiny-App-Farm-Setup.exe";
const STABLE_LINUX = "Tiiny-App-Farm.AppImage";
const HISTORY = "releases.json";

const argv = process.argv.slice(2);
const value = (name, fallback = null) => {
  const at = argv.indexOf(name);
  return at === -1 ? fallback : argv[at + 1];
};
const flag = (name) => argv.includes(name);

const publish = flag("--publish");
const bucket = value("--bucket", BUCKET);
const farmDir = value("--farm", FARM);
const keep = value("--out", null);

const log = (line = "") => process.stdout.write(`${line}\n`);
const run = (cmd, args, options = {}) =>
  execFileSync(cmd, args, { encoding: "utf8", ...options });
const die = (message) => {
  process.stderr.write(`\n${message}\n`);
  process.exit(1);
};

// A flat name the worker will serve: no spaces, no slashes.
const flatten = (name) => name.replace(/[^A-Za-z0-9._-]+/g, "-");

// --- The commit, and the two runs ----------------------------------------
let sha = value("--sha");
if (!sha) {
  run("git", ["fetch", "origin", "main"], { cwd: repoRoot, stdio: "ignore" });
  sha = run("git", ["rev-parse", "origin/main"], { cwd: repoRoot }).trim();
}
const version = JSON.parse(
  fs.readFileSync(path.join(repoRoot, "src-tauri", "tauri.conf.json"), "utf8"),
).version;
log(`commit   ${sha}`);
log(`version  ${version}`);

function runFor(workflow, override) {
  if (override) return override;
  const rows = JSON.parse(
    run("gh", ["run", "list", "--workflow", workflow, "--commit", sha, "--limit", "20",
      "--json", "databaseId,conclusion,status"], { cwd: repoRoot }),
  );
  const done = rows.filter((row) => row.status === "completed");
  if (!done.length) die(`${workflow} has not finished for ${sha.slice(0, 8)}. Nothing to publish yet.`);
  const good = done.find((row) => row.conclusion === "success");
  if (!good) die(`${workflow} did not succeed for ${sha.slice(0, 8)}: ${done[0].conclusion}.`);
  return String(good.databaseId);
}

const macosRun = runFor("macos.yml", value("--macos-run"));
const windowsRun = runFor("windows.yml", value("--windows-run"));
const linuxRun = runFor("linux.yml", value("--linux-run"));
log(`runs     macos ${macosRun}, windows ${windowsRun}, linux ${linuxRun}`);

// --- Bring them down ------------------------------------------------------
const work = keep ? path.resolve(keep) : fs.mkdtempSync(path.join(os.tmpdir(), "launcher-release-"));
fs.mkdirSync(work, { recursive: true });
log(`into     ${work}`);
for (const id of [macosRun, windowsRun, linuxRun]) {
  run("gh", ["run", "download", id, "--dir", work], { cwd: repoRoot, stdio: "inherit" });
}

const find = (dir) => {
  const out = [];
  const walk = (at) => {
    for (const entry of fs.readdirSync(at, { withFileTypes: true })) {
      const full = path.join(at, entry.name);
      if (entry.isDirectory()) walk(full);
      else out.push(full);
    }
  };
  walk(dir);
  return out;
};
const files = find(work);
const sha256 = (file) => crypto.createHash("sha256").update(fs.readFileSync(file)).digest("hex");

// --- What the run said the bytes are -------------------------------------
// Every SHA256SUMS the runs wrote, merged by filename. A file with no recorded
// hash is a file nobody vouched for.
const recorded = new Map();
for (const sums of files.filter((f) => path.basename(f) === "SHA256SUMS")) {
  for (const line of fs.readFileSync(sums, "utf8").split("\n")) {
    const found = line.trim().match(/^([0-9a-f]{64})\s+\*?(.+)$/i);
    if (found) recorded.set(found[2].trim(), found[1].toLowerCase());
  }
}
if (!recorded.size) {
  die(
    "Neither run recorded a SHA256SUMS, so there is nothing to check the downloaded bytes\n" +
    "against. That file is written by the workflows; a run from before they did will not have\n" +
    "one. Push to main again and publish that run.",
  );
}

const vouched = (file) => {
  const name = path.basename(file);
  const want = recorded.get(name);
  if (!want) die(`${name} has no recorded hash from the run that built it.`);
  const got = sha256(file);
  if (got !== want) die(`${name} does not match what the run recorded.\n  run:  ${want}\n  here: ${got}`);
  return got;
};

// --- Sort out what we have -----------------------------------------------
const feedFiles = files.filter((f) => path.basename(f) === "latest.json");
if (!feedFiles.length) {
  die(
    "That run produced no latest.json, which means it produced no updater bundles either.\n" +
    "TAURI_SIGNING_PRIVATE_KEY is not set on the repository, so there is nothing for the\n" +
    "updater to read and nothing signed to publish. Set it and push to main again.",
  );
}
const feed = feedFiles[0];
const dmgs = files.filter((f) => f.endsWith(".dmg"));
const installers = files.filter((f) => f.endsWith(".exe"));
const tarballs = files.filter((f) => f.endsWith(".app.tar.gz"));
if (dmgs.length !== 2) die(`Expected two disk images, one per architecture, found ${dmgs.length}.`);
if (installers.length !== 1) die(`Expected one Windows installer, found ${installers.length}.`);
if (tarballs.length !== 2) die(`Expected two updater bundles, found ${tarballs.length}.`);
const installer = installers[0];
const appImages = files.filter((f) => f.endsWith(".AppImage"));
if (appImages.length !== 1) die(`Expected one Linux AppImage, found ${appImages.length}.`);
const appImage = appImages[0];
const arm = dmgs.find((f) => /aarch64|arm64/.test(path.basename(f)));
const intel = dmgs.find((f) => f !== arm);
if (!arm || !intel) die("Could not tell the two disk images apart by architecture.");

// --- Refuse anything that is not what it claims to be --------------------
const answer = JSON.parse(fs.readFileSync(feed, "utf8"));
if (answer.version !== version) {
  die(`latest.json names ${answer.version} and this checkout is ${version}.`);
}
for (const key of ["darwin-aarch64", "darwin-x86_64"]) {
  const row = answer.platforms?.[key];
  if (!row) die(`latest.json does not name ${key}.`);
  const named = path.basename(new URL(row.url).pathname);
  if (new URL(row.url).pathname !== `/launcher/${named}`) {
    die(`latest.json points ${key} at ${row.url}, and the farm's worker only serves one flat name under /launcher/.`);
  }
  const held = tarballs.find((f) => path.basename(f) === named);
  if (!held) die(`latest.json points ${key} at ${named}, which this run did not produce.`);
  const sig = `${held}.sig`;
  if (!fs.existsSync(sig)) die(`${named} has no signature beside it.`);
  if (fs.readFileSync(sig, "utf8").trim() !== String(row.signature).trim()) {
    die(`The signature in latest.json for ${key} is not the one in ${path.basename(sig)}.`);
  }
  log(`feed     ${key} -> ${named}, signature matches`);
}

for (const dmg of dmgs) {
  const name = path.basename(dmg);
  try {
    run("codesign", ["--verify", "--strict", "--verbose=1", dmg], { stdio: "pipe" });
    run("spctl", ["-a", "-t", "open", "--context", "context:primary-signature", "-v", dmg], { stdio: "pipe" });
    run("xcrun", ["stapler", "validate", dmg], { stdio: "pipe" });
  } catch (error) {
    die(
      `${name} is not signed, notarised and stapled, so anyone who downloads it is stopped by\n` +
      `Gatekeeper. Set the Apple secrets and push to main before publishing.\n  ` +
      `${(error.stderr || error.message || "").toString().trim().split("\n").pop()}`,
    );
  }
  log(`macos    ${name} signed, notarised, stapled, sha256 ${vouched(dmg).slice(0, 16)}`);
}

const head = fs.readFileSync(installer).subarray(0, 2).toString("latin1");
if (head !== "MZ") die(`${path.basename(installer)} is not a Windows executable.`);

// Whether that run signed the installer is not a guess: the workflow's Verify
// signature step only runs when it did, so a skipped step is an unsigned
// installer. Windows is the only thing that can check an Authenticode
// signature, so what is checked here either way is that the bytes are the ones
// the runner hashed.
let windowsSigned = false;
try {
  const steps = JSON.parse(
    run("gh", ["run", "view", windowsRun, "--json", "jobs"], { cwd: repoRoot }),
  ).jobs.flatMap((job) => job.steps || []);
  const verify = steps.find((step) => step.name === "Verify signature");
  windowsSigned = Boolean(verify) && verify.conclusion === "success";
} catch {
  // Asked and could not be told. Treated as unsigned, which is the answer that
  // makes somebody look rather than the one that lets it through quietly.
  windowsSigned = false;
}
const windowsWord = windowsSigned ? "Authenticode Valid" : "UNSIGNED";
log(`windows  ${path.basename(installer)} sha256 ${vouched(installer).slice(0, 16)} (${windowsWord})`);
if (!windowsSigned) {
  log("");
  log("         The Windows installer is UNSIGNED. AZURE_CLIENT_ID is not set, so that run");
  log("         built it without an Authenticode signature. Windows will warn anybody who");
  log("         runs it. It is uploaded anyway, because an unsigned installer is better");
  log("         than no installer, but it is not what should stay up.");
}
// An AppImage is one ELF file that has to be executable to be anything. There
// is no signature to check: Linux has no notary and nothing a stranger's
// machine would check a desktop signature against, so the checksum published
// beside it is what somebody can verify, and it goes in the release history.
const elf = fs.readFileSync(appImage).subarray(0, 4);
if (elf[0] !== 0x7f || elf.subarray(1, 4).toString("latin1") !== "ELF") {
  die(`${path.basename(appImage)} is not an ELF executable.`);
}
log(`linux    ${path.basename(appImage)} sha256 ${vouched(appImage).slice(0, 16)} (UNSIGNED, which is what Linux has)`);
for (const tarball of tarballs) log(`updater  ${path.basename(tarball)}`);

// --- Put them where people download them ---------------------------------
const versioned = (file) => flatten(path.basename(file));

// What the release history will say about each file. The versioned name is the
// one that goes in it, because a stable name is overwritten by the next release
// and a history that points at stable names is a history of one release.
const shipped = {
  "mac-arm64": { file: arm, signed: true, notarised: true },
  "mac-x64": { file: intel, signed: true, notarised: true },
  "windows-x64": { file: installer, signed: windowsSigned, notarised: false },
  "linux-x64": { file: appImage, signed: false, notarised: false },
};

const uploads = [
  // Immutable, version in the name, what the feed and the history point at.
  ...Object.values(shipped).map(({ file }) => ({ file, key: `${PREFIX}${versioned(file)}` })),
  ...tarballs.map((f) => ({ file: f, key: `${PREFIX}${path.basename(f)}` })),
  ...tarballs.map((f) => ({ file: `${f}.sig`, key: `${PREFIX}${path.basename(f)}.sig` })),
  // Stable, no version, what the download buttons point at. Five minute cache.
  { file: arm, key: `${PREFIX}${STABLE_MAC}` },
  { file: intel, key: `${PREFIX}${STABLE_MAC_INTEL}` },
  { file: installer, key: `${PREFIX}${STABLE_WINDOWS}` },
  { file: appImage, key: `${PREFIX}${STABLE_LINUX}` },
  // Last, always.
  { file: feed, key: `${PREFIX}latest.json` },
];

// --- The release history -------------------------------------------------
// One object per release, newest first, so the site can offer every older
// executable rather than only the newest. It is read from the bucket, added to,
// and written back: an entry that is already there is never dropped, because
// the only copy of what 0.1.0 shipped is the one in that file.

// The entry's notes are the changelog, so there is one place where a release is
// described and the download page reads it rather than repeating it.
function notesFor(wanted) {
  const changelog = fs.readFileSync(path.join(repoRoot, "CHANGELOG.md"), "utf8");
  const headings = [...changelog.matchAll(/^## +(\d+\.\d+\.\d+)(?: +- +(\S+))?\s*$/gm)];
  const at = headings.findIndex((found) => found[1] === wanted);
  if (at === -1) return null;
  const from = headings[at].index + headings[at][0].length;
  const next = headings[at + 1];
  return changelog.slice(from, next ? next.index : undefined).trim() || null;
}

function olderIsFirst(a, b) {
  const parts = (v) => v.split(".").map((n) => Number.parseInt(n, 10) || 0);
  const [x, y] = [parts(b.version), parts(a.version)];
  for (let i = 0; i < 3; i += 1) if (x[i] !== y[i]) return x[i] - y[i];
  return 0;
}

function history() {
  // What the bucket already has. A missing file is the first publish since this
  // existed, and then the seed is where the history starts.
  let existing = null;
  try {
    const got = run("npx", ["wrangler", "r2", "object", "get", `${bucket}/${PREFIX}${HISTORY}`,
      "--remote", "--pipe"], { cwd: farmDir, stdio: ["ignore", "pipe", "pipe"] });
    existing = JSON.parse(got);
    if (!Array.isArray(existing)) die(`${PREFIX}${HISTORY} in the bucket is not a list. Look at it before publishing over it.`);
  } catch (error) {
    // Only one failure is allowed to be quiet: the object is not there yet.
    // Anything else, a network error, a permission problem, a half written
    // file, must stop, because carrying on would replace every older release
    // with a history that starts today.
    const said = `${error.stderr || ""}${error.stdout || ""}${error.message || ""}`;
    if (!/specified key does not exist/i.test(said)) {
      die(
        `Could not read ${PREFIX}${HISTORY} from the bucket, and it is not a missing file.\n` +
        `Publishing now would drop every release already in it.\n  ${said.trim().split("\n").slice(-3).join("\n  ")}`,
      );
    }
    existing = null;
  }
  if (!Array.isArray(existing)) {
    const seed = path.join(repoRoot, "docs", "releases-seed.json");
    existing = fs.existsSync(seed) ? JSON.parse(fs.readFileSync(seed, "utf8")) : [];
    log(`history  nothing in the bucket yet, starting from ${existing.length} seeded entr${existing.length === 1 ? "y" : "ies"}`);
  } else {
    log(`history  ${existing.length} release${existing.length === 1 ? "" : "s"} already recorded`);
  }

  const mine = {
    version,
    date: new Date().toISOString().slice(0, 10),
    commit: sha,
    notes: notesFor(version),
    files: Object.fromEntries(Object.entries(shipped).map(([key, { file, signed, notarised }]) => [key, {
      name: versioned(file),
      size: fs.statSync(file).size,
      sha256: sha256(file),
      signed,
      notarised,
    }])),
  };

  // Replace this version if it is already there, keep every other entry, and
  // refresh the notes of the older ones from the changelog so one file remains
  // the only place a release is described.
  const kept = existing.filter((row) => row && row.version !== version);
  for (const row of kept) {
    const notes = notesFor(row.version);
    if (notes) row.notes = notes;
  }
  const all = [mine, ...kept].sort(olderIsFirst);
  log(`history  ${all.length} release${all.length === 1 ? "" : "s"} after this one: ${all.map((r) => r.version).join(", ")}`);
  return all;
}

const releases = history();
const historyFile = path.join(work, HISTORY);
fs.writeFileSync(historyFile, JSON.stringify(releases, null, 2) + "\n");
// Before latest.json, after everything it describes.
uploads.splice(uploads.length - 1, 0, { file: historyFile, key: `${PREFIX}${HISTORY}` });

if (publish) {
  const who = run("npx", ["wrangler", "whoami"], { cwd: farmDir });
  if (!/Account ID/i.test(who)) {
    die("wrangler is not logged in from the farm checkout. Do not log it in from here; say so and stop.");
  }
  log(`wrangler ok, from ${farmDir}`);
}

log();
for (const { file, key } of uploads) {
  if (!fs.existsSync(file)) die(`${file} is missing.`);
  const size = (fs.statSync(file).size / 1e6).toFixed(1);
  if (!publish) {
    log(`would    put ${key}  (${size} MB)`);
    continue;
  }
  run("npx", ["wrangler", "r2", "object", "put", `${bucket}/${key}`, "--file", file, "--remote"],
    { cwd: farmDir, stdio: "inherit" });
  log(`put      ${key}`);
}

// --- Say what somebody can now click -------------------------------------
const links = [
  `${SITE}/launcher/${STABLE_MAC}`,
  `${SITE}/launcher/${STABLE_WINDOWS}`,
  `${SITE}/launcher/${STABLE_LINUX}`,
  `${SITE}/launcher/${HISTORY}`,
  `${SITE}/launcher/latest.json`,
];
log();
if (!publish) {
  log("Nothing was uploaded. Add --publish to do it for real.");
  log("These are the links it would make:");
  for (const link of links) log(`  ${link}`);
} else {
  log("Live now:");
  for (const link of links) {
    let status = "no answer";
    try {
      status = run("curl", ["-s", "-o", "/dev/null", "-w", "%{http_code}", "-L", link]).trim();
    } catch { /* the line below says what happened */ }
    log(`  ${status}  ${link}`);
  }
  log(`  and the Intel disk image at ${SITE}/launcher/${STABLE_MAC_INTEL}`);
}

log();
log("site/launcher.json, for the farm repository:");
log(JSON.stringify({
  enabled: true,
  version,
  mac: STABLE_MAC,
  macIntel: STABLE_MAC_INTEL,
  windows: STABLE_WINDOWS,
  linux: STABLE_LINUX,
}, null, 2));
log();
log(`The history for the older versions page is /launcher/${HISTORY}, newest first,`);
log("one object per release with its notes and every file it shipped.");
if (!windowsSigned) {
  log();
  log("Say somewhere on the page that the Windows installer is not signed yet, or the");
  log("first thing a stranger sees is a warning nobody told them to expect.");
}
