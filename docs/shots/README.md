# What each picture is

Every one of these is the launcher's own window, captured by window id rather
than by screen region, so nothing else that happened to be on the screen is in
them. Taken on a MacBook Pro (Apple M5 Max, macOS 26.6.2) on 2026-09-14 from the
ad hoc signed 0.1.0 build against Jason's Tiiny at 172.17.7.177, with the engine
pointed at a scratch home so nothing of his was touched. Pictures 01 to 15 were
taken against engine farm 0.1.13, and 16 to 23 against farm 0.1.14.

The window is 1100 by 720 points in all of them except `10-smallest-size.png`
and `12-smallest-first-run.png`, which are the smallest size the launcher
allows, and `09-the-tray.png` and `22-tray-lost-model.png`, which are the menu
bar menu rather than the window.

| Picture | What it shows |
| --- | --- |
| `01-first-run.png` | No Tiiny on file, so the launcher went looking. Jason's Tiiny, found over the USB cable, with one Use button, and the key step underneath: one masked field, nothing that asks for a file. The hardware serial is blacked out. |
| `02-the-farm.png` | The live catalog from tiinyapp.farm, with each app's art and badge. |
| `03-the-card.png` | What an app needs and what it can reach, before anything downloads. |
| `04-installing.png` | An install in progress, in the engine's own words. |
| `05-running.png` | Three apps running, each with Open, In browser, Stop, Log and Remove. Story Lantern is on 8421 because something else on this Mac holds 8420 and the engine moved it; the bot is on 7790 because 7788 is off limits for this run. |
| `06-nothing-planted.png` | The Running pane with nothing installed. |
| `07-settings-and-doctor.png` | Settings, and `farm doctor` rendered as sentences with their fix lines. The first line names the interpreter the launcher runs apps with, and the third says the Tiiny answered it in 2 ms. The third switch, Open apps in your browser, is the one that turns the per-app window off. |
| `08-failure-in-words.png` | A checksum mismatch. Nothing was unpacked and the panel says so. |
| `09-the-tray.png` | The menu bar menu: what is running, with Open, Stop and Update behind it. |
| `10-smallest-size.png` | The window at 720 by 560, the smallest it goes. The grid drops to two columns. |
| `12-smallest-first-run.png` | The same size, on the first run. The found row puts its button underneath rather than running off the side. |
| `13-app-window-daybreak.png` | Daybreak in its own window, titled Daybreak, not a tab in anybody's browser. |
| `14-app-window-story-lantern.png` | Story Lantern in its own window. |
| `15-app-window-tiiny-bot.png` | Titanium Tiiny Bot in its own window. |
| `11-the-log.png` | A stopped app's own `farm.log`. The Tiiny's hardware serial is blacked out; it is the device's, not the launcher's, and it does not need to travel in a public repository. |
| `16-models-loaded.png` | The Models pane. What the Tiiny has loaded right now, with the kind, the cost in NPU units and the state on each, and the unit budget above them: 32 of 100 free, drawn as a bar. |
| `17-models-on-disk.png` | The rest of the same pane: everything downloaded and not loaded. Each row carries a Load button, and each model that costs more than the Tiiny has free says how much more. The buttons are all disabled in this build, because farm 0.1.14 can load a model only as part of starting an app that needs it. |
| `18-card-needs-met.png` | Daybreak's card with the chat model loaded. The needs line is one chip per kind, green because it is there, and the Better with line underneath carries the three kinds the app prefers: embedding and image are loaded, rerank is not, and none of them stops it starting. |
| `19-card-needs-unmet.png` | The same card 4 seconds after the chat model was stopped on the Tiiny. Nobody touched the window: the chip went red and the sentence underneath says what is missing, in words. |
| `20-running-needs-unmet.png` | The Running pane in the same state. Daybreak and Titanium Tiiny Bot have a greyed Start with the reason beside it and a Load and start button next to it. Story Lantern is still running and carries the badge saying the model it was using is not loaded any more. |
| `21-load-and-start-refused.png` | What Load and start does on farm 0.1.14. The engine answers a JSON start with the missing kinds before it tries to load anything, so the load never happens, and the panel says exactly that rather than leaving somebody pressing the button twice. |
| `22-tray-lost-model.png` | The menu bar in the same state: `story-lantern  port 8421  no chat model loaded`. The suffix disappears on its own when the model comes back. |
| `23-needs-met-again.png` | The Running pane 2 seconds after the chat model was loaded again. Every refusal, badge and Load and start button has gone, and Start is live. The Tiiny was left with the same four models loaded, the same 68 of 100 units used and the same 16 models on disk it had before any of this. |

Three serials are blacked out, in `01`, `11` and `12`. They identify one person's
device, they are not something the launcher handles, and they do not need to
travel in a public repository. Nothing else in any picture is edited.

The checksum mismatch in `08` was made on purpose, by pointing the engine at a
local copy of the catalog with one byte of Story Lantern's SHA-256 changed. The
archive on GitHub was not touched. Everything else is the real catalog and a
real install.
