#!/usr/bin/env node
// Stage the Python the launcher carries inside itself.
//
// Downloads the pinned astral-sh/python-build-standalone build, refuses it
// unless the bytes match the checksum in scripts/runtime.pins.json, prunes
// everything a farm app cannot use, installs the pinned tiinyapp-farm into it,
// and leaves the result in src-tauri/runtime/.
//
//   node scripts/stage-runtime.mjs                     the host target
//   node scripts/stage-runtime.mjs --target x86_64-apple-darwin
//   node scripts/stage-runtime.mjs --keep-dylib
//
// lib/libpython3.11.dylib is 18 MB and nothing in the pruned tree links against
// it: python-build-standalone links the interpreter statically, and neither
// remaining extension module references it. It is dropped by default. An app
// that embeds Python rather than running python3 would want it back, and no app
// in the catalog does that today; --keep-dylib is how it comes back if one ever
// does. See docs/LAUNCHER-1-REPORT.md.

import { createHash } from "node:crypto";
import { execFileSync } from "node:child_process";
import fs from "node:fs";
import path from "node:path";
import { fileURLToPath } from "node:url";

const here = path.dirname(fileURLToPath(import.meta.url));
const root = path.resolve(here, "..");
const pins = JSON.parse(fs.readFileSync(path.join(here, "runtime.pins.json"), "utf8"));

const argv = process.argv.slice(2);
const flag = (name) => argv.includes(name);
const value = (name) => {
  const at = argv.indexOf(name);
  return at === -1 ? null : argv[at + 1];
};

function hostTarget() {
  if (process.platform === "darwin") {
    return process.arch === "x64" ? "x86_64-apple-darwin" : "aarch64-apple-darwin";
  }
  if (process.platform === "win32") return "x86_64-pc-windows-msvc";
  throw new Error(
    `The launcher is a macOS and Windows app; ${process.platform} has no pinned runtime. ` +
      `Linux users already have pip install tiinyapp-farm.`,
  );
}

const target = value("--target") || hostTarget();
const pin = pins.python.targets[target];
if (!pin) {
  throw new Error(`No runtime is pinned for ${target}. The pinned targets are ${Object.keys(pins.python.targets).join(", ")}.`);
}

const windows = target.includes("windows");
const out = path.join(root, "src-tauri", "runtime");
const cache = path.join(root, "build", "cache");
const work = path.join(root, "build", "stage-" + target);

// Everything the page decided to drop: tcl, tk, idlelib, tkinter, headers and
// caches. Paths are relative to the unpacked python/ root, and a name that is
// not there is not an error, because the runtime's layout moves between
// releases (3.11.16 ships tcl9 where 3.11.13 shipped tcl8).
const PRUNE_GLOBS = [
  "include",
  "share",
  "lib/pkgconfig",
  "lib/tcl*",
  "lib/tk*",
  "lib/itcl*",
  "lib/thread3.*",
  "lib/libtcl9thread*",
  "lib/libtcl*",
  "lib/libtk*",
  "lib/python3.11/idlelib",
  "lib/python3.11/tkinter",
  "lib/python3.11/turtledemo",
  "lib/python3.11/lib-dynload/_tkinter*",
  // Windows lays the same tree out flat.
  "Lib/idlelib",
  "Lib/tkinter",
  "Lib/turtledemo",
  "tcl",
  "DLLs/_tkinter*",
  "DLLs/tcl*",
  "DLLs/tk*",
];

function log(line) {
  process.stdout.write(line + "\n");
}

function rm(target_) {
  fs.rmSync(target_, { recursive: true, force: true });
}

// Every segment of a prune pattern is matched against the names a directory
// actually has, case sensitively, on both platforms.
//
// This used to lean on fs.existsSync for a segment with no wildcard in it, and
// that is wrong on Windows, where the filesystem is case insensitive: "lib"
// found "Lib", and then "thread*" inside it matched Lib/threading.py and
// deleted the standard library's threading module. pip failed on the next line
// with "No module named 'threading'", which is a long way from saying a prune
// pattern was too greedy. GitHub Actions found it; this Mac never could.
function expand(base, pattern) {
  let found = [base];
  for (const part of pattern.split("/")) {
    const next = [];
    const rx = new RegExp(
      "^" + part.split("*").map((s) => s.replace(/[.*+?^${}()|[\]\\]/g, "\\$&")).join(".*") + "$",
    );
    for (const dir of found) {
      let entries;
      try {
        entries = fs.readdirSync(dir);
      } catch {
        continue; // not a directory, or not there
      }
      for (const name of entries) if (rx.test(name)) next.push(path.join(dir, name));
    }
    found = next;
  }
  return found;
}

function dropCaches(dir) {
  let dropped = 0;
  const walk = (at) => {
    for (const entry of fs.readdirSync(at, { withFileTypes: true })) {
      const full = path.join(at, entry.name);
      if (entry.isDirectory()) {
        if (entry.name === "__pycache__") {
          rm(full);
          dropped += 1;
        } else {
          walk(full);
        }
      }
    }
  };
  walk(dir);
  return dropped;
}

// How many Mach-O files are in the tree, by reading the first four bytes of
// each rather than shelling out to file(1).
//
// It goes in runtime.json so that the signing script and the workflow both
// check against what was actually staged. A number written down in a workflow
// goes stale the first time a runtime release changes what it ships, and the
// failure that causes is an unsigned file inside a signed bundle, which is the
// exact thing this app is shaped around.
const MACH_O_MAGIC = new Set([0xfeedface, 0xfeedfacf, 0xcefaedfe, 0xcffaedfe, 0xcafebabe, 0xbebafeca]);

function machOCount(dir) {
  let found = 0;
  const head = Buffer.alloc(4);
  const walk = (at) => {
    for (const entry of fs.readdirSync(at, { withFileTypes: true })) {
      const full = path.join(at, entry.name);
      if (entry.isSymbolicLink()) continue;
      if (entry.isDirectory()) {
        walk(full);
        continue;
      }
      let fd;
      try {
        fd = fs.openSync(full, "r");
        if (fs.readSync(fd, head, 0, 4, 0) === 4 && MACH_O_MAGIC.has(head.readUInt32BE(0))) found += 1;
      } catch {
        // Unreadable is not Mach-O for this purpose.
      } finally {
        if (fd !== undefined) fs.closeSync(fd);
      }
    }
  };
  walk(dir);
  return found;
}

function measure(dir) {
  let bytes = 0;
  let files = 0;
  const walk = (at) => {
    for (const entry of fs.readdirSync(at, { withFileTypes: true })) {
      const full = path.join(at, entry.name);
      if (entry.isSymbolicLink()) {
        files += 1;
      } else if (entry.isDirectory()) {
        walk(full);
      } else {
        files += 1;
        bytes += fs.statSync(full).size;
      }
    }
  };
  walk(dir);
  return { bytes, files };
}

async function fetchAsset() {
  fs.mkdirSync(cache, { recursive: true });
  const archive = path.join(cache, pin.asset);
  if (fs.existsSync(archive) && digest(archive) === pin.sha256) {
    log(`cached   ${pin.asset}`);
    return archive;
  }
  const url = pins.python.source + pin.asset;
  log(`download ${url}`);
  const response = await fetch(url, { redirect: "follow" });
  if (!response.ok) throw new Error(`The runtime download answered ${response.status}. Nothing was written.`);
  fs.writeFileSync(archive, Buffer.from(await response.arrayBuffer()));
  const got = digest(archive);
  if (got !== pin.sha256) {
    rm(archive);
    throw new Error(
      `The runtime download does not match the checksum in scripts/runtime.pins.json. ` +
        `Expected ${pin.sha256}, got ${got}. Nothing was unpacked.`,
    );
  }
  log(`checksum ${got.slice(0, 16)} matches the pin`);
  return archive;
}

function digest(file) {
  return createHash("sha256").update(fs.readFileSync(file)).digest("hex");
}

// python-build-standalone ships bin/python3, bin/python and four developer
// scripts as symlinks. The Tauri bundler copies resource files by content, so
// a symlink either arrives dangling or arrives as a second 18 MB copy of the
// interpreter that nobody signed. None of them are needed: farm rewrites an
// entry command beginning `python` or `python3` to the interpreter it is
// itself running under, by absolute path (Farm.start in farm/farm.py), and the
// launcher always invokes that path directly. So they go.
function dropLinks(dir) {
  let dropped = 0;
  const walk = (at) => {
    for (const entry of fs.readdirSync(at, { withFileTypes: true })) {
      const full = path.join(at, entry.name);
      if (entry.isSymbolicLink()) {
        rm(full);
        dropped += 1;
      } else if (entry.isDirectory()) {
        walk(full);
      }
    }
  };
  walk(dir);
  return dropped;
}

function interpreterIn(dir) {
  const candidates = windows
    ? ["python.exe"]
    : ["bin/python3.11", "bin/python3", "bin/python"];
  for (const candidate of candidates) {
    const full = path.join(dir, candidate);
    if (fs.existsSync(full)) return full;
  }
  throw new Error(`No interpreter in the staged tree at ${dir}.`);
}

async function main() {
  const archive = await fetchAsset();

  rm(work);
  fs.mkdirSync(work, { recursive: true });
  log(`unpack   ${path.basename(archive)}`);
  execFileSync("tar", ["-xzf", archive, "-C", work], { stdio: "inherit" });

  const tree = path.join(work, "python");
  if (!fs.existsSync(tree)) throw new Error(`The archive did not contain a python/ root.`);

  const before = measure(tree);

  let pruned = 0;
  for (const pattern of PRUNE_GLOBS) {
    for (const hit of expand(tree, pattern)) {
      rm(hit);
      pruned += 1;
    }
  }
  log(`prune    ${pruned} paths removed`);

  const python = interpreterIn(tree);
  // Normally the pinned release off PyPI. `farmFrom` in the pins file installs
  // it from a directory instead, which is how a version that has not published
  // yet gets carried. The version check below is the same either way, so a
  // source that declares something else fails here rather than in somebody's
  // hands.
  const from = pins.farmFrom || null;
  // A pip requirement, so either a git URL or a directory. A directory is for
  // working on the engine and the launcher at once; only a URL belongs on a
  // branch, because a runner has never seen anybody's worktree.
  if (from && !from.startsWith("git+") && !fs.existsSync(path.join(from, "pyproject.toml"))) {
    throw new Error(`farmFrom points at ${from}, which is neither a git URL nor a Python project.`);
  }
  log(from
    ? `farm     installing tiinyapp-farm from ${from} (expecting ${pins.farm})`
    : `farm     installing tiinyapp-farm==${pins.farm}`);
  execFileSync(
    python,
    [
      "-m", "pip", "install",
      "--quiet", "--no-cache-dir", "--no-compile", "--disable-pip-version-check",
      "--no-warn-script-location",
      ...(from ? [from] : [`tiinyapp-farm==${pins.farm}`]),
    ],
    { stdio: "inherit", env: { ...process.env, PYTHONDONTWRITEBYTECODE: "1" } },
  );

  // pip writes its own caches while it runs, so the caches go after the
  // install rather than with the rest of the pruning. Measured on a MacBook
  // Pro (Apple M5 Max) on 2026-09-14: keeping them costs 23 MB and saves
  // nothing measurable, farm --version being 0.03 s to 0.04 s either way.
  log(`cache    ${dropCaches(tree)} __pycache__ directories removed`);
  log(`links    ${dropLinks(tree)} symlinks removed`);

  if (!flag("--keep-dylib") && !windows) {
    for (const hit of expand(tree, "lib/libpython3.11.dylib")) {
      rm(hit);
      log(`dylib    removed ${path.relative(tree, hit)}, which nothing in the tree links against`);
    }
  }

  const after = measure(tree);
  // Counted before the tree is moved into place, because it is counted in it.
  const machO = windows ? 0 : machOCount(tree);

  // Everything farm.py imports from the standard library, asked for by name.
  // A prune pattern that reaches one module too far is otherwise found at the
  // worst moment, on somebody's machine, by an app that will not start.
  const NEEDED = [
    "argparse", "getpass", "hashlib", "http.client", "json", "os", "pathlib",
    "re", "shlex", "shutil", "socket", "sqlite3", "ssl", "subprocess",
    "tarfile", "tempfile", "threading", "time", "urllib.request", "zipfile",
  ];
  execFileSync(python, ["-c", `import ${NEEDED.join(", ")}`], { stdio: "inherit" });
  log(`stdlib   ${NEEDED.length} modules farm needs all import`);

  const version = execFileSync(python, ["-m", "farm.farm", "--version"], { encoding: "utf8" }).trim();
  if (version !== `farm ${pins.farm}`) {
    throw new Error(`The staged runtime answers "${version}" and the pin says farm ${pins.farm}.`);
  }

  rm(out);
  fs.mkdirSync(path.dirname(out), { recursive: true });
  fs.renameSync(tree, out);

  fs.writeFileSync(
    path.join(out, "runtime.json"),
    JSON.stringify(
      {
        target,
        python: pins.python.version,
        pythonRelease: pins.python.release,
        farm: pins.farm,
        farmFrom: pins.farmFrom || null,
        stagedAt: new Date().toISOString(),
        files: after.files,
        bytes: after.bytes,
        machO,
      },
      null,
      2,
    ) + "\n",
  );

  const mb = (n) => (n / 1024 / 1024).toFixed(1);
  log("");
  log(`staged   ${out}`);
  log(`target   ${target}`);
  log(`runtime  CPython ${pins.python.version} (${pins.python.release}), ${version}`);
  log(`size     ${mb(before.bytes)} MB in ${before.files} files became ${mb(after.bytes)} MB in ${after.files} files`);
  if (!windows) log(`mach-o   ${machO} files have to be signed before the app around them is`);
  rm(work);
}

main().catch((error) => {
  process.stderr.write(String(error.message || error) + "\n");
  process.exit(1);
});
