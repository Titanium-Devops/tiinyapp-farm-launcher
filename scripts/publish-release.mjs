#!/usr/bin/env node
// Take a signed run's artifacts and put them where people download them.
//
//   node scripts/publish-release.mjs --bucket <r2-bucket>            # dry run
//   node scripts/publish-release.mjs --bucket <r2-bucket> --publish
//
// The workflows build, sign, notarise and upload to GitHub Actions, and stop
// there. This is the step after: it finds the two runs for one commit, pulls
// what they made, refuses anything that is not signed and stapled, and puts the
// files in R2 under launcher/<version>/ with latest.json last.
//
// latest.json goes last on purpose. It is the file the updater reads, and a
// feed that names a download which is not there yet turns every launcher in the
// world into one that cannot update.
//
// What it cannot check, and says so rather than implying otherwise: the
// Authenticode signature on the Windows installer, which only Windows can
// verify. The Windows workflow will not upload an installer whose signature
// Windows itself did not call Valid, so that check has already happened on a
// machine that could do it.
//
// Needs on this Mac: gh, logged in with access to the repository; wrangler,
// logged in to the Cloudflare account that owns the bucket. No key, token or
// password is read from a file or an environment variable here.

import { execFileSync } from "node:child_process";
import crypto from "node:crypto";
import fs from "node:fs";
import os from "node:os";
import path from "node:path";
import { fileURLToPath } from "node:url";

const here = path.dirname(fileURLToPath(import.meta.url));
const repoRoot = path.join(here, "..");

const argv = process.argv.slice(2);
const value = (name, fallback = null) => {
  const at = argv.indexOf(name);
  return at === -1 ? fallback : argv[at + 1];
};
const flag = (name) => argv.includes(name);

const bucket = value("--bucket");
const prefix = value("--prefix", "launcher/");
const publish = flag("--publish");
const keep = value("--out", null);

if (!bucket) {
  process.stderr.write(
    "A bucket is not optional: --bucket <r2-bucket>. See the top of this file.\n",
  );
  process.exit(2);
}

const log = (line) => process.stdout.write(`${line}\n`);
const run = (cmd, args, options = {}) =>
  execFileSync(cmd, args, { encoding: "utf8", ...options });

// The commit to publish. Whatever is asked for has to be a commit that both
// workflows have already finished, so the default is the tip of main rather
// than whatever happens to be checked out.
let sha = value("--sha");
if (!sha) {
  run("git", ["fetch", "origin", "main"], { cwd: repoRoot, stdio: "ignore" });
  sha = run("git", ["rev-parse", "origin/main"], { cwd: repoRoot }).trim();
}
log(`commit   ${sha}`);

const version = JSON.parse(
  fs.readFileSync(path.join(repoRoot, "src-tauri", "tauri.conf.json"), "utf8"),
).version;
log(`version  ${version}`);

// --- The two runs ---------------------------------------------------------
// Both, or nothing. A macOS release without the Windows installer is a release
// that half the people who click Download cannot use.
function runFor(workflow) {
  const rows = JSON.parse(
    run("gh", [
      "run", "list",
      "--workflow", workflow,
      "--commit", sha,
      "--limit", "20",
      "--json", "databaseId,conclusion,status,headSha,event",
    ], { cwd: repoRoot }),
  );
  const done = rows.filter((row) => row.status === "completed");
  if (!done.length) {
    throw new Error(`${workflow} has not finished for ${sha.slice(0, 8)}. Nothing to publish yet.`);
  }
  const good = done.find((row) => row.conclusion === "success");
  if (!good) {
    throw new Error(`${workflow} did not succeed for ${sha.slice(0, 8)}: ${done[0].conclusion}.`);
  }
  return good.databaseId;
}

const macosRun = runFor("macos.yml");
const windowsRun = runFor("windows.yml");
log(`runs     macos ${macosRun}, windows ${windowsRun}`);

// --- Bring them down ------------------------------------------------------
const work = keep
  ? path.resolve(keep)
  : fs.mkdtempSync(path.join(os.tmpdir(), "launcher-release-"));
fs.mkdirSync(work, { recursive: true });
log(`into     ${work}`);

for (const id of [macosRun, windowsRun]) {
  run("gh", ["run", "download", String(id), "--dir", work], { cwd: repoRoot, stdio: "inherit" });
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
const one = (test, what) => {
  const hits = files.filter(test);
  if (hits.length !== 1) {
    throw new Error(`Expected exactly one ${what}, found ${hits.length}.`);
  }
  return hits[0];
};

const feedFiles = files.filter((f) => path.basename(f) === "latest.json");
if (!feedFiles.length) {
  throw new Error(
    "That run produced no latest.json, which means it produced no updater bundles either. " +
    "TAURI_SIGNING_PRIVATE_KEY is not set on the repository, so there is nothing for the " +
    "updater to read and nothing signed to publish. Set it and push to main again.",
  );
}
const feed = one((f) => path.basename(f) === "latest.json", "latest.json");
const installer = one((f) => f.endsWith(".exe"), "Windows installer");
const dmgs = files.filter((f) => f.endsWith(".dmg"));
if (dmgs.length !== 2) {
  throw new Error(`Expected two disk images, one per architecture, found ${dmgs.length}.`);
}
const tarballs = files.filter((f) => f.endsWith(".app.tar.gz"));
if (tarballs.length !== 2) {
  throw new Error(`Expected two updater bundles, found ${tarballs.length}. Without them the feed is a promise nothing can keep.`);
}

// --- Refuse anything that is not what it claims to be ---------------------
const sha256 = (file) =>
  crypto.createHash("sha256").update(fs.readFileSync(file)).digest("hex");

const answer = JSON.parse(fs.readFileSync(feed, "utf8"));
if (answer.version !== version) {
  throw new Error(`latest.json names ${answer.version} and this checkout is ${version}.`);
}
for (const key of ["darwin-aarch64", "darwin-x86_64"]) {
  const row = answer.platforms?.[key];
  if (!row) throw new Error(`latest.json does not name ${key}.`);
  const named = path.basename(new URL(row.url).pathname);
  const held = tarballs.find((f) => path.basename(f) === named);
  if (!held) {
    throw new Error(`latest.json points ${key} at ${named}, which this run did not produce.`);
  }
  const sig = `${held}.sig`;
  if (!fs.existsSync(sig)) throw new Error(`${named} has no signature beside it.`);
  if (fs.readFileSync(sig, "utf8").trim() !== row.signature.trim()) {
    throw new Error(`The signature in latest.json for ${key} is not the one in ${path.basename(sig)}.`);
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
    throw new Error(
      `${name} is not signed, notarised and stapled, so anyone who downloads it will be stopped by Gatekeeper. ` +
      `Set the Apple secrets and push to main before publishing. (${(error.stderr || error.message || "").toString().trim().split("\n").pop()})`,
    );
  }
  log(`macos    ${name} signed, notarised, stapled, ${sha256(dmg).slice(0, 16)}`);
}

// Windows: what can honestly be checked from here is that it is a PE file with
// bytes in it. Whether the Authenticode signature is valid was decided by
// Windows in the workflow, which refuses to upload an installer it called
// anything but Valid.
const head = fs.readFileSync(installer).subarray(0, 2).toString("latin1");
if (head !== "MZ" || fs.statSync(installer).size < 1_000_000) {
  throw new Error(`${path.basename(installer)} does not look like a Windows installer.`);
}
log(`windows  ${path.basename(installer)} ${sha256(installer).slice(0, 16)} (Authenticode checked by the workflow, not here)`);

// --- Put them where people download them ---------------------------------
const typeOf = (name) => {
  if (name.endsWith(".dmg")) return "application/x-apple-diskimage";
  if (name.endsWith(".exe")) return "application/vnd.microsoft.portable-executable";
  if (name.endsWith(".tar.gz")) return "application/gzip";
  if (name.endsWith(".sig")) return "text/plain";
  if (name.endsWith(".json")) return "application/json";
  return "application/octet-stream";
};

// Everything but the feed, then the feed. Nothing here is uploaded twice.
const payload = [...dmgs, installer, ...tarballs, ...tarballs.map((f) => `${f}.sig`)]
  .map((file) => ({ file, key: `${prefix}${version}/${path.basename(file)}` }));
payload.push({ file: feed, key: `${prefix}latest.json` });

for (const { file, key } of payload) {
  const name = path.basename(file);
  if (!publish) {
    log(`would    put ${key} (${(fs.statSync(file).size / 1e6).toFixed(1)} MB, ${typeOf(name)})`);
    continue;
  }
  run("wrangler", [
    "r2", "object", "put", `${bucket}/${key}`,
    "--file", file,
    "--content-type", typeOf(name),
    "--remote",
  ], { cwd: repoRoot, stdio: "inherit" });
  log(`put      ${key}`);
}

log("");
if (!publish) {
  log("Nothing was uploaded. Add --publish to do it for real.");
} else {
  log("Uploaded. The download links are:");
  for (const dmg of dmgs) log(`  https://tiinyapp.farm/${prefix}${version}/${path.basename(dmg)}`);
  log(`  https://tiinyapp.farm/${prefix}${version}/${path.basename(installer)}`);
  log(`  https://tiinyapp.farm/${prefix}latest.json`);
}
log("");
log("Those links only answer if the farm's worker serves that prefix from this");
log("bucket. It does not today: /launcher/ is a 404 while /seeds-files/<key>");
log("serves R2. That route is a change in the farm repository, not here.");
