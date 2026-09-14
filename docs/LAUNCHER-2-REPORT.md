# LAUNCHER-2: the models pane and per-app needs

Built 2026-09-14 on top of the merged LAUNCHER-1 work (`f0105af`), against farm
0.1.14 from PyPI, which is the engine side of farm PR 34.

**Every number below was measured on one machine: a MacBook Pro, Apple M5 Max,
macOS 26.6.2 (25G83), arm64, against Jason's Tiiny at 172.17.7.177.** Nothing
here was measured on Windows or on any other machine. What was not measured is
in section 9 and is labelled as such.

---

## 1. What was asked for, and what is there

| Asked for | Where it is | Proved by |
| --- | --- | --- |
| A Models pane: loaded models as cards with kind, units and state | `ui/index.html`, `renderModels` in `ui/app.js` | `docs/shots/16-models-loaded.png` |
| Free units as a bar | Same pane, above the list | Same picture: 32 of 100 free |
| Downloaded and not loaded, with a Load button each, guarded by free units | Same pane, `modelRow` in `ui/app.js` | `docs/shots/17-models-on-disk.png` |
| A Needs line on every app card, each kind marked loaded or not | `renderCardModels` in `ui/app.js`, decided in `src-tauri/src/models.rs` | `18-card-needs-met.png`, `19-card-needs-unmet.png` |
| A Better with line for prefers | Same function | `18-card-needs-met.png` |
| Start disabled with the reason, and a Load and start button | `renderCardState` in `ui/app.js` | `20-running-needs-unmet.png` |
| One `farm models --watch --json` child while a window is open, restarted if it dies | `Watch` in `src-tauri/src/lib.rs`, `spawn_streaming` in `src-tauri/src/engine.rs` | Section 3 |
| A badge on a running app whose needed model was unloaded | `lostSentence` in `ui/app.js` | `20-running-needs-unmet.png` |
| The same in the tray menu | `src-tauri/src/tray.rs` | `22-tray-lost-model.png` |
| Never load a model without a click | Nothing in the launcher ever calls a load | Section 5 |

The one decision the window makes, whether Start can be pressed, is made in Rust
in `src-tauri/src/models.rs` and nowhere else. The window draws what that
decision says. A need is matched by kind or by model id, the same rule the
engine uses in `met_by`, so the two cannot disagree about the same app.

---

## 2. The live proof, and the Tiiny put back

Recorded before anything was touched, and again at the end:

| | Before | After |
| --- | --- | --- |
| NPU units used | 68 of 100 | 68 of 100 |
| Models loaded | 4 | 4 |
| Models on disk | 16 | 16 |

The loaded set, the unit figures and the disk list were compared item by item
and were identical. The four loaded models were `Qwen/Qwen3-8B` (chat, 28
units), `Qwen/Qwen3-Embedding-0.6B` (embedding, 1), `Tongyi-MAI/Z-Image-Turbo`
(image, 32) and `Qwen/Qwen3-TTS-12Hz-1.7B-CustomVoice` (tts, 7).

Two models were stopped and started during the work, both times the same one,
`Qwen/Qwen3-8B`, and both times through the farm's own gateway so that the key
was handled the farm's way: read in process, sent in a header, never in a path,
an argument or a log. The tool that did it is an operator tool in the scratch
directory and is not part of the launcher.

---

## 3. What the watch costs, measured

The engine polls the device every three seconds and prints one JSON object per
change. A second, timestamped copy of the same watch was run beside the
launcher's, so the delay between the Tiiny changing and the engine saying so
could be read rather than guessed.

Three changes were timed.

| Change | Counted from | The engine said so after | The window was showing it by |
| --- | --- | --- | --- |
| `Qwen/Qwen3-8B` loaded | the device first reporting it loaded | 2.79 s | 3.83 s |
| `Qwen/Qwen3-8B` unloaded | the stop being asked for | 0.46 s | 4.59 s |
| `Qwen/Qwen3-8B` loaded again | the device first reporting it loaded | 1.84 s | 3.98 s |

A load takes the device several seconds, so the two load rows are counted from
the moment the device itself said the model was loaded, which was polled twice a
second. An unload is immediate, so that row is counted from the moment the stop
was asked for, and the device was not polled on that leg.

The engine column is its own detection lag and is bounded by its three-second
interval. The window column is when a screenshot was taken and found the window
already correct, so it is an upper bound on the window's delay and not a
measurement of the repaint.

The watch was then killed outright while the launcher was running. A new one was
in its place six seconds later with a different process id, which is the two
second pause plus the time the loop takes to notice. There was one watch before
and one after, never two.

---

## 4. Two bugs this proof found, both fixed here

**The window was holding a snapshot the watch never moved.** The watch read
each line, emitted `models:change` and refreshed the tray, but never folded the
change into the snapshot the window reads. `app_needs` then answered from a
snapshot that had not moved since the last full device read, so a model coming
or going was invisible until something else happened to read the whole device.
Measured before the fix: the chat model was stopped at one point and the card
still said `chat, loaded` 32 seconds later. `models::fold` now lands the change
in the held snapshot before anything is told to look again, and two tests in
`src-tauri/src/models.rs` fail if that call goes away.

**The watch outlived the launcher.** Quitting left the interpreter the watch
runs in polling the Tiiny for ever; three of them were found alive at once
during this work. The app now stops the watch on `RunEvent::Exit`. Verified:
quitting from the menu bar leaves no child behind. A launcher that is killed
outright still leaks one, and that needs the engine's help, which is in section
6.

---

## 5. Nothing loads a model without a click

There is no code path in the launcher that loads a model on its own. The watch
only reads. `farm_start` passes `--load` only when the window sends `load: true`,
and the window sends it only from the Load and start button. There is a comment
in `src-tauri/src/lib.rs` where a per-model load command would go, saying why
there is not one.

---

## 6. Three things the engine cannot do yet

These are farm 0.1.14 findings, not launcher work, and each one is something the
launcher has had to say out loud rather than hide.

**1. `farm models` has no per-model load.** There is no `farm models --load <id>`
and no other command that loads one model by name. The only way a model gets
loaded is as part of starting an app that needs it. So every Load button in the
Models pane is disabled, with the reason on the button. The alternative was to
reach past the engine to the device from the launcher, which would mean two
things in the product that can load a model and two sets of rules about it. The
free-unit guard is real and visible either way: a model that costs more than the
Tiiny has free says how much more, in red, on its own row.

**2. `farm start --load --json` does not load.** In `check_models`, the line
`if as_json: return missing` runs before the loop that offers to load, so
`--load` only ever works on the prose path. The window says exactly that when
somebody presses Load and start, rather than leaving them pressing it twice:
`docs/shots/21-load-and-start-refused.png`.

**3. `farm models --watch` does not notice that nobody is reading it.** It only
writes when something changes, so it never gets a broken pipe, and a launcher
that is killed rather than quit leaves it running and polling for ever. If the
watch checked for its reader being gone it would exit on its own.

---

## 7. The gate, and what it can no longer do here

`scripts/verify-launcher.mjs` drives the built app's engine against the real
Tiiny in a scratch home. Against farm 0.1.13 it passed all ten checks. Against
farm 0.1.14 it is **7 of 10** when run from a terminal on macOS, and the three
it cannot finish are start, status and stop.

The reason is not a launcher regression. macOS gives local network access to an
application, not to a file. The interpreter inside the bundle has that access
when the launcher started it, and does not have it when a shell did. Since
0.1.14 a start asks the Tiiny what it has loaded, so those three checks now need
the access the shell cannot give. The engine's own words, from the same
interpreter and the same scratch home:

```
farm: This Python cannot reach your Tiiny, because macOS is blocking it from
your local network. Run farm doctor.
```

Two changes were made to the gate, neither of which weakens it:

- A refusal used to print `port undefined, undefined`, because the gate read the
  fields off an error object. It now raises the engine's own sentence, so the
  reason is on the line.
- When a check fails for want of the device, the gate asks the engine why and,
  if the answer names the local network, prints that answer and explains whose
  grant it is.

The gate still exits non-zero. A skipped check that reads as a pass would be
worse than a failure somebody has to read.

Those three were proved by hand instead, through the running app, which is where
the access exists: Story Lantern started and ran on port 8421 for 28 minutes,
appeared in the Running pane and the menu bar the whole time, and stopped when
Stop was pressed. `20-running-needs-unmet.png` and `23-needs-met-again.png` are
that app running.

---

## 8. Checks

| Check | Result |
| --- | --- |
| `cargo test` | 76 passed, 0 failed |
| Of those, in `models.rs` | 18 |
| `cargo clippy --all-targets -- -D warnings` | clean |
| `cargo fmt --check` | clean |
| `npm run tauri build` | app and dmg built, ad hoc signed |
| `scripts/verify-launcher.mjs` against the real Tiiny | 7 of 10, section 7 |

The 18 tests in `src-tauri/src/models.rs` cover the watch line parser, the needs
state machine and the fold into the held snapshot. Their fixture is the real
state of Jason's Tiiny, copied out of `farm models --json` rather than invented,
so a change in what the engine answers shows up as a failing test rather than as
a window that quietly says the wrong thing.

---

## 9. Not measured

- Windows. Nothing in this pull request was run on Windows. The watch child is
  spawned the same way as every other engine call, which is covered by the
  existing `CREATE_NO_WINDOW` path, but that is an argument and not a
  measurement.
- An Intel Mac.
- A Tiiny with no models loaded at all, or a Tiiny that goes away mid-watch. The
  restart path was exercised by killing the watch child, not by taking the
  device away, so what the window says while the device is gone is untested.
- What happens when a model is loading rather than loaded. The engine reports a
  `changed` event with a state, and `apply` keeps it, but no model was caught
  mid-load during this work.
