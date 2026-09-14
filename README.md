# Tiiny App Farm launcher

A small desktop app for macOS and Windows that installs and runs
[Tiiny App Farm](https://tiinyapp.farm) apps for somebody who will never open a
terminal. It carries its own CPython and the `tiinyapp-farm` command line tool
inside itself, so there is nothing to install first: no Python, no Node, no
package manager.

It is a face on the command line tool that already exists, not a second way of
doing things. Same install directory, same device file, same lock, same logs.
Somebody who starts with the launcher and later learns the CLI finds their apps
exactly where the CLI expects them.

The design this implements is `docs/LAUNCHER.md` in the
[tiinyapp-farm](https://github.com/Titanium-Devops/tiinyapp-farm) repository.
What was built, and what was measured, is `docs/LAUNCHER-1-REPORT.md` for the
first round and `docs/LAUNCHER-2-REPORT.md` for the models pane and per-app
needs.

## What is inside

| Piece | Where |
| --- | --- |
| The shell: window, tray, deep link, updater | `src-tauri/src/` |
| The window | `ui/` (plain files, no build step) |
| The engine: CPython and `tiinyapp-farm`, pinned | `scripts/runtime.pins.json` |
| Staging the engine into the bundle | `scripts/stage-runtime.mjs` |
| Signing the engine's Mach-O files | `scripts/sign-runtime.sh` |
| The release pipeline | `.github/workflows/` |

## Building it

```sh
npm ci
node scripts/stage-runtime.mjs          # fetch, verify, prune, install the engine
npm run tauri -- build --bundles app,dmg
```

`stage-runtime.mjs` writes `src-tauri/runtime/`, which is not committed. It
refuses a runtime whose bytes do not match the checksum in
`scripts/runtime.pins.json`, and it fails rather than staging anything if the
engine it installed does not answer with the pinned version.

On macOS the Mach-O files inside that tree have to be signed before the app
around them is, because the Tauri bundler signs frameworks and sidecars inside
out and a resource tree is neither:

```sh
scripts/sign-runtime.sh                       # ad hoc, for a local build
scripts/sign-runtime.sh "Developer ID Application: ..."
```

## Checking it against a real Tiiny

`scripts/verify-launcher.mjs` drives the built bundle's engine against a real
Tiiny in a scratch home, and prints what happened in sentences. Its own header
comment says how to call it.

Two things about it are deliberate. `--home` is required, because a gate that
runs against somebody's real `~/tiinyapps` can install, start and remove their
apps. And the device key is read by that script and written straight to the
child's standard input, never to an argument, an environment variable or a log.

**The launcher itself has no way to be handed a path to a key file.** The device
pane takes the key one way, by being pasted into a masked field, and writes it
through `farm device --key-stdin`. A path field would be a second way in, and it
would teach somebody to leave their key lying about in a file.

`FARM_LAUNCHER_HOME=/tmp/farm-gate` works on the app too, and points the
launcher's engine at a scratch home for a test run.

## Tests

```sh
cd src-tauri && cargo test && cargo clippy --all-targets -- -D warnings && cargo fmt --check
```

The JSON reader, the progress reader, the state machine, the failure classifier
and the deep link parser are plain Rust with no window behind them, so this is a
real test run rather than a compile check.
