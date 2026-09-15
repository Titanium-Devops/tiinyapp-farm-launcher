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

Every installer goes up twice. The versioned name is what the updater feed
points at and is immutable for a year; the stable name is what the site's
download button points at, because `site/launcher.json` takes one bare filename
per platform and that filename cannot change between releases.

| What | Key | Cached |
| --- | --- | --- |
| Apple silicon, for the feed | `launcher/Tiiny-App-Farm_0.1.0_aarch64.dmg` | a year |
| Apple silicon, for the button | `launcher/Tiiny-App-Farm.dmg` | five minutes |
| Intel, for the feed | `launcher/Tiiny-App-Farm_0.1.0_x86_64.dmg` | a year |
| Windows, for the button | `launcher/Tiiny-App-Farm-Setup.exe` | five minutes |
| The feed | `launcher/latest.json` | never |

The names the builds produce have spaces in them, which the worker's filename
rule refuses, so the script renames on upload and never on disk.

**One thing the site cannot express.** `launcher.json` has one `mac` filename
and there are two disk images. The button gets the Apple silicon one. The Intel
one is uploaded and reachable at `/launcher/Tiiny-App-Farm-Intel.dmg`, and
nothing on the page links to it until the site can carry two names or the build
produces one universal disk image.

It needs `gh` logged in, and `wrangler` logged in to the account that owns the
bucket, which it checks with `wrangler whoami` from the farm's own checkout
before the first upload. It reads no key, token or password from a file or an
environment variable, and it prints none.

Both workflows now write a `SHA256SUMS` beside what they built and upload it
with the artifact, so the publish step has something the building machine
vouched for to compare the downloaded bytes against. Before this there was
nothing: no workflow logged a checksum anywhere.

**Proved by running it.** Against `d4fc257`, the current tip of main, it found
both runs, pulled the artifacts and stopped with:

> That run produced no latest.json, which means it produced no updater bundles
> either. TAURI_SIGNING_PRIVATE_KEY is not set on the repository, so there is
> nothing for the updater to read and nothing signed to publish. Set it and push
> to main again.

Which is correct, and is what the first run tonight will say until the secrets
are set.

## 4. The route exists, and the feed was pointing at a shape it will not serve

I read a 404 on `/launcher/latest.json` and called the route missing. It is not.
The farm's worker has served that prefix since its pull request 29, and the 404
is the route answering:

```
$ curl -s https://tiinyapp.farm/launcher/latest.json
{"error":"The launcher has not been published yet."}
```

A missing route does not answer JSON in words. The status code alone was not
evidence and I should have read the body before saying so.

What is real, and worse, is what reading `worker/main.mjs` turned up.
`launcherType()` accepts `^[A-Za-z0-9][A-Za-z0-9._-]*$` and nothing else, so a
name with a slash in it is refused before the bucket is ever asked. The worker
serves `GET /launcher/<file>` out of the key `launcher/<file>`, one flat name.

`macos.yml` was writing every feed URL as
`https://tiinyapp.farm/launcher/<version>/<name>`, which has a slash in it. Every
one of those would have answered "That launcher file does not exist", and the
updater would have been broken for everybody from the first release, quietly,
because an updater that cannot fetch says nothing to the person using it.

Fixed here: the fragment now writes `https://tiinyapp.farm/launcher/<name>`. The
version is already inside the name, which is also what earns the object its one
year cache in the worker. A name with no version in it gets five minutes, which
is what the two stable download names want.

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
  `wrangler r2 object put` ran for real, against two real runs, and stopped
  where it should. No object has been written to any bucket.
- A signed or notarised build of any kind. None exists yet, on this Mac or in
  CI, because the Apple and Azure secrets are not set.
- Windows, and an Intel Mac.
- What the card says for a stopped app whose usual port differs, because the
  engine does not tell the window that yet.
