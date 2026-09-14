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
| Mach-O files after `libpython3.11.dylib` is dropped too | 3 |

The seven that go with the page's list are the tcl and tk libraries, the itcl
and thread extensions, and `_tkinter`. The eighth is `libpython3.11.dylib`,
which nothing links against. The three that stay are `bin/python3.11`,
`lib-dynload/_crypt` and `lib-dynload/_dbm`.
`scripts/sign-runtime.sh` signs those four, in the staging directory, before
`tauri build` copies them. A Mach-O signature lives inside the file, so signing
the staging copy is signing what ships.

Verified on the built bundle: `codesign --verify --deep --strict` accepts the app,
and each of the four files inside `Contents/Resources/runtime` verifies on its
own. The macOS workflow runs the same two checks before it will upload anything.

### 2.2 The sizes, which replace the page's projection

| What | Measured | The page's figure |
| --- | --- | --- |
| Disk image, arm64 | 25,247,412 bytes | projected about 25 MB |
| App bundle on disk | 56.8 MB | 7.8 MB plus a runtime |
| Runtime as published | 27,087,450 bytes (66.1 MB unpacked, 2,035 files) | 27.1 MB |
| Runtime staged, pruned, with farm in it | 39.3 MB in 1,517 files | 44 MB, 1,650 files for 3.11.13 |
| `farm --version` from inside the bundle | `farm 0.1.12` | the phase 0 question |

The disk image figure moves by a few hundred bytes between builds of the same
source, because it is compressed. The builds here came in between 32,459,073
and 32,764,713 bytes, the last of which carries the finder.

Two reasons the staged tree is bigger than the page's 3.11.13 figure. 3.11.16
ships tcl9 and tk9 rather than tcl8, and both `bin/python3.11` and
`lib/libpython3.11.dylib` are 18 MB each in this build.

**The 18 MB `libpython3.11.dylib` is dropped, and that is the difference between
32.8 MB and 25.2 MB.** `otool -L` on the interpreter and on both remaining
extension modules shows none of them reference it: python-build-standalone links
the interpreter statically. Dropping it is now the default and
`--keep-dylib` brings it back, for the day an app ships a compiled extension
that wants to embed Python. No app in the catalog does today.

The bundle without it still works, which is the point of dropping it rather than
arguing about it: `farm --version` answers `farm 0.1.12` from inside the app, and
the ten check gate ran again on that bundle and installed, started, listed and
stopped a real app against the real Tiiny.

Dropping it also changes how many Mach-O files have to be signed, from four to
three, so that number is no longer written down anywhere. The staging script
counts them by reading the first four bytes of every file and records the count
in `runtime.json`; `sign-runtime.sh` refuses unless it signed exactly that many,
and the workflow refuses unless it verified exactly that many inside the built
bundle. A number typed into a workflow goes stale the first time a runtime
release changes what it ships, and the failure that causes is an unsigned file
inside a signed bundle.

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

### 2.5 Finding the Tiiny

Finding the Tiiny is the engine's job. `farm device --find --json` looks on
every USB cable in the machine, on this network, and at the TiinyOS client, and
answers with one object:

```
{"command":"device","ok":true,"blocked":false,
 "python":"<the interpreter that did the looking>","moved":null,
 "found":[{"serial","name","address","via","base","interfaces"}]}
```

`via` is `cable`, `network` or `TiinyOS client`. The launcher draws exactly that
and has no second idea of where a Tiiny might be. The first run pane starts
looking on its own, shows each box by name, address, how it was reached and its
serial, with one Use button each, offers Look again and a manual address
underneath, and says in one sentence when nothing answered.

Measured on this Mac, from inside the built app: Jason's Tiiny found over the
USB cable at 172.17.7.177 in the first look, and the key saved to it from a
file, mode 0600, without the key ever being displayed.

Two of the four states that pane can be in were not seen, because the Tiiny was
plugged in the whole time and the app already held its local network grant: the
sentence when nothing answers, and the panel when macOS has refused the app.
Both are written and both are one branch away from the state that was seen, but
neither has been on screen, and a picture of one would have to be staged rather
than met.

That command is in farm 0.1.12, which published while this was being built. The
engine is pinned to `0.1.12` from PyPI and `farmFrom` is gone; everything above
was measured against that pin. While it was unpublished the pin was the public
commit the finder was written on, as a git URL rather than a path so that a
runner that has never seen this Mac built the same thing, and CI proved that
worked before the repin. **farm 0.1.13 published later the same hour**, which the
launcher's own doctor pointed out on screen; nothing here has run against it, and
moving to it is one line in `scripts/runtime.pins.json`. Deleting `farmFrom` and setting
`farm` to `0.1.12` is the whole of the repin, and the Settings pane says
`built from <path>` for as long as it is a worktree, so nothing can quietly
claim a published version it is not.

### 2.6 macOS Local Network privacy, and why the Info.plist key is the whole story

macOS grants local network access **per application**, and every Python the
launcher starts is a child of the launcher, so all of it is attributed to
"Tiiny App Farm". Without `NSLocalNetworkUsageDescription` in `Info.plist`,
macOS shows no prompt at all: it answers "No route to host" to every probe, for
ever, which reads exactly like a Tiiny that is switched off. The key is
declared, and the sentence in it is what the person is shown.

`NSBonjourServices` is deliberately absent. It is only needed to browse or
advertise an mDNS service and the finder does neither: it reads its own side of
each USB cable and works out the box's address by arithmetic, sends one UDP
discovery packet, and asks the TiinyOS client at a fixed host name.

Measured here, all with the same bundled interpreter and the same scratch home,
and the difference between the rows is only which process was its parent:

| Parent | `farm device --find` said |
| --- | --- |
| A terminal | `blocked: true`, and the farm moved to `/opt/homebrew/bin/python3` and saved it |
| The launcher | `blocked: false`, found Jason's Tiiny over the cable, moved nothing |

And the doctor, run from the Settings pane, so as a child of the launcher:

```
farm 0.1.11, running apps with
  /Users/sem/Applications/Tiiny App Farm.app/Contents/Resources/runtime/bin/python3.11 (Python 3.11.16).
The Tiiny on file is at http://172.17.7.177/v1.
Your Tiiny at 172.17.7.177 answered this Python in 9 ms.
The key on file is accepted by your Tiiny.
```

**What was not observed: the prompt itself.** This Mac had already allowed the
app earlier in the same session, under the same bundle identifier, so macOS did
not ask again, and nothing here reset that grant: the privacy database is not
something a build should be rewriting on somebody's machine. What the two rows
above show is the grant working and its absence failing, which is the behaviour
either side of the prompt. Seeing the prompt itself needs a machine that has
never run this bundle identifier, and it belongs in the beta.

### 2.7 One interpreter, and only one. Fixed, and proved.

A single local network grant to "Tiiny App Farm" has to cover finding the
device, the doctor, and every app the launcher starts. That only holds if there
is exactly one interpreter, and the CLI's own fallback is what breaks it: when
macOS refuses a Python the local network, the farm walks the other Pythons on
the machine, keeps the first that gets through, and **saves it** in
`~/.tiinyapps/settings.json`, where every later command reads it. A launcher
that inherited that would run apps under a Homebrew Python nobody has granted
anything.

Two things stop it, and both are in.

**`farm start` is always given `--python <the bundled interpreter>`.** That wins
over the saved setting and, more to the point, skips the walk entirely:
`Farm.start` only calls `choose_python` when no `--python` was passed
(`if interpreter and not python:` in `farm/farm.py`).

**The saved setting is put back before and after every engine call.** `device`
and `doctor` have no `--python` flag, so the setting is the only lever there.
`Engine::pin_interpreter` rewrites it, keeping every other key, and
`Engine::run` calls it on both sides of the child.

The exact run asked for, on this Mac, in a scratch home:

| | |
| --- | --- |
| Put on file by hand, which is what a refused probe leaves | `"python": "/opt/homebrew/bin/python3"` |
| Story Lantern then started from the launcher's own Start button | |
| `ps` | `~/Applications/Tiiny App Farm.app/Contents/Resources/runtime/bin/python3.11 lantern.py` |
| `settings.json` afterwards | `.../Tiiny App Farm.app/Contents/Resources/runtime/bin/python3.11`, and no Homebrew path |
| Every other key in that file | untouched |

The same holds for a command with no flag: a foreign interpreter written in by
hand is replaced on the launcher's next engine call, within the eight second
status poll, without anybody pressing anything.

For contrast, Jason's own Story Lantern ran beside all of this the whole time
under Xcode's Python 3.9, which is why port 8420 was taken and the engine moved
the launcher's copy to 8421. The launcher never touched it.

Pinning a shared setting is a real consequence and it is worth saying plainly:
the launcher writes into the same `~/.tiinyapps` the CLI reads, so somebody who
uses both will find the interpreter pointing inside the app bundle. That is
stable while the app is installed and harmless once it is not, because
`Farm.app_python` only takes a saved interpreter that is still runnable and
falls back to its own otherwise. If the engine ever grows a way to forbid the
walk from outside, a `FARM_PYTHON_PINNED` or the like, the launcher should use
it and this reconciliation should go.

### 2.7b The microphone, the camera, and what was not seen

`NSMicrophoneUsageDescription` and `NSCameraUsageDescription` are in the built
`Info.plist`, and `com.apple.security.device.audio-input` and
`com.apple.security.device.camera` are in the entitlements the app is signed
with. At the web layer wry grants capture permission itself
(`WKPermissionDecision::Grant` in `wry_web_view_ui_delegate.rs`), so no
page-level prompt stands in the way; what remains is the macOS prompt the first
time something opens the device.

**That prompt was not seen, and the reason is worth reading.** Titanium Tiiny
Bot was installed, started on port 7790 and opened in its own window; its
console loaded and Titan answered in it. Holding its Talk button for two and a
half seconds produced this, in the app's own words:

```
Cannot reach the device. Check its address and that its model is running.
```

Its voice path refuses before it reaches `getUserMedia`, because the voice model
is not running on this Tiiny. The privacy database has no entry of any kind for
`farm.tiinyapp.launcher`, which says the same thing from the other side: nothing
asked for the microphone, so nothing was prompted. The plumbing is in and
unproven, and it needs a Tiiny with a voice model on it.

### 2.7c The key: one masked field, and no second way in

The device pane takes the key one way. The person copies it from TiinyOS, pastes
it into a masked field, and the launcher writes it by piping it to
`farm device --base <url> --key-stdin`. It is never an argument, never an
environment variable, never in a log, and it is never shown again. The caption
under the field says so in plain words: "Copy it from TiinyOS, Settings, API
Key. Saved on this computer, only you can read it, and it is never shown again."

**There is no field that asks for a path to a key file, anywhere.** There was
one, and it is gone: the input, the dialog, the Rust command behind it and the
example in the README. A path field is a second way in and it teaches somebody
to leave their key lying about in a file. `scripts/verify-launcher.mjs` still
reads a key file, because a gate has to come from somewhere, and it pipes it
into the engine itself on the command line of the test, outside the app.

Measured, through the window, on a fresh scratch home: the masked field took a
stand-in string and the launcher wrote `device.json` with the right base, a key
of the right length and mode 0600. The real key was then put on file by the gate
script for the rest of the run, so it never went near a clipboard or a
keystroke. `docs/shots/01-first-run.png` is that pane, with the field masked.

### 2.7d A window per app

Open puts a running app in its own window rather than taking a tab from whatever
somebody was doing. One window per app, labelled `app:<id>` so it can never
collide with the launcher's own, titled with the app's name, sized and placed
where it was last left. "In browser" is on every row and on every card, and a
Settings switch makes the browser the default.

An app's window is that app. A link to any other origin goes to the system
browser, because the window has no address bar and nobody could tell where they
had ended up. Downloads go to the Downloads folder. The window is not in any
Tauri capability, so the page inside it cannot call a single launcher command.

Measured on this Mac, all three apps from the launcher's own buttons:

| | |
| --- | --- |
| Daybreak, live news over SSE | its own window, port 8811 |
| Story Lantern | its own window, port 8421 |
| Titanium Tiiny Bot | its own window, port 7790 |
| Closing the bot's window | all three apps still running |
| Pressing Open again | the window came back at 728,148 at 1000 by 761, exactly where it was left |
| Pressing Open once more | still one window, not two |

The bot is on 7790 rather than its declared 7788 because this run was told to
keep off that port. It is a movable app, so `--port` was enough, and that is how
it was started.

localStorage is per app because each app is a different origin, which is what a
different port makes it. One caveat, honestly: the port can change between runs
when the engine moves an app off a busy one, and an app that moves loses what it
kept. Nothing was done about that, and it is worth a decision before the beta.

External links were not clicked live, because the click would have opened a
website on somebody's screen. The rule has six tests against real cases in
`src-tauri/src/appwindow.rs`, including the one that catches a lazy prefix
check: `http://localhost:84210` is not inside `http://localhost:8421`.

### 2.8 Failure, in words

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

### 2.9 The window

Eleven captures in `docs/shots/`, each of the launcher's own window taken by
window id, so nothing else that was on the screen is in any of them. The window
frame measures 1100 by 720 points, and 720 by 560 at the smallest size it allows,
where the catalog grid drops to two columns and nothing scrolls sideways.

One thing the builder alone did not do: `inner_size(1100, 720)` produced a window
of 1197 by 881 points. Saying it again with `set_size` after the window exists is
taken. The constants are `WINDOW` and `SMALLEST` in `src-tauri/src/lib.rs`, and
they are what the screenshots were measured at.

### 2.10 Tests

57 Rust tests, `cargo clippy --all-targets -- -D warnings` clean, `cargo fmt
--check` clean. The JSON reader, the progress reader, the state machine, the
failure classifier, the interpreter pin, the window registry and the rule about
where an app window may navigate all have no window behind them, so this is a
real test run rather than a compile check.

Three defects were found by something other than me reading the code.

CI found two this Mac never could, both because the repository builds
differently from a working copy.

`cargo test` could not build at all on a clean checkout. `tauri-build` refuses
when a declared resource path is missing, and `runtime` is not committed: it is
fetched and staged. It was there on this Mac and nowhere else. Both test jobs
stage it now, which also means the staging script is exercised on both platforms
before the long bundle jobs start.

The other was Windows only. The prune patterns were
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

### 2.11 What CI measured, on GitHub's runners

Both workflows are green in UNSIGNED mode on the pull request, which is what
this repository can reach before it has a single secret. Runs 34894173004
(macos) and 34894173078 (windows), on commit `7416a03`, which is the commit
that installs the engine from a git URL rather than from PyPI:

| Job | |
| --- | --- |
| cargo test (macos), fmt and clippy included | success |
| cargo test (windows) | success |
| macOS app, aarch64 | success |
| macOS app, x86_64 | success |
| Windows installer, NSIS, x64 | success |
| latest.json | skipped, because the feed job only runs on main |

The Windows numbers, measured on the runner rather than projected:

| What | Measured |
| --- | --- |
| NSIS installer | 27,621,001 bytes |
| Runtime as published, unpacked | 144.7 MB in 3,963 files |
| Runtime staged and pruned, with farm in it | 119.7 MB in 1,608 files |
| `farm --version` from the staged tree | `farm 0.1.11` |

The design page projected about 60 MB for the Windows installer. It is 27.6 MB,
smaller than the macOS disk image even though the tree inside it is twice the
size, because NSIS compresses it harder than a disk image does.

That run is also the proof that the engine pin works away from this Mac: both
platforms installed the farm from
`git+https://github.com/Titanium-Devops/tiinyapp-farm@7e4cce6` and staged it,
on runners that have never seen anybody's worktree.

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

**`farm` on the PATH is a stub.** The switch is in Settings, remembers its
answer, and says in the window that the doing of it lands later. The brief allows
a stub in this build.

---

## 4. Planned, not measured

Nothing in this section has been run.

- **Windows, by hand.** CI builds it, so the installer and the staging are no
  longer unmeasured (see 2.9). What has not happened is anybody installing that
  installer on a Windows machine and pressing a button: no window has been
  opened, no app has been planted, and the current-user install with no
  administrator prompt is a configuration rather than an observation.
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
- **The microphone, on a Tiiny with a voice model.** Section 2.7b. Everything is
  declared and nothing has been through the system prompt, because the app under
  test refuses before it reaches the microphone.
- **What an app keeps when its port moves.** An app window's localStorage is per
  origin, and the origin is the port, so an app the engine moves off a busy port
  loses what it kept. Nobody has decided whether that matters.
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
