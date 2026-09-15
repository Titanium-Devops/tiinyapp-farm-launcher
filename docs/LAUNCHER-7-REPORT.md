# LAUNCHER-7: Linux, a changelog, and a history the site can read

Built 2026-09-15 on top of the 0.1.0 release. Version 0.1.1.

**Measured on one machine: a MacBook Pro, Apple M5 Max, macOS 26.6.2 (25G83),
arm64, plus GitHub's ubuntu-22.04 runner for anything Linux.** Section 5 is
what was not measured, and it is longer than usual on purpose.

---

## 1. Linux

A third workflow, `.github/workflows/linux.yml`, building the AppImage for
x86_64 on ubuntu-22.04 from the same pins file, the same prune list and the same
engine as the other two.

`scripts/runtime.pins.json` gained the fourth target:

| | |
| --- | --- |
| Asset | `cpython-3.11.16+20260901-x86_64-unknown-linux-gnu-install_only.tar.gz` |
| Size | 48,910,466 bytes |
| SHA-256 | `faa0758583a63f14c5eee516af82738403b59c13edda6fc0a21d953febd89eed` |

Measured here: the download, the checksum and the prune all run on this Mac and
the prune removes the same 14 paths it removes for the other targets. What
cannot run here is the step after, because staging installs `farm` into the
interpreter it just unpacked and a Linux interpreter does not execute on macOS.
That used to fail with a bare `ENOEXEC` from inside a spawn; it now says so:

> This is a darwin machine and x86_64-unknown-linux-gnu is a runtime for another
> one. Staging runs the interpreter it just unpacked, so each target is staged
> on its own platform: the workflows do that, one runner each.

**Unsigned, and that is not a gap in the workflow.** Linux has no notarisation
and nothing a stranger's machine would check a desktop signature against. What a
person can check is the SHA-256, so it is written by the runner that built the
file, verified by the publish step, and published in the release history beside
the download.

Two things the pruning does differently on Linux, both deliberate:

- **`lib/libpython3.11.so` stays.** The macOS build drops the equivalent dylib
  because nothing in the pruned tree links against it, measured. That has not
  been measured on a Linux machine, so the file stays. It costs size; an app
  that will not start costs more.
- **No Mach-O count and no signing pass.** Both are macOS ideas. The staged
  `runtime.json` records zero, which is what the macOS workflow's inside-out
  signing check compares against and what Linux has none of.

**The updater does not cover Linux, and `latest.json` says nothing about it.**
Tauri can update an AppImage, but I have no Linux machine and cannot watch an
update happen, and an updater entry that has never been exercised is worse than
an absent one: absent means "download the new one", wrong means every Linux
install stops updating and nobody finds out. The AppImage is built and published;
the feed stays macOS only until somebody with a Linux desktop can watch one
update land.

## 2. CHANGELOG.md, and a check that it is real

`CHANGELOG.md` at the root, Keep a Changelog shape, newest first, with entries
for 0.1.0 (written from what shipped last night, including its known limits) and
0.1.1.

`scripts/check-release-notes.mjs` fails if:

- `tauri.conf.json`, `package.json` and `Cargo.toml` do not all name the same
  version (all three were 0.1.0 and only one of them would have been bumped),
- that version has no heading in `CHANGELOG.md`,
- the entry has no date, or is empty, or is not the first one in the file.

Proved both ways: it passes on 0.1.1, and with the version set to a fabricated
0.9.9 it reported all three disagreements and the missing entry, then passed
again once put back.

## 3. The release history

`scripts/publish-release.mjs` now writes `launcher/releases.json`: one object per
release, newest first, each with its version, date, commit, the changelog entry
as markdown, and every file it shipped with size, SHA-256 and whether it is
signed and notarised. It is served by the route that already exists, so the farm
worker needs no change.

It is read, added to, and written back. Only one failure is allowed to be quiet,
the object not existing yet; any other failure to read stops the publish, because
carrying on would replace every older release with a history that starts today.
An entry for a version already present is replaced rather than duplicated, and
every other entry is kept.

The notes come from `CHANGELOG.md` at publish time, for older entries too, so a
release is described in one place and the download page reads it rather than
repeating it.

**0.1.0 is seeded from what is actually on the site**, not from the report's
prose: `docs/releases-seed.json` carries the three files it shipped, and each
size and SHA-256 was taken by downloading the published object and hashing it.
They match the values in `docs/LAUNCHER-RELEASE-REPORT.md` to the character:

| File | SHA-256 | Bytes |
| --- | --- | --- |
| `Tiiny-App-Farm_0.1.0_aarch64.dmg` | `b339e697…18cd1b42` | 25,288,602 |
| `Tiiny-App-Farm_0.1.0_x64.dmg` | `62301dbb…0d776c97` | 25,964,260 |
| `Tiiny-App-Farm_0.1.0_x64-setup.exe` | `d8a02af9…bfa7375f` | 27,720,608 |

Ordering, extraction and the seed were exercised offline: the 0.1.1 notes do not
run into the 0.1.0 entry, a version with no entry returns nothing rather than the
wrong text, and `0.1.10` sorts above `0.1.1` rather than below it.

## 4. The Windows gate needs all three Azure secrets

The gate asked one question, whether `AZURE_CLIENT_ID` was set. Setting one of
the three flipped it to signing, and `azure/login` then failed with a message
about none of them, after the whole build, so the installer was never uploaded.
That cost a run last night.

It now requires `AZURE_CLIENT_ID`, `AZURE_TENANT_ID` and
`AZURE_SUBSCRIPTION_ID` together, and when it builds unsigned it names which are
missing rather than leaving somebody to guess.

## 5. What was not measured, which on this one matters

- **The AppImage has never been run.** Not by me, not by anybody on this team.
  There is no Linux machine here. The runner checks that the file exists, is an
  executable ELF, and is large enough to contain the bundled Python; that is the
  whole of what a build runner can say about a desktop app. **The window, the
  tray, the deep link, the per-app windows and whether the bundled interpreter
  reaches a Tiiny over the local network are all untested on Linux.**
- The Linux runtime has not been staged end to end anywhere yet, because the
  install step needs a Linux host. The first run of the new workflow is the
  first time that happens.
- No Linux update has been attempted, which is why the feed leaves Linux out.
- `releases.json` has never been written to the bucket. The read, the merge and
  the ordering are exercised offline; the first real read-modify-write is the
  first publish after this merges.
- Windows and an Intel Mac are unchanged and still only built by CI.

## 6. Checks

| Check | Result |
| --- | --- |
| `cargo test` | 84 passed, 0 failed |
| `cargo clippy --all-targets -- -D warnings` | clean |
| `cargo fmt --check` | clean |
| `node scripts/check-workflows.mjs` | 18 shell steps, 0 that bash refused |
| `node scripts/check-release-notes.mjs` | 0.1.1 agrees across all three, and has an entry |
| `npm run tauri build` | builds `Tiiny App Farm_0.1.1_aarch64.dmg` |

The workflow check now covers three files rather than two, which is how the new
one was written without repeating either of last night's two mistakes.
