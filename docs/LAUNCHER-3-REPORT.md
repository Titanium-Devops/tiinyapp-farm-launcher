# LAUNCHER-3: the Load buttons work

Built 2026-09-14 on top of LAUNCHER-2 (`f301dd4`), against the engine side of
MODELS-2.

**Every number below was measured on one machine: a MacBook Pro, Apple M5 Max,
macOS 26.6.2 (25G83), arm64, against Jason's Tiiny at 172.17.7.177.** Nothing
here was measured on Windows or on any other machine. What was not measured is
in section 8 and is labelled as such.

---

## 1. What changed

LAUNCHER-2 shipped a Models pane whose Load buttons were all dead, because farm
0.1.14 could only load a model as part of starting an app that needed one. That
is fixed in the engine, and this is the window catching up.

| Before | Now |
| --- | --- |
| Every Load button disabled, with the engine's limit as its tooltip | A live Load button on every downloaded model that fits |
| A model too big for the free units had a disabled button like all the others | It has no button at all, and says how much more it needs |
| Load and start ran, failed, and explained that the engine could not do it | Load and start loads a model and then starts the app, and says which model it loaded |

Whether a model fits is the engine's rule, in `load_one`, and it refuses in its
own words if anything ever gets past the window. The window does not work it out
a second time: `Snapshot::short_by` in `src-tauri/src/models.rs` says how much
each downloaded model is short by, the pane reads that map, and five tests pin
it, including the two edges that matter. A model that costs exactly what is free
fits, and a model whose cost the device does not report is never called too big,
because refusing on a number nobody has would be worse than trying.

---

## 2. Proved against Jason's Tiiny, and put back

Recorded before anything was touched, and again at the end:

| | Before | After |
| --- | --- | --- |
| NPU units used | 68 of 100 | 68 of 100 |
| Models loaded | 4 | 4 |
| Models on disk | 16 | 16 |

Compared item by item, three times: after the model that was loaded by hand was
taken off again, after Load and start, and at the end. Identical every time.

Two models were moved during the work and both were put back. `zai-org/GLM-OCR`,
4 units, was loaded from the pane and then unloaded. `Qwen/Qwen3-8B`, the chat
model, was unloaded so that an app would refuse to start, and Load and start put
it back by itself, which is the cleanest restore of the lot: the product undid
the change.

---

## 3. Loading one model, measured

| | |
| --- | --- |
| Load pressed on `zai-org/GLM-OCR` | 0.0 s |
| The button read Loading, continuously, for | 13 s and counting |
| The device first reported it loaded | 30.8 s |

Thirty seconds is the device putting a model in the NPU, not the window waiting
on itself. The button says Loading for the whole of it and nothing else on the
pane is disabled, so somebody can still read what is there.

What the pane did with the four units, with nothing typed:

| Model | Before the load | After it |
| --- | --- | --- |
| free units | 32 of 100 | 28 of 100 |
| `openai/gpt-oss-20b`, 32 units | a live Load button | no button, "4 more than are free" |
| `Qwen/Qwen3.6-35B-A3B`, 47 units | "15 more than are free" | "19 more than are free" |
| `deepreinforce-ai/Ornith-1.0-35B`, 50 units | "18 more than are free" | "22 more than are free" |

`docs/shots/24-models-load-live.png` and `docs/shots/25-models-after-a-load.png`
are those two states.

---

## 4. Load and start, measured

The chat model was unloaded, which left three apps refusing to start with the
reason beside a greyed Start and a Load and start button next to it. Then Load
and start was pressed.

| | Daybreak | Story Lantern |
| --- | --- | --- |
| Pressed | 0.0 s | 0.0 s |
| The app was running | 13.6 s | 13.7 s |
| What it loaded first | `Qwen/Qwen3-8B` | `Qwen/Qwen3-8B` |

The window says so rather than leaving it to be discovered: "Story Lantern is
running on port 8421. It loaded Qwen/Qwen3-8B on your Tiiny first."
(`docs/shots/27-load-and-start.png`). The name comes from the engine's own
`loaded` list, so the window is not guessing which model was picked.

Both times the farm chose `Qwen/Qwen3-8B` at 28 units over the 32 unit
`openai/gpt-oss-20b`, which is `pick_model` taking the cheapest that fits, and
which is why the device ended the run exactly as it started it.

---

## 5. What was left alone, on purpose

**There is no Unload button.** The engine grew `farm models --unload <id>` in
the same change, and it works: the operator side of this proof used it. It is
not in the window, because nothing in the brief asked for it and taking a model
out of somebody's NPU is a different kind of decision from putting one in. The
refusal when a model will not fit says what is in the way rather than offering
to clear it.

**The watch still stops when the launcher quits.** The brief said to remove the
watcher-leak workaround if I added one. What LAUNCHER-2 added is not a
workaround: the app kills the watch child it started, on `RunEvent::Exit`. The
engine's new behaviour, exiting when nobody is reading, covers the different
case where the launcher is killed outright and never runs that code. Both are
kept because each covers what the other cannot: without the Exit hook a clean
quit leaves the child alive until the next model change, which may never come.
This was flagged to the team lead rather than decided quietly.

---

## 6. The build in Jason's Applications folder

`~/Applications/Tiiny App Farm.app` is the build of **main at `f301dd4`**, which
is the merge of LAUNCHER-2. That is the build with the Models pane, the needs
lines and the badges, and it carries farm 0.1.14, so its Load buttons are the
dead ones this pull request fixes.

It was replaced with a development build for the live proof above and put back
afterwards from a copy taken before any of it started. A development build was
never left in place at the end of a session.

---

## 7. Checks

One thing on this Mac is worth writing down, because it stops every build cold
and it is not a code problem. Xcode 27.0 became the selected toolchain partway
through this work and its licence has not been accepted, so `cc`, `cargo` and
`/usr/bin/git` all fail with "You have not agreed to the Xcode license
agreements". Accepting it needs `sudo xcodebuild -license accept`, which is the
machine owner's to run. Everything below was built and run with
`DEVELOPER_DIR=/Library/Developer/CommandLineTools`, which points each process
at the Command Line Tools that are already installed and changes nothing on the
machine.

| Check | Result |
| --- | --- |
| `cargo test` | 84 passed, 0 failed |
| `cargo clippy --all-targets -- -D warnings` | clean |
| `cargo fmt --check` | clean |
| `npm run tauri build` | app and dmg built, ad hoc signed |

---

## 8. Not measured

- Windows. Nothing in this pull request was run on Windows.
- An Intel Mac.
- Loading a model while an app that needs it is starting. The two paths do not
  share a lock in the window, and the engine takes its own.
- A load that the device refuses halfway. Every load attempted here succeeded,
  so the failure sentence has been read in the code and not on a screen.
