#!/usr/bin/env node
// Read every `run:` script out of the workflows and ask bash whether it parses.
//
//   node scripts/check-workflows.mjs
//
// Two bugs reached main in one night that this would have caught before either
// cost a release.
//
// The first was an indentation slip: a comment written at six spaces beside
// lines at ten dedents out of the block scalar it belongs to and ends the YAML
// mapping early. GitHub then cannot read the file at all, and the only sign is
// a run that fails with no jobs and no logs, listed under the file path instead
// of the workflow's name.
//
// The second was an apostrophe. A shell comment reading "the farm's worker",
// inside a script handed to `node -e '...'`, closes the single quoted string on
// that apostrophe. The shell re-parses the rest, the argument list arrives
// mangled, and the step dies somewhere that looks nothing like the cause.
//
// A YAML parser catches the first and not the second, because the file is valid
// YAML either way. Only a shell can say whether a shell script parses, so that
// is what this asks. No dependency: the blocks are found by indentation, which
// is all a workflow's run steps are.
//
// Expressions become a placeholder first, because ${{ matrix.target }} is not
// shell and bash has no opinion about it.

import { execFileSync } from "node:child_process";
import fs from "node:fs";
import os from "node:os";
import path from "node:path";
import { fileURLToPath } from "node:url";

const here = path.dirname(fileURLToPath(import.meta.url));
const workflows = path.join(here, "..", ".github", "workflows");

const indentOf = (line) => line.length - line.trimStart().length;

// Every `run: |` block in one file, with the step name that owns it.
function runBlocks(text) {
  const lines = text.split("\n");
  const found = [];
  let name = "an unnamed step";
  for (let i = 0; i < lines.length; i += 1) {
    const line = lines[i];
    const named = line.match(/^\s*-?\s*name:\s*(.+?)\s*$/);
    if (named) name = named[1].replace(/^["']|["']$/g, "");
    if (!/^\s*-?\s*run:\s*[|>][-+]?\s*$/.test(line)) continue;
    const outer = indentOf(line);
    const body = [];
    let j = i + 1;
    // Blank lines belong to the block; the first non-blank line at or left of
    // the `run:` key ends it.
    for (; j < lines.length; j += 1) {
      if (lines[j].trim() === "") { body.push(""); continue; }
      if (indentOf(lines[j]) <= outer) break;
      body.push(lines[j]);
    }
    const strip = Math.min(...body.filter((l) => l.trim() !== "").map(indentOf));
    // What ends the block should be YAML: a key, or the next list item. A line
    // of shell out here means the block ended early, which is what an
    // under-indented line inside it does, and GitHub will not read the file at
    // all. bash cannot see that, because what is left still parses.
    const after = lines[j] ?? "";
    // A comment at step level ends a block perfectly well, and these files are
    // full of them.
    const ended = after.trim() === ""
      || after.trim().startsWith("#")
      || /^\s*(-\s|[A-Za-z_][\w.-]*\s*:)/.test(after);
    found.push({ name, script: body.map((l) => l.slice(strip)).join("\n"), ended, after: after.trim() });
    i = j - 1;
  }
  return found;
}

let checked = 0;
let bad = 0;

// The YAML half needs a parser. python3 with PyYAML is on this Mac and on every
// GitHub runner; without it the shell half still runs and says so, because a
// check that quietly does less than it claims is worse than one that is absent.
let yamlCheck = true;
try {
  execFileSync("python3", ["-c", "import yaml"], { stdio: "pipe" });
} catch {
  yamlCheck = false;
  process.stdout.write("python3 with PyYAML is not here, so only the shell half runs\n");
}

for (const file of fs.readdirSync(workflows).filter((n) => /\.ya?ml$/.test(n))) {
  const text = fs.readFileSync(path.join(workflows, file), "utf8");

  // Does it parse as YAML at all? This is the half that catches the
  // indentation slip, where a line written too far left ends the block scalar
  // early and the shell below it becomes nonsense YAML. bash cannot see that,
  // because what is left of the script still parses perfectly well.
  if (yamlCheck) {
    try {
      execFileSync("python3", ["-c", "import sys,yaml; yaml.safe_load(open(sys.argv[1]))",
        path.join(workflows, file)], { stdio: "pipe" });
    } catch (error) {
      bad += 1;
      const said = (error.stderr || "").toString().trim().split("\n").slice(-4).join("\n  ");
      process.stderr.write(`\n${file}: GitHub will not be able to read this.\n  ${said}\n`);
    }
  }

  // A workflow with no name is usually one GitHub could not read either, and it
  // shows up in the run list as the file path rather than the name.
  if (!/^name:\s*\S/m.test(text)) {
    process.stderr.write(`${file}: has no name at the top level, which GitHub shows as the path\n`);
    bad += 1;
  }

  for (const { name, script, ended, after } of runBlocks(text)) {
    if (!ended) {
      bad += 1;
      process.stderr.write(
        `\n${file} :: ${name}\n  the run block ends at a line that is not YAML, so something in it is ` +
        `under-indented and GitHub will not read this file:\n  ${after}\n`,
      );
    }
    // pwsh and cmd steps are somebody else's grammar.
    if (/^\s*\$|Get-ChildItem|Set-Content/m.test(script) && !/^\s*set -/m.test(script)) continue;
    checked += 1;
    const scratch = path.join(os.tmpdir(), `wf-${process.pid}-${checked}.sh`);
    fs.writeFileSync(scratch, script.replace(/\$\{\{[^}]*\}\}/g, "EXPRESSION"));
    try {
      execFileSync("bash", ["-n", scratch], { stdio: "pipe" });
    } catch (error) {
      bad += 1;
      const said = (error.stderr || "").toString().trim().split("\n")
        .map((l) => l.replace(scratch, "the script")).join("\n  ");
      process.stderr.write(`\n${file} :: ${name}\n  ${said}\n`);
    } finally {
      fs.rmSync(scratch, { force: true });
    }
  }
}

process.stdout.write(`${checked} shell steps checked, ${bad} that bash refused\n`);
process.exit(bad ? 1 : 0);
