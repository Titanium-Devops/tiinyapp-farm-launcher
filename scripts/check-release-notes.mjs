#!/usr/bin/env node
// The version this repository is about to build has to be written down.
//
//   node scripts/check-release-notes.mjs
//
// Three files name the version and a fourth explains it. All four have to
// agree, because the release history on the site is built from the changelog:
// a version with no entry there ships a download nobody can read anything
// about, and a version that disagrees with its own Cargo.toml ships one file
// named after another.
//
// This is cheap to run and the thing it prevents is only ever noticed by
// somebody looking at a download page wondering what they are about to install.

import fs from "node:fs";
import path from "node:path";
import { fileURLToPath } from "node:url";

const root = path.join(path.dirname(fileURLToPath(import.meta.url)), "..");
const read = (file) => fs.readFileSync(path.join(root, file), "utf8");

const said = [];
const wrong = [];

const tauri = JSON.parse(read("src-tauri/tauri.conf.json")).version;
said.push(["src-tauri/tauri.conf.json", tauri]);
said.push(["package.json", JSON.parse(read("package.json")).version]);
const cargo = read("src-tauri/Cargo.toml").match(/^version\s*=\s*"([^"]+)"/m);
said.push(["src-tauri/Cargo.toml", cargo ? cargo[1] : null]);

for (const [file, version] of said) {
  if (version !== tauri) {
    wrong.push(`${file} says ${version ?? "nothing"} and src-tauri/tauri.conf.json says ${tauri}`);
  }
}

// The entry itself. A heading is enough to find it; the words under it are what
// the site shows, so an empty entry is as bad as a missing one.
const changelog = read("CHANGELOG.md");
const headings = [...changelog.matchAll(/^## +(\d+\.\d+\.\d+)(?: +- +(\S+))?\s*$/gm)];
const entry = headings.find((found) => found[1] === tauri);

if (!entry) {
  const known = headings.map((h) => h[1]).join(", ") || "none at all";
  wrong.push(
    `CHANGELOG.md has no entry for ${tauri}. It has ${known}. ` +
      `Add one before this builds: the release history page is made from that file.`,
  );
} else {
  if (!entry[2]) wrong.push(`The ${tauri} entry in CHANGELOG.md has no date after it.`);
  const from = entry.index + entry[0].length;
  const next = headings.find((h) => h.index > entry.index);
  const body = changelog.slice(from, next ? next.index : undefined).trim();
  if (body.length < 40) {
    wrong.push(`The ${tauri} entry in CHANGELOG.md is empty, so the download page would say nothing.`);
  }
  if (headings[0][1] !== tauri) {
    wrong.push(`CHANGELOG.md starts with ${headings[0][1]} and this is ${tauri}. Newest first.`);
  }
}

if (wrong.length) {
  for (const line of wrong) process.stderr.write(`${line}\n`);
  process.exit(1);
}
process.stdout.write(`${tauri} agrees across all three version fields and has an entry in CHANGELOG.md\n`);
