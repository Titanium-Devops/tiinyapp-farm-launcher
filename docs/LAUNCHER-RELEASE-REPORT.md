# The first release, and the night it took

Tiiny App Farm 0.1.0, built from main at `238c9a8`, signed, notarised, stapled
and published to tiinyapp.farm on 2026-09-14.

**Every number below was measured on one machine: a MacBook Pro, Apple M5 Max,
macOS 26.6.2 (25G83), arm64.** What could not be measured is in section 5 and
says so.

---

## 1. What is on the site

| File | Key | Cached |
| --- | --- | --- |
| Apple silicon, for the download button | `launcher/Tiiny-App-Farm.dmg` | five minutes |
| Intel, for the download button | `launcher/Tiiny-App-Farm-Intel.dmg` | five minutes |
| Windows, for the download button | `launcher/Tiiny-App-Farm-Setup.exe` | five minutes |
| Apple silicon, for the updater | `launcher/Tiiny-App-Farm_0.1.0_aarch64.dmg` | a year |
| Intel, for the updater | `launcher/Tiiny-App-Farm_0.1.0_x64.dmg` | a year |
| Windows, versioned | `launcher/Tiiny-App-Farm_0.1.0_x64-setup.exe` | a year |
| The two updater bundles and their signatures | `launcher/tiinyapp-farm-launcher_0.1.0_<arch>.app.tar.gz[.sig]` | a year |
| The feed | `launcher/latest.json` | never |

All four public URLs answer 200 with the content type the worker intends. The
feed names both architectures and both bundles it points at answer 200.

`site/launcher.json`, for the farm repository:

```json
{
  "enabled": true,
  "version": "0.1.0",
  "mac": "Tiiny-App-Farm.dmg",
  "macIntel": "Tiiny-App-Farm-Intel.dmg",
  "windows": "Tiiny-App-Farm-Setup.exe"
}
```

## 2. What was checked before anything was uploaded

`scripts/publish-release.mjs` refuses to upload until every one of these passes,
and all of them did:

```
feed     darwin-aarch64 -> tiinyapp-farm-launcher_0.1.0_aarch64.app.tar.gz, signature matches
feed     darwin-x86_64  -> tiinyapp-farm-launcher_0.1.0_x86_64.app.tar.gz, signature matches
macos    Tiiny App Farm_0.1.0_aarch64.dmg signed, notarised, stapled, sha256 b339e69746eb103b
macos    Tiiny App Farm_0.1.0_x64.dmg     signed, notarised, stapled, sha256 62301dbbe5ccd4eb
windows  Tiiny App Farm_0.1.0_x64-setup.exe sha256 d8a02af90819f9b8 (Authenticode Valid)
```

Every file was checked against the `SHA256SUMS` its building machine wrote, and
the disk image downloaded from the site afterwards has the same sha256 as the
one that was notarised. `latest.json` went up last, so the feed never named a
download that was not there.

**Apple said nothing.** The notary accepted both disk images without comment.

## 3. The release is good, and this is how that is known

Measured after publication, on the bits the site serves:

- `spctl -a -t open --context context:primary-signature` accepts the disk image
  as `Notarized Developer ID`, with and without a quarantine flag on it.
- `xcrun stapler validate` passes on the image and on the app.
- `codesign --verify --deep --strict` passes and the app satisfies its
  designated requirement, signed `Developer ID Application: Jason Brashear
  (F2DH8T4BVH)`.
- The bundled interpreter carries the hardened runtime with exactly the three
  entitlements it needs: `allow-jit`, `allow-unsigned-executable-memory`,
  `disable-library-validation`.
- Run from a clean mount of the downloaded image, `python3.11 -m farm.farm
  --version` answers `farm 0.1.17`.
- Installed clean into `/Applications` with nothing touched afterwards, the app
  opened with a window, drew the live catalog, and started its
  `models --watch --json` child against the engine.

`docs/shots/29-gatekeeper-first-open.png` is what a person sees the first time
they open it after downloading:

> "Tiiny App Farm" is an app downloaded from the Internet. Are you sure you want
> to open it? Safari downloaded this file today. Apple checked it for malicious
> software and none was detected.

That sentence is the notarisation answering in the dialog itself, which is the
proof worth having. Clicking Open once is the whole of the first-run experience.

## 4. What is not proved, and why

**The first launch from a quarantined download has not been completed on this
machine.** The prompt appears and the app waits behind it, correctly. It could
not be accepted from here: macOS refuses synthetic events on that alert by
design, which is the right behaviour and not a defect.

Worse, this Mac's Gatekeeper daemon is wedged tonight. The control that settles
it is not our app at all: Apple's own notarised Google Chrome, copied aside and
given the same quarantine flag, hangs for the full 180 second cap on
`--version`, while the identical binary without the flag answers instantly. If
Chrome cannot pass this machine's first-launch assessment, nothing quarantined
can, and the launcher's hang is that symptom rather than anything about the
build.

**This has to be redone after a reboot**, on a machine whose `syspolicyd` is
healthy: download from the site in a browser, drag to Applications, open once,
accept the prompt, and watch the window come up and reach the Tiiny. Until then
the first-run path is argued from the pieces rather than seen end to end.

## 5. What tonight cost, and the rule that comes out of it

Four bugs reached main tonight and every one of them lived in a step that had
never executed, because the secret that gates it had never been set:

1. The updater overlay carried a `comment` key, which Tauri's config schema
   refuses. The build died before compiling.
2. The comment explaining that, written at six spaces beside lines at ten,
   ended the YAML block scalar early and made the whole workflow unreadable.
   Every run failed with no jobs and no logs.
3. An apostrophe in "the farm's worker", inside a script handed to `node -e`,
   closed the quoted string. The feed fragment step died on an argument list the
   shell had rearranged.
4. Setting one of the three Azure secrets flipped the Windows gate to signing
   and would have failed the job at login, losing the installer, until all three
   were in.

`scripts/check-workflows.mjs` now asks the two questions that catch the middle
two: does the file parse as YAML, and does `bash -n` accept every `run:` script.
Neither tool alone sees both.

And two rules about testing an installed app, learned the expensive way:

- **Never delete or replace an app bundle at a path while its processes are
  running.** What that leaves behind is not in the bundle, so deleting and
  reinstalling does not clear it, and the app starts a process that never gets a
  window.
- **Never hand-set a quarantine flag on the same path you test from.** Testing
  Gatekeeper is worth doing; do it on a copy at a throwaway path, and never on
  the copy somebody is about to use.

Both of those are what made a good release look broken for an hour, and both
were mine.
