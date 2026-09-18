#!/usr/bin/env node
// Does this signature really belong to these bytes?
//
//   node scripts/verify-updater-signature.mjs <file> [<file>.sig]
//
// The updater refuses an update whose signature does not verify, which is the
// right behaviour and a terrible way to find out. The failure is silent until
// somebody presses Update, it looks identical to a wrong key, and on Windows it
// is easy to cause by accident: Authenticode rewrites the installer after the
// bundler has already signed it, so a signature made at bundle time is a
// signature over bytes nobody will ever download.
//
// So this checks, on the machine that made the file, against the public key the
// shipped app actually carries. No dependencies: Tauri's signatures are
// minisign, which is Ed25519 over either the file or its BLAKE2b-512 digest,
// and node can do both.

import crypto from "node:crypto";
import fs from "node:fs";
import path from "node:path";
import { fileURLToPath } from "node:url";

const root = path.join(path.dirname(fileURLToPath(import.meta.url)), "..");

const die = (said) => {
  process.stderr.write(`${said}\n`);
  process.exit(1);
};

const file = process.argv[2];
const sigFile = process.argv[3] || `${file}.sig`;
if (!file) die("Say which file to check.");
if (!fs.existsSync(file)) die(`${file} is not there.`);
if (!fs.existsSync(sigFile)) die(`${sigFile} is not there, so there is nothing to check against.`);

// The public key the built app carries. Checking against anything else would
// prove nothing: this is the one every installed copy will use.
const pubkeyField = JSON.parse(
  fs.readFileSync(path.join(root, "src-tauri", "tauri.conf.json"), "utf8"),
).plugins?.updater?.pubkey;
if (!pubkeyField) die("src-tauri/tauri.conf.json has no plugins.updater.pubkey.");

// Both files are a base64 blob whose contents are a minisign file: a comment
// line, a base64 payload line, and for a signature two more lines.
const unwrap = (blob) => Buffer.from(blob.trim(), "base64").toString("utf8");
const payloadLine = (text, which) => {
  const lines = text.split("\n").filter((line) => line.trim().length);
  const found = lines.filter((line) => !line.startsWith("untrusted comment:") && !line.startsWith("trusted comment:"));
  if (!found[which]) die("That does not look like a minisign file.");
  return Buffer.from(found[which].trim(), "base64");
};

const pub = payloadLine(unwrap(pubkeyField), 0);
if (pub.length !== 42) die(`The public key is ${pub.length} bytes and a minisign key is 42.`);
const pubAlgorithm = pub.subarray(0, 2).toString("latin1");
const pubKeyId = pub.subarray(2, 10);
const rawKey = pub.subarray(10);

const sig = payloadLine(unwrap(fs.readFileSync(sigFile, "utf8")), 0);
if (sig.length !== 74) die(`The signature is ${sig.length} bytes and a minisign signature is 74.`);
const algorithm = sig.subarray(0, 2).toString("latin1");
const sigKeyId = sig.subarray(2, 10);
const signature = sig.subarray(10);

if (!pubKeyId.equals(sigKeyId)) {
  die(
    `This signature was made with a different key.\n` +
    `  the app trusts key ${pubKeyId.toString("hex")} (${pubAlgorithm})\n` +
    `  this signature is  ${sigKeyId.toString("hex")} (${algorithm})\n` +
    `Every update signed with it would be refused by every installed app.`,
  );
}

// "ED" signs the BLAKE2b-512 digest of the file; the older "Ed" signs the file
// itself. Tauri writes the first, and reading the two bytes rather than
// assuming is what makes this worth keeping.
const bytes = fs.readFileSync(file);
let message;
if (algorithm === "ED") {
  message = crypto.createHash("blake2b512").update(bytes).digest();
} else if (algorithm === "Ed") {
  message = bytes;
} else {
  die(`This signature says it uses algorithm ${algorithm}, which is not one minisign makes.`);
}

// An Ed25519 public key node will take, built round the 32 raw bytes.
const key = crypto.createPublicKey({
  key: Buffer.concat([Buffer.from("302a300506032b6570032100", "hex"), rawKey]),
  format: "der",
  type: "spki",
});

if (!crypto.verify(null, message, key, signature)) {
  die(
    `The signature in ${path.basename(sigFile)} does not belong to ${path.basename(file)}.\n` +
    `If these bytes were signed and then changed, for example by Authenticode signing\n` +
    `an installer after the bundler signed it, this is exactly what that looks like.`,
  );
}

const size = (bytes.length / 1e6).toFixed(1);
process.stdout.write(
  `${path.basename(file)} (${size} MB) is signed by key ${pubKeyId.toString("hex")}, ${algorithm}, and the signature verifies\n`,
);
