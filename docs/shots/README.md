# What each picture is

Every one of these is the launcher's own window, captured by window id rather
than by screen region, so nothing else that happened to be on the screen is in
them. Taken on a MacBook Pro (Apple M5 Max, macOS 26.6.2) on 2026-09-14 from the
ad hoc signed 0.1.0 build against Jason's Tiiny at 172.17.7.177, with the engine
pointed at a scratch home so nothing of his was touched. Pictures 01 to 15 were
taken against engine farm 0.1.13, 16 to 23 against farm 0.1.14, 24 to 27 against farm 0.1.15
from PyPI, 28 against farm 0.1.17, and 29 on the
published release itself.

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
| `24-models-load-live.png` | The downloaded models with the Load buttons live. Anything that fits the 32 NPU units free has a button; anything that does not says how much more it needs and offers no button at all, because a button that cannot work is worse than a sentence. |
| `25-models-after-a-load.png` | The same list a minute later, after Load was pressed on the 4 unit OCR model. It has gone from the list because it is loaded now, and every shortfall underneath it has grown by four: `openai/gpt-oss-20b` had a live button at exactly 32 units and now says it is 4 short. Nothing was typed to make that happen. |
| `26-npu-after-a-load.png` | The budget at the top of the same pane, 28 of 100 free, with the bar moved to match. |
| `27-load-and-start.png` | Load and start on an app whose chat model was not loaded. The farm loaded one and then started the app, and the window says which one: Daybreak is running on port 8811, and it loaded Qwen/Qwen3-8B on the Tiiny first. |
| `28-usual-port.png` | Daybreak's card while it is stopped, after a run on a port its manifest does not ask for. The needs line says the 8811 it asks for; the line under it says it usually comes up on 8813, because that is where it ran last. Made by starting it with --port 8814, stopping it, and opening the card, which is the case that matters: where an app will come up is a question somebody asks before pressing Start. When the two numbers agree the second line is not drawn. |
| `29-gatekeeper-first-open.png` | What a person meets the first time they open the download: macOS asking whether to open an app from the internet, and saying "Apple checked it for malicious software and none was detected", which is the notarisation answering in the dialog. Cropped to the alert; the rest of that screen was somebody's desktop. |
| `30-badges-and-seeds.png` | The farm at the default window, 1100 by 720. Daybreak and Tiiny Brain carry the New badge in their top left corner, because this copy of the launcher had never drawn either of them before. Story Lantern carries Update available in its top right, because 0.1.1 is planted here and the catalog has 0.1.3. Beside every Planted chip is the seed stack: where nobody has given that app a seed yet it is a husk with No seeds yet written beside it, because on a farm where almost everything is at zero a lone faint dot reads as a rendering fault rather than as an answer. Made in a scratch home with Story Lantern's installed manifest wound back to 0.1.1; the catalog itself was not touched. |
| `31-badges-smallest-size.png` | The same screen at the smallest the window goes, 720 by 560. Two columns, and both corners still read over the art. Story Lantern's footer shows Planted 0.1.1 with its seed stack beside it, and Tiiny Brain still carries New. |
| `32-seed-stack-shapes.png` | Every shape the seed stack takes, drawn by the real page and the real stylesheet against the shapes Rust decides: a husk and the words at zero, one to three seeds in a row, four to nine in two rows with the wider one underneath, and a full pile with the number beside it from ten up. The counts above nine are a harness; the rest of this picture is the live catalog. |
| `33-update-banner.png` | The launcher saying a newer launcher is ready, at the default window. Made with a pretend feed on a local port claiming 9.9.9, because a real self-update cannot be seen until the release after the one that adds it: the feed's newest version is this one. Cropped to the top of the window; the rest of that screen showed a device serial. |
| `34-update-banner-smallest.png` | The same banner at 720 by 560, the smallest the window goes. The sentence and both buttons still sit on one strip. |
| `35-update-refused.png` | What an update signed with the wrong key looks like: the banner is still there, the app has not crashed, and the words say what happened. Made by pointing the feed at a file signed with a key that is not the launcher's, which is the failure that matters most, because an update that installed without that check would be somebody else's program. Two crops of one screen, the banner and the message; the serial between them is not in the picture. |
| `36-about-window.png` | The About window, at the 420 by 715 it is fixed at, opened from the About item in the menu bar. The version and the date under the name are read from the build and from CHANGELOG.md rather than typed: this is a real 0.1.4 saying so. Every row below is a door that opens in the person's own browser, and the four marks are the real ones from the farm's brand folder, with GitHub's octicon for the two GitHub rows. |
| `37-about-menu-item.png` | The application menu, with About Tiiny App Farm where macOS used to put its own panel. The rest of that menu is what Tauri's default carried, written out by hand because a menu is set whole rather than edited. Cropped to the menu; the rest of that screen was somebody's desktop. |
| `38-about-newest-version.png` | The same window after Check for updates was pressed against the live feed at tiinyapp.farm. The answer is a line under the buttons, and the line keeps its place in the layout whether or not it has anything in it, so pressing the button moves nothing above it. This build is 0.1.4 and the feed's newest is older, which is why the answer is that this is the newest one. |

Three serials are blacked out, in `01`, `11` and `12`. They identify one person's
device, they are not something the launcher handles, and they do not need to
travel in a public repository. Nothing else in any picture is edited.

The checksum mismatch in `08` was made on purpose, by pointing the engine at a
local copy of the catalog with one byte of Story Lantern's SHA-256 changed. The
archive on GitHub was not touched. Everything else is the real catalog and a
real install.
