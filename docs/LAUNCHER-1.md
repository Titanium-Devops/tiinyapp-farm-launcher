# LAUNCHER-1: the Tiiny App Farm launcher, first build

Jason, 2026-09-14 13:57: "Build it." The decision page is /Users/sem/code/tiinyapp-farm-main/docs/LAUNCHER.md
(read it first, all of it; it is the brief and this file only fixes the decisions and the deliverables).

## Decisions, fixed

1. Shape A: Tauri 2, web view UI, a bundled CPython runtime, and the farm CLI as the engine.
2. Bundle the runtime (astral-sh/python-build-standalone, CPython 3.11, install_only build, pruned as the
   page measured: drop tcl, tk, idlelib, tkinter, headers, caches). Fetch-on-first-run stays the fallback
   only if signing the bundled tree fails in phase 0.
3. The shell talks to the engine as a child process with `--json` (farm 0.1.11: list, status, install
   `--yes`, update, start, stop, check, doctor all take `--json`; device is set with
   `farm device --base <url> --key-stdin`, the key on stdin, never on a command line, never logged).
4. `farm` on PATH: a toggle in Settings, off by default. (Can be a stub in this build.)
5. Updater keypair: new, generated with `cargo tauri signer generate` into the scratchpad only, never
   committed, never printed; the public key goes in tauri.conf.json; the private key and password are
   handed to the orchestrator BY PATH for Jason to put in Bitwarden.
6. Downloads and feed: https://tiinyapp.farm/launcher/latest.json and artifacts beside it (another worker
   wires the Worker and the site; you only need the URL in the updater config).
7. Windows x64 only. 8. Product name "Tiiny App Farm", identifier `farm.tiinyapp.launcher`, deep link scheme
   `tiinyfarm` (`tiinyfarm://install/<id>`). 9. No Linux. 10. No sign-in.

## Where

Repo Titanium-Devops/tiinyapp-farm-launcher, clone at /Users/sem/orca/workspaces/tiinyapp-farm-launcher
(empty, public). Reference for Tauri 2 config, tray, updater, single instance, deep links, macOS hardened
runtime + entitlements, NSIS current-user mode, and the two signing workflows:
/Users/sem/orca/workspaces/titanium-bot-desktop (read only; copy files, do not edit that repo).
Engine source of truth: PyPI package tiinyapp-farm, pinned to 0.1.11 for this build; the build script
installs that exact version into the bundled runtime (pip download or pip install --target) and the
launcher runs it as `<bundled python> -m farm.farm ...` (console script entry is farm.farm:main). Record the
pinned version in one place and show it in the About/Settings pane.

## Deliverables, in order (phases 0 to 4 of the page)

0. A macOS app bundle containing the pruned CPython tree and the pinned farm that, from inside the bundle,
   answers `farm --version` = 0.1.11. Ad hoc signed locally; the macos.yml copy carries the extra
   `codesign` step for the Mach-O files inside the Python tree (the page measured four) BEFORE the app is
   signed, so a real run with Jason's certificate will notarise. CI on GitHub: both workflows green in
   UNSIGNED mode (no secrets on the repo yet; the workflows must skip signing cleanly when the secrets are
   absent and say so in the log, and fail loudly when they are present and wrong).
1. The engine: the shell drives list, status, install, start, stop, update, remove, check and doctor through
   the bundled CLI and renders the JSON. Download progress during install (the CLI prints progress on
   stderr; read it, or poll the app dir size, your call, say which).
2. First run: find the Tiiny (probe http://openai.api.tiiny/v1 then a manual box), paste the key once
   (stdin to `farm device`, 0600 file, never echoed), live catalog grid from https://tiinyapp.farm
   (icon, name, pitch, badge), card detail shows requires and permissions before anything downloads, Plant
   button, ready wait using the manifest health path, Open button to http://127.0.0.1:<port>.
3. The tray: menu bar (Mac) / tray (Windows) listing running apps with Open, Stop, Update, and Quit.
   Closing the window leaves apps running. Single instance. Autostart off by default.
4. Failure in words: busy port offers another port (`--port`), checksum mismatch installs nothing and says
   so, a crashed app shows the last lines of farm.log in a pane, doctor findings render as sentences with
   their fix line.

## Look

The farm's own feeling, not Titanium Bot's. Read /Users/sem/code/tiinyapp-farm-main/site/assets (CSS tokens,
the catalog cards, "Grown by", the plant marks) and docs/ART-STYLE.md, and match them. No em dashes anywhere
a person reads. Spell it Tiiny. Verify the window at 1100x720 and at the smallest size you allow; every
state (empty, first run, installing, running, failed) gets a real screenshot into docs/shots/.

## Proof

- `farm --version` from inside the bundle, the ad hoc signed app at ~/Applications/Tiiny App Farm.app
  launching from Finder, the installer sizes measured (page 0 replaces the 25 MB projection).
- A real install of story-lantern or tiiny-bench through the launcher against Jason's Tiiny: the device is
  at http://172.17.7.177/v1 over the cable; the device key is the file /Users/sem/.tiiny_1_api_key
  (chmod 600; pipe it into `farm device --key-stdin` in-process; NEVER print it, never put it in argv,
  never in a log, never in a screenshot). Use a scratch HOME so Jason's own ~/.tiinyapps is untouched.
  Do not use ports 8430, 8431, 8425, 7777, 7788, 8500.
- Rust tests for the JSON parsing and the state machine; `cargo test` and `cargo clippy` clean; CI green.
- docs/LAUNCHER-1-REPORT.md in the launcher repo: what was measured (with the machine), what was planned,
  what is left for phases 5 and 6, and the list of secrets the repo needs by NAME only (Apple, Azure,
  Tauri updater), plus the path of the generated updater private key for Jason.

## Rules

Commit on a branch, small commits, push, open a PR against main of the launcher repo; do not merge. Do not
touch /Users/sem/code/tiinyapp-farm or tiinyapp-farm-main. Never read ~/.api_keys, the keychain,
~/.claude/.credentials.json or ~/.tinyfish. Every `gh` call needs `< /dev/null`. Timeouts on everything.
Report with measured facts separated from plans.
