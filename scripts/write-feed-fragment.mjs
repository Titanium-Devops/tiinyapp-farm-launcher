#!/usr/bin/env node
// One platform's row in latest.json, written beside the file it describes.
//
//   node scripts/write-feed-fragment.mjs windows-x86_64 <path to the installer>
//   node scripts/write-feed-fragment.mjs linux-x86_64   <path to the AppImage>
//
// The two Mac rows are written inside macos.yml, because both architectures
// come out of that one workflow and a feed naming one of them is a feed that
// silently never updates the other. Windows and Linux are separate workflows
// and each writes one fragment here; publish-release.mjs is the first place
// that has all three runs together, and it folds them into one feed.
//
// This is a script rather than a `node -e` one liner in each workflow because
// the last two one liners in these workflows both cost a release: an
// apostrophe inside a single quoted script on macOS, and a comment indented out
// of a `run: |` block. A file has neither problem and can be read.

import fs from "node:fs";
import path from "node:path";
import { fileURLToPath } from "node:url";

const root = path.join(path.dirname(fileURLToPath(import.meta.url)), "..");

const die = (said) => {
  process.stderr.write(`${said}\n`);
  process.exit(1);
};

const key = process.argv[2];
const artifact = process.argv[3];
if (!key || !artifact) die("Say which platform key, and which file.");
if (!/^(windows|linux|darwin)-[a-z0-9_]+$/.test(key)) die(`${key} is not a platform key the updater knows.`);
if (!fs.existsSync(artifact)) die(`${artifact} is not there.`);

const sigFile = `${artifact}.sig`;
if (!fs.existsSync(sigFile)) die(`${path.basename(artifact)} has no signature beside it.`);
const signature = fs.readFileSync(sigFile, "utf8").trim();
if (!signature) die(`${path.basename(sigFile)} is empty.`);

const version = JSON.parse(
  fs.readFileSync(path.join(root, "src-tauri", "tauri.conf.json"), "utf8"),
).version;

// One flat filename. The farm's worker refuses any launcher key with a slash in
// it before it ever looks in the bucket, and the version inside the name is
// what earns the object its one year cache. publish-release.mjs flattens the
// same way and refuses the fragment if the two disagree.
const name = path.basename(artifact).replace(/[^A-Za-z0-9._-]+/g, "-");
const url = `https://tiinyapp.farm/launcher/${name}`;

fs.mkdirSync(path.join(root, "feed"), { recursive: true });
fs.writeFileSync(
  path.join(root, "feed", `${key}.json`),
  JSON.stringify({ version, key, signature, url }, null, 2) + "\n",
);
process.stdout.write(`${key} -> ${name} at ${version}\n`);
