# The marks the About window carries

Four brand marks, copied byte for byte from `brand/` in the
[tiinyapp-farm](https://github.com/Titanium-Devops/tiinyapp-farm) repository so
that the launcher and the site always show the same ones. A copy that has been
touched up is a copy that drifts, so refreshing any of these is a plain copy of
the file from there and nothing else.

| File | What it is | Where it is used |
| --- | --- | --- |
| `ti-mark.svg` | The Titanium Computing mark. | The Made by row for Titanium Computing, and the launcher's own window has no other use for it. |
| `tiiny-logo.svg` | The Tiiny wordmark. | The Built for row. |
| `titanium-bot-logo.svg` | The Titanium Bot wordmark. | The Part of row, beside the farm, which is the same pair the farm site's footer carries. |
| `farm-mark-256.png` | The farm's sprout. | The top of the About window and the Part of row for tiinyapp.farm. |

`ti-mark.svg` carries `aria-label="favicon"` and a `<title>favicon</title>`
inside it, because that is what the file says in the farm repository. It is
loaded through an `<img>` with its own `alt`, so nothing a person hears comes
from inside the file. It is left alone rather than tidied so that the two copies
stay identical.

The two GitHub marks in `ui/about.html` are not here: they are the standard
`mark-github` path from [primer/octicons](https://github.com/primer/octicons),
copyright GitHub, Inc., published under the MIT licence, and they are inline in
the page rather than a file because they are one path each. The globe, the
licence and the update icons beside them are drawn in that file too and are not
anybody else's work.
