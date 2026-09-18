# Changelog

What changed in each release of the Tiiny App Farm launcher, newest first. The
release history page on tiinyapp.farm is built from this file, so every entry is
written for somebody deciding whether to update rather than for somebody reading
the diff.

This project follows [Keep a Changelog](https://keepachangelog.com/en/1.1.0/).

## 0.1.4 - 2026-09-18

### Added

- **An About window that says who made this and where it lives.** The panel
  macOS used to put up named the app, its version and a copyright line, and
  nothing in it went anywhere. This one is the whole credit roll: Titanium
  Computing, who build and keep the farm, and Jason Brashear, who wrote it, on
  GitHub and on his own site. Tiiny, the AI Pocket Lab every app here talks to.
  The farm itself, and Titanium Bot beside it, the same pair the farm's own
  footer carries. The launcher's source on GitHub and the licence it is
  published under. Every one of them has its mark beside it, says in one line
  what it is, and opens in your own browser rather than inside this app. It is
  on the About item in the menu bar, in the menu bar icon's menu, and in
  Settings.
- **Check for updates, on a button.** The launcher has looked for a newer
  version on its own since 0.1.2, when it opens and every four hours after.
  Now you can also just ask, and be told the answer in words: either you are on
  the newest one, or the strip at the top of the farm has the button that
  installs the one that is waiting.
- **Show the log.** The launcher writes down the few things it decides not to
  interrupt you about, and this puts that file in front of you. A launcher that
  has had nothing to say says that instead.

### Changed

- The version and the release date in the About window are read from the build
  and from this file, so there is no number in that window anybody has to
  remember to change.

## 0.1.3 - 2026-09-18

### Changed

- **This release exists to prove the updater.** 0.1.2 added the mechanism that
  looks for a newer version and offers to update and restart, but the newest
  version its feed could offer was itself. 0.1.3 is the first version a 0.1.2
  launcher can see, download, verify and install on its own. Nothing else has
  changed. If your launcher put up "Tiiny App Farm 0.1.3 is ready" and came back
  as 0.1.3, the updater works.

## 0.1.2 - 2026-09-18

### Added

- **Update available, on the card.** An app you have planted that the catalog
  has moved past says so in the top right corner of its card, over the art,
  rather than only inside the card. Opening it still shows the same Update
  button that was always there.
- **New, on the card.** An app this launcher has never put on your screen
  before carries a New badge in its top left corner until you open it. The
  first run of a new install marks the whole farm seen without a word, so
  nobody's first look is a wall of badges, and an app you have already met does
  not become New again when it publishes a new version.
- **A stack of seeds.** Every card says how many seeds the farm has given that
  app, as a little pile beside the Planted chip that grows with the count: an
  outline where nobody has given one yet, then the seeds themselves, and from
  ten up a full pile with the number. It is read once per look at the farm and
  never holds the cards back.
- **The launcher updates itself.** It looks for a newer version when you open
  it and every four hours it stays open. When one is waiting, a quiet strip at
  the top of the farm says so, with one button that downloads it, installs it
  and starts the new one. Not now puts that version away for good; the one
  after it asks again. The menu bar carries the same line. A launcher that
  cannot reach the farm says nothing at all rather than interrupting you about
  it.
- **Windows and Linux can update themselves too**, not only macOS. Every
  platform's update carries a signature that is checked before anything is
  installed, and an update whose signature is not ours is refused and says so
  rather than being installed quietly.

### Known limits

- The seed count is read only. Giving an app a seed is still something you do
  on tiinyapp.farm; a later version brings it into the window.
- The count comes from the farm, so a launcher with no network draws the cards
  with no piles on them rather than waiting.
- **A real self-update is first proven at 0.1.3.** This is the release that
  adds the mechanism, so the newest version the feed can offer it is itself.
  What has been proven here is every part that can be: the banner, the refusal
  of an update signed with the wrong key, and that each platform's signature
  belongs to the exact file published. The download and restart on a real
  release is witnessed at the next one.
- Nobody on this team has yet run the Linux AppImage on a Linux desktop, so its
  self-update is signed and verified but unexercised, the same as the AppImage
  itself has always been.

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
