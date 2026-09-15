# Changelog

What changed in each release of the Tiiny App Farm launcher, newest first. The
release history page on tiinyapp.farm is built from this file, so every entry is
written for somebody deciding whether to update rather than for somebody reading
the diff.

This project follows [Keep a Changelog](https://keepachangelog.com/en/1.1.0/).

## 0.1.1 - 2026-09-15

### Added

- **Linux.** An AppImage for x86_64, carrying the same bundled Python and the
  same `tiinyapp-farm` engine as the other two. One file, made executable and
  run; nothing to install first.
- **A release history.** Every release, its notes and every file it shipped are
  published as `launcher/releases.json`, so older versions stay reachable rather
  than being replaced by the newest one.

### Changed

- The card for an app says which port it will really come up on when that is
  not the port its manifest asks for, whether the app is running or stopped.
- The engine inside is `tiinyapp-farm` 0.1.17, which remembers the port an app
  last used and tries it again before the one the manifest names.

### Known limits

- The AppImage is **not signed**. Linux has no notarisation and nothing a
  stranger's machine would check a desktop signature against. The SHA-256 of
  every file is published with the release and is what can be verified.
- The AppImage has been built but **never run on a Linux desktop** by anybody on
  this team. The tray, the deep link and the window are untested there.
- The updater covers macOS only. A Linux update means downloading the new
  AppImage.

## 0.1.0 - 2026-09-14

The first release.

### Added

- **A window for the farm.** Browse the catalog on tiinyapp.farm, see what an
  app needs and what it can reach before anything downloads, and install, start,
  stop, update and remove it without opening a terminal.
- **It carries its own Python.** CPython 3.11.16 and the `tiinyapp-farm` engine
  are inside the app, so there is nothing to install first: no Python, no Node,
  no package manager.
- **A Models pane.** What your Tiiny has loaded right now, what is on its disk,
  and what the NPU budget is. A model that fits can be loaded from the window; a
  model that does not says how much more it needs.
- **What each app needs, in words.** Every app card says which kinds of model it
  needs and whether your Tiiny has them, and an app that cannot run says why
  instead of failing after it starts. Load and start loads the model it needs
  and then starts the app, and says which one it loaded.
- **A running app that loses its model says so**, on its row and in the menu
  bar, within a few seconds of the model going away.
- **Each app opens in its own window**, with its own name on it, rather than
  taking a tab in whatever browser happens to be open.
- **The menu bar** keeps the list of what is running when the window is closed.

### Known limits

- macOS and Windows only.
- The Windows installer in this release is **not signed**, so Windows warns
  about it. The macOS disk images are signed, notarised and stapled.
- One Tiiny at a time.
