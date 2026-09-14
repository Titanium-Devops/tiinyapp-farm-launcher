# LAUNCHER-1: what was built, and what was measured

Built 2026-09-14 against `docs/LAUNCHER.md` in the `tiinyapp-farm` repository and
the brief in `docs/LAUNCHER-1.md`. Phases 0 to 4.

**Every number below was measured on one machine: a MacBook Pro, Apple M5 Max,
macOS 26.6.2 (25G83), arm64.** Nothing here was measured on Windows, on an Intel
Mac, or on any machine but that one. Anything not measured is in the section
called "Planned, not measured" and is labelled as such there.

---

## 1. What it is

A Tauri 2 app carrying CPython 3.11.16 and `tiinyapp-farm` 0.1.11 inside itself.
Every action the window takes is the CLI doing the work, run as a child process
with `--json`. One install directory, one device file, one lock, one set of
failure messages, whichever face somebody came in through.

| Piece | Where |
| --- | --- |
| Window, tray, deep link, updater, engine | `src-tauri/src/` |
| The page | `ui/` (plain files, no build step) |
| The pinned engine | `scripts/runtime.pins.json` |
| Staging it into the bundle | `scripts/stage-runtime.mjs` |
| Signing its Mach-O files | `scripts/sign-runtime.sh` |
| The real-Tiiny gate | `scripts/verify-launcher.mjs` |
| The two release workflows | `.github/workflows/` |

Every command the brief lists is reachable from the window:

| Command | Where it is in the window |
| --- | --- |
| `list` | The farm grid and the Running pane, on every refresh |
| `status` | The Running pane, the tray, and a poll every eight seconds |
| `install` | Plant it, on a card |
| `start`, `stop` | The Running pane, the card, and the tray |
| `update` | The Update button, and the tray's Update entry |
| `remove` | Remove, on a row and on a card |
| `check` | Look for app updates, on the Settings pane |
| `doctor` | Check everything, on the Settings pane |
| `device` | The first run pane, and Change it on Settings |

The pinned versions live in exactly one file, `scripts/runtime.pins.json`, and
the Settings pane reads them back out of the staged tree rather than repeating
them.

---

## 2. Measured

### 2.1 Phase 0: the riskiest thing

The question phase 0 exists to answer is whether a CPython tree can live inside a
signed macOS app. The sharp edge the design page found is real: the Tauri bundler
signs frameworks and sidecar binaries inside out, per Apple's rule, and only
those. A Python tree copied in as a resource is neither, so it would reach the
notary unsigned.

The page counted four Mach-O files in a pruned CPython 3.11.13 tree. That number
held for 3.11.16, but only after pruning. Measured here:

| | |
| --- | --- |
| Mach-O files in the tree as published | 11 |
| Mach-O files after the page's prune list | 4 |

The seven that go are the tcl and tk libraries, the itcl and thread extensions,
and `_tkinter`. The four that stay are `bin/python3.11`,
`lib/libpython3.11.dylib`, `lib-dynload/_crypt` and `lib-dynload/_dbm`.
`scripts/sign-runtime.sh` signs those four, in the staging directory, before
`tauri build` copies them. A Mach-O signature lives inside the file, so signing
the staging copy is signing what ships.

Verified on the built bundle: `codesign --verify --deep --strict` accepts the app,
and each of the four files inside `Contents/Resources/runtime` verifies on its
own. The macOS workflow runs the same two checks before it will upload anything.

### 2.2 The sizes, which replace the page's projection

| What | Measured | The page's figure |
| --- | --- | --- |
| Disk image, arm64 | 32,464,071 bytes | projected about 25 MB |
| App bundle on disk | 73.5 MB | 7.8 MB plus a runtime |
| Runtime as published | 27,087,450 bytes (66.1 MB unpacked, 2,035 files) | 27.1 MB |
| Runtime staged, pruned, with farm in it | 56.2 MB in 1,518 files | 44 MB, 1,650 files for 3.11.13 |
| `farm --version` from inside the bundle | `farm 0.1.11` | the phase 0 question |

The disk image figure moves by a few hundred bytes between builds of the same
source, because it is compressed. Four builds here came in between 32,459,073
and 32,464,071 bytes.

Two reasons the staged tree is bigger than the page's 3.11.13 figure. 3.11.16
ships tcl9 and tk9 rather than tcl8, and both `bin/python3.11` and
`lib/libpython3.11.dylib` are 18 MB each in this build.

**18 MB of the 56 is a library nothing links against.** `otool -L` on the
interpreter and on both remaining extension modules shows none of them reference
`libpython3.11.dylib`: python-build-standalone statically links the interpreter.
`node scripts/stage-runtime.mjs --drop-unused-dylib` removes it, which would take
the disk image to somewhere near the page's projection. It is **off by default**,
because an app that embeds Python rather than running `python3` would want that
library, and the decision to drop it is Jason's rather than mine. No app in the
catalog embeds Python today.

Bytecode caches were measured rather than guessed at. Keeping them costs 23 MB
and saves nothing: `farm --version` is 0.03 s to 0.04 s with or without them,
three runs each. They are dropped.

The staging script also removes the six symlinks
python-build-standalone ships (`bin/python`, `bin/python3`, `2to3`, `idle3`,
`pydoc3`, `python3-config`). The Tauri bundler copies resource files by content,
so materialising them added a second and a third 18 MB copy of the interpreter
that nothing had signed. Nothing needs them: `Farm.start` rewrites an entry
command beginning `python` or `python3` to the absolute path of the interpreter
it is running under.

### 2.3 A real install against a real Tiiny

`node scripts/verify-launcher.mjs` drives the built bundle's engine in a scratch
home. All ten checks passed, twice:

```
yes  the engine answers from inside the bundle: farm 0.1.11
yes  the Tiiny's key is saved from standard input: Device settings saved.
yes  the device file is private: mode 600
yes  the address on file is the one asked for: http://172.17.7.177/v1
yes  the key is on file: 36 characters, not shown
yes  the catalog answers: 7 apps
yes  story-lantern installs: version 0.1.2
yes  story-lantern starts: port 8421, http://localhost:8421
yes  story-lantern is in the status list: pid 29728, up 0s, health no health path
yes  story-lantern stops
```

Port 8421 rather than 8420 because something else on this Mac is listening on
8420, and the engine moved a movable app off it by itself. That is the behaviour
Jason asked the CLI for on 2026-09-14, working through the launcher without the
launcher knowing anything about it.

The key was read by the gate process and written to the child's standard input.
It is not in any argument, any environment variable, any log, or any screenshot.
The one place its existence is recorded is the line above, which says how many
characters it has and nothing else.

The window did the same install afterwards, driven through the
`tiinyfarm://install/story-lantern` deep link, and `docs/shots/04-installing.png`
is that install in progress.

### 2.4 Progress, and where it comes from

`farm install --json` puts one JSON object on stdout and everything a person
would read on stderr. Those lines are the only honest progress there is, because
they come from the code doing the work. The launcher reads them and maps five of
them to a phase; everything else it leaves alone. The exact sentences farm 0.1.11
printed installing Story Lantern here are the test fixture in
`src-tauri/src/progress.rs`, copied out of the run rather than written from
memory, so a change to the engine's words turns the tests red.

There is no byte-level percentage, because the engine does not print one. The bar
moves in five steps, and the words beside it are the engine's own.

One detail about the Open button. The design asks for
`http://127.0.0.1:<port>`; what the launcher opens is the link the engine gives
it, which is `http://localhost:<port>` plus the app's own landing path when its
manifest names one. They are the same address, and taking the engine's link
means an app that says it opens at `/show` opens at `/show`. The launcher will
hand the system opener nothing else: only a loopback address or the farm's own
site, checked before it goes.

### 2.5 macOS Local Network privacy, and the launcher

This is the one finding worth reading twice.

The bundled interpreter is a binary macOS has never seen. Measured here, in this
order:

| When | What happened |
| --- | --- |
| Bundled interpreter, run as a child of a terminal | `[Errno 65] No route to host` reaching 172.17.7.177 |
| `farm doctor` inside the launcher, first attempts | "This Python cannot reach your Tiiny at 172.17.7.177, because macOS is blocking it from your local network." Every Python on the machine was blocked, including Apple's. |
| `farm doctor` inside the launcher, later the same session | "Your Tiiny at 172.17.7.177 answered this Python in 4 ms." Every Python on the machine reached it. |
| Bundled interpreter, run as a child of a terminal, after all that | still `[Errno 65]` |

Local network access on macOS is granted to the responsible application, not to
the binary. Run from a terminal, the bundled interpreter inherits the terminal's
permission, which it does not have. Run as a child of the launcher, it inherits
the launcher's, which it got. The launcher reaches the Tiiny; a terminal poking
at the bundled interpreter proves nothing about that.

Two things follow. First, the first-run experience includes a system permission
prompt nobody has designed for yet, and the window will look broken for as long
as the person has not answered it. The farm's own `doctor` already says what to
do in plain words, and it is on the Settings pane, so the material is there. What
is missing is showing it at the right moment rather than waiting for somebody to
press Check everything.

Second, while the permission is refused, the farm's fallback saves a **different
interpreter** into `~/.tiinyapps/settings.json` and runs apps under it. On the
first gate run here it saved `/opt/homebrew/bin/python3`. That is the CLI being
helpful, and it quietly undoes the launcher's whole premise: the apps were not
running on the Python the launcher carries. Worth a decision before the beta.

None of this was tested on a notarised, Developer ID signed build, because I have
no certificate. Gatekeeper and the privacy database treat an ad hoc signed app
differently, and this is the first thing to check on Jason's own certificate.

### 2.6 Failure, in words

Which failure a message is, and what to offer next, is decided in
`src-tauri/src/trouble.rs` and tested there, rather than by the page reading
words out of an error string. Each of the four cases the design calls for has a
test against the sentence farm 0.1.11 actually raises.

| The engine says | The window says, and offers |
| --- | --- |
| `Checksum mismatch; archive was not unpacked or run.` | "The download did not match the catalog", plus that nothing on the computer changed. No install. |
| `Port 8420 is already in use; use farm start x --port N.` | "That port is taken", plus a field prefilled with 8421 and a Try that port button. |
| `Port 8500 is already in use, and tiiny-brain cannot be moved off it.` | "That port is taken", and no port offered, because the app declares one port and reads no setting for another. |
| `... timed out waiting for readiness after 10 s.` | "It started and then stopped", plus the last lines of that app's `farm.log` in a pane. |

The checksum case was produced rather than imagined: the engine was pointed at a
local copy of the catalog with one byte of Story Lantern's SHA-256 changed, the
archive on GitHub untouched. `docs/shots/08-failure-in-words.png` is the result,
and the scratch home afterwards contained one empty `.locks` directory and no app.

The doctor's findings render as sentences with their fix line underneath, which
is `docs/shots/07-settings-and-doctor.png`.

### 2.7 The window

Eleven captures in `docs/shots/`, each of the launcher's own window taken by
window id, so nothing else that was on the screen is in any of them. The window
frame measures 1100 by 720 points, and 720 by 560 at the smallest size it allows,
where the catalog grid drops to two columns and nothing scrolls sideways.

One thing the builder alone did not do: `inner_size(1100, 720)` produced a window
of 1197 by 881 points. Saying it again with `set_size` after the window exists is
taken. The constants are `WINDOW` and `SMALLEST` in `src-tauri/src/lib.rs`, and
they are what the screenshots were measured at.

### 2.8 Tests

42 Rust tests, `cargo clippy --all-targets -- -D warnings` clean, `cargo fmt
--check` clean. The JSON reader, the progress reader, the state machine, the
failure classifier, the settings file and the deep link parser have no window
behind them, so this is a real test run rather than a compile check.

Three defects were found by something other than me reading the code.

The Windows leg of CI found one this Mac never could. The prune patterns were
resolved with `fs.existsSync` for any segment without a wildcard in it, and
Windows has a case insensitive filesystem: `lib` found `Lib`, and `thread*`
inside it matched `Lib/threading.py` and deleted the standard library's
threading module. pip failed on the next line with `No module named
'threading'`, which is a long way from saying a prune pattern was too greedy.
Every segment is now matched against the names a directory actually has, case
sensitively, on both platforms, and the staged tree is asked to import the
twenty standard library modules `farm.py` uses before the staging is accepted.
Staging on macOS is byte for byte what it was.

Two the tests found, both fixed in the code rather than in the test:

- A start that failed left the app in `Starting`, a state nothing else would ever
  move it out of, so the window would have shown a spinner forever.
- The doctor's fix line rendered twice, because a finding with nothing of its own
  to say carries its fix line as its message and the page drew both.

---

## 3. Deliberate departures from the design page

**macOS is built per architecture, not universal.** The page assumed the desktop
app's universal build could be copied. It cannot: python-build-standalone
publishes an arm64 tree and an x86_64 tree and there is no fat one, so a
universal app would carry 56 MB of the wrong machine in every download. The
workflow builds twice and the updater feed names `darwin-aarch64` and
`darwin-x86_64` separately, which is what those keys are for. A third job merges
the two fragments and **fails if either is missing**, because a feed naming one
architecture is an update that silently never arrives on half the installed base.
That is the same class of failure as the key mismatch the desktop workflow
already guards against.

**`farm remove` has no `--json` mode in 0.1.11.** `JSON_COMMANDS` is list,
status, check, doctor, install, update, start and stop. The brief's phase 1 asks
for remove through the bundled CLI, so the launcher runs it as prose and shows
the sentence it prints. This works and is honest, but it is the one command whose
result the launcher reads as English rather than as a structure. Adding `--json`
to remove in a later farm release would close it.

**The device pane can read the key from a file.** The design has one way in,
pasting it. There are now two: pasting, and naming a file the launcher opens
itself. The second exists because the key has to reach the engine without ever
being on a clipboard or in a screenshot, and it is what makes the gate honest.
It is one extra button and it declares exactly what it does.

**`farm` on the PATH is a stub.** The switch is in Settings, remembers its
answer, and says in the window that the doing of it lands later. The brief allows
a stub in this build.

---

## 4. Planned, not measured

Nothing in this section has been run.

- **Windows.** The x86_64 runtime is pinned and checksummed and
  `stage-runtime.mjs` knows the Windows layout, and `windows.yml` stages it and
  asks the staged engine for its version before it bundles anything. No Windows
  machine has run any of it. The installer size is unmeasured, and the design
  page's 60 MB projection stands until a run replaces it.
- **Signing and notarising.** The macOS workflow signs the four Mach-O files, then
  the app, then notarises and staples the disk image, then refuses to upload
  unless `spctl` and `stapler validate` accept both. None of that has run, because
  the repository has no secrets. The workflows build unsigned and say so in the
  log, which is what a run on this branch does.
- **The updater.** A keypair was generated and the public half is in
  `tauri.conf.json`. No update has been signed, served or applied.
- **Autostart.** The switch calls the plugin. Nobody has logged out and back in.
- **An Intel Mac.** The x86_64 leg builds in CI on an arm64 runner, which cannot
  run the interpreter it stages, so that job checks the file is there and says in
  the log that it could not do more.

---

## 5. What phases 5 and 6 need

- **The site and the feed.** Download buttons on `/install/` with the platform
  detected, `tiinyfarm://install/<id>` beside the copyable command on every app
  page, and `https://tiinyapp.farm/launcher/latest.json` plus the artifacts served
  from the farm's Worker. The launcher already speaks the deep link and already
  points the updater at that URL.
- **The first-run permission moment.** Section 2.5. The window should say what
  macOS is about to ask and what to answer, before it asks.
- **The interpreter fallback.** Decide whether an app that cannot reach the Tiiny
  under the bundled Python should quietly move to another one on the machine, or
  should say so. Today it moves, and the person is not told.
- **The beta.** Five people who are not developers, on both platforms. The number
  that matters is minutes from download to a running app, and how many of them
  needed a human.

---

## 6. Secrets the repository needs, by name

None of these are set. Both workflows check for them and build unsigned, with a
notice in the log, when they are absent. They fail loudly when a secret is present
and wrong, which is the case worth having.

**macOS, in `.github/workflows/macos.yml`:**

- `APPLE_CERTIFICATE_P12_BASE64`
- `APPLE_CERTIFICATE_PASSWORD`
- `APPLE_SIGNING_IDENTITY`
- `APPLE_TEAM_ID`
- Either `APP_STORE_CONNECT_KEY_ID`, `APP_STORE_CONNECT_ISSUER_ID` and
  `APP_STORE_CONNECT_KEY_P8_BASE64`, or `APPLE_ID` and
  `APPLE_APP_SPECIFIC_PASSWORD`. The App Store Connect trio is the steadier door:
  it does not expire when somebody changes an Apple ID password.
- `TAURI_SIGNING_PRIVATE_KEY`
- `TAURI_SIGNING_PRIVATE_KEY_PASSWORD`

**Windows, in `.github/workflows/windows.yml`:**

- `AZURE_CLIENT_ID`
- `AZURE_TENANT_ID`
- `AZURE_SUBSCRIPTION_ID`

The Windows job signs through a GitHub OIDC federated credential scoped to
`refs/heads/main`. There is no client secret and there should not be one. Do not
widen that credential to cover branches: sign only on main is the boundary, not a
workaround.

### The updater keypair

Generated for this app with `tauri signer generate`. It is new, it is not the
desktop app's, and it is not in the repository.

- **Private key:** `<scratchpad>/launcher/updater-key/tiinyapp-farm-launcher.key`
- **Its password:** `<scratchpad>/launcher/updater-key/password.txt`
- **Public half:** already in `src-tauri/tauri.conf.json` under
  `plugins.updater.pubkey`, where it is not a secret.

Both files are mode 0600 and neither has been printed, logged or committed. The
orchestrator has the absolute paths. They belong in Bitwarden and then in the two
GitHub secrets above, and the scratchpad copies should go once they are there.

**Losing the private key strands every installed copy of the launcher**, because
an update signed with anything else is refused by the updater without a word. The
macOS workflow fails the build if the key does not match the public key in the
config, because the CLI only warns about that and a green build whose every update
is silently refused is the worst shape this failure can take.
