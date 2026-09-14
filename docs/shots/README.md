# What each picture is

Every one of these is the launcher's own window, captured by window id rather
than by screen region, so nothing else that happened to be on the screen is in
them. Taken on a MacBook Pro (Apple M5 Max, macOS 26.6.2) on 2026-09-14 from the
ad hoc signed 0.1.0 build, against Jason's Tiiny at 172.17.7.177, with the
engine pointed at a scratch home so nothing of his was touched.

The window is 1100 by 720 points in all of them except the last, which is the
smallest size the launcher allows.

| Picture | What it shows |
| --- | --- |
| `01-first-run.png` | No Tiiny on file, so the launcher went looking. Jason's Tiiny, found over the USB cable, with one Use button. The hardware serial is blacked out. |
| `02-the-farm.png` | The live catalog from tiinyapp.farm, with each app's art and badge. |
| `03-the-card.png` | What an app needs and what it can reach, before anything downloads. |
| `04-installing.png` | An install in progress, in the engine's own words. |
| `05-running.png` | Story Lantern running. The port is 8421 because something else on this Mac holds 8420 and the engine moved it. |
| `06-nothing-planted.png` | The Running pane with nothing installed. |
| `07-settings-and-doctor.png` | Settings, and `farm doctor` rendered as sentences with their fix lines. The first line names the interpreter the launcher runs apps with, and the third says the Tiiny answered it in 9 ms. |
| `08-failure-in-words.png` | A checksum mismatch. Nothing was unpacked and the panel says so. |
| `09-the-tray.png` | The menu bar menu: what is running, with Open, Stop and Update behind it. |
| `10-smallest-size.png` | The window at 720 by 560, the smallest it goes. The grid drops to two columns. |
| `11-the-log.png` | A stopped app's own `farm.log`. The Tiiny's hardware serial is blacked out; it is the device's, not the launcher's, and it does not need to travel in a public repository. |

Two serials are blacked out, in `01` and `11`. They identify one person's
device, they are not something the launcher handles, and they do not need to
travel in a public repository. Nothing else in any picture is edited.

The checksum mismatch in `08` was made on purpose, by pointing the engine at a
local copy of the catalog with one byte of Story Lantern's SHA-256 changed. The
archive on GitHub was not touched. Everything else is the real catalog and a
real install.
