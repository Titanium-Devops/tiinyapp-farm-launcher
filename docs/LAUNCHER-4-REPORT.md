# LAUNCHER-4: the port an app really comes up on, and a way to ship

Built 2026-09-15 on top of LAUNCHER-3 (`d4fc257`), against farm 0.1.16 from
PyPI.

**Every number below was measured on one machine: a MacBook Pro, Apple M5 Max,
macOS 26.6.2 (25G83), arm64, against Jason's Tiiny at 172.17.7.177.** What was
not measured is in section 5 and is labelled as such.

---

## 1. The engine is farm 0.1.16

`scripts/runtime.pins.json` names `0.1.16`. That release remembers the port each
app last took, in `port.json` beside the app, and tries it before the one the
manifest asks for. The reason is worth repeating because it decides the wording
in the window: a browser keeps logins, local storage and permissions per origin,
and an origin is a port. Moving an app from 8811 to 8813 because something else
had 8811 that morning throws all of that away, and to the person it looks like
the app forgot them.

## 2. What the card says now

The card already said what the manifest asks for. It now also says where the app
really comes up, when those differ:

> It needs Python 3.11 or newer, port 8811, your Tiiny, for chat, 28 NPU units.
> The launcher carries its own Python 3.11.16, so there is nothing to install
> first.
>
> It usually comes up on port 8813 rather than the 8811 it asks for, because
> that is where it ran last.

`docs/shots/28-usual-port.png` is that card, made by starting Daybreak on 8813
with `--port` so the two numbers really disagreed, then opening its card.

When the two agree the line is not drawn at all. The needs sentence has already
said the number, and a second line repeating it is noise.

**One limit, and it is the engine's.** `usualPort` reaches the launcher only on
the rows of `farm status --json`, which lists running apps. `farm list --json`
does not carry it, and there is no per-app command that answers for a stopped
one. So the line appears for an app that is running and not for one that is
stopped, which is the opposite of when somebody would most like to know. Fixing
that is one field on the `list` rows in the engine, not work in this window.

## 3. A way to get a signed build to people

There was no phase 5. Neither workflow has an R2 step, there was no publish
script, and the artifacts stopped at GitHub Actions. `scripts/publish-release.mjs`
is that step, in one command:

```sh
node scripts/publish-release.mjs --bucket <r2-bucket>            # dry run
node scripts/publish-release.mjs --bucket <r2-bucket> --publish
```

It finds the macOS and Windows runs for one commit and refuses unless both
succeeded, downloads what they made, and then refuses again unless:

- `latest.json` names this checkout's version and both architectures,
- every signature in it is byte for byte the `.sig` file beside the bundle it
  names,
- every disk image passes `codesign --verify --strict`, `spctl` against its
  primary signature, and `xcrun stapler validate`.

Then it uploads, disk images and installer and updater bundles first, and
`latest.json` last. That order is the whole point: the feed is what every
installed launcher reads, and a feed naming a download that is not there yet
turns all of them into launchers that cannot update.

What it does not check, and says so rather than implying otherwise: the
Authenticode signature on the Windows installer, which only Windows can verify.
The Windows workflow will not upload an installer whose signature Windows itself
called anything but Valid, so that check has already happened on a machine that
could do it.

It needs `gh` logged in, and `wrangler` logged in to the account that owns the
bucket. It reads no key, token or password from a file or an environment
variable.

**Proved by running it.** Against `d4fc257`, the current tip of main, it found
both runs, pulled the artifacts and stopped with:

> That run produced no latest.json, which means it produced no updater bundles
> either. TAURI_SIGNING_PRIVATE_KEY is not set on the repository, so there is
> nothing for the updater to read and nothing signed to publish. Set it and push
> to main again.

Which is correct, and is what the first run tonight will say until the secrets
are set.

## 4. The thing that still blocks a download, and it is not here

`macos.yml` writes every feed fragment with the download URL hardcoded to
`https://tiinyapp.farm/launcher/<version>/<name>`.

| URL | Today |
| --- | --- |
| `https://tiinyapp.farm/` | 200 |
| `https://tiinyapp.farm/seeds-files/<a real key>` | 200 |
| `https://tiinyapp.farm/launcher/` | 404 |
| `https://tiinyapp.farm/launcher/latest.json` | 404 |

So the worker serves R2 under `/seeds-files/`, and nothing serves `/launcher/`.
Uploading to the bucket is necessary and not sufficient: somebody with the farm
repository has to route `/launcher/*` to R2, or the URL in `macos.yml` has to
change to a prefix that already works. That is a change in another repository
and it is not made here.

## 5. Checks, and what was not measured

| Check | Result |
| --- | --- |
| `cargo test` | 84 passed, 0 failed |
| `cargo clippy --all-targets -- -D warnings` | clean |
| `cargo fmt --check` | clean |
| `npm run tauri build` | app and dmg built, `farm 0.1.16` inside |
| `scripts/verify-launcher.mjs` against the real Tiiny | 7 of 10 |

The gate is 7 of 10 for the reason `docs/LAUNCHER-3-REPORT.md` section 8
records, unchanged: macOS grants local network access to an application rather
than to a file, and a start has asked the Tiiny what it has loaded since 0.1.14,
so a shell cannot finish the start, status and stop checks. The gate says so in
the engine's own words and still exits non-zero.

Not measured:

- The upload half of `publish-release.mjs`. Everything up to the first
  `wrangler r2 object put` ran for real; no object has been written to any
  bucket, and no bucket name has been used.
- A signed or notarised build of any kind. None exists yet, on this Mac or in
  CI, because the Apple and Azure secrets are not set.
- Windows, and an Intel Mac.
- What the card says for a stopped app whose usual port differs, because the
  engine does not tell the window that yet.
