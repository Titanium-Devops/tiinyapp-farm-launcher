#!/usr/bin/env bash
# Sign every Mach-O file inside the staged Python, before the app around it is
# signed.
#
# Apple signs a bundle inside out, and the Tauri bundler only does that for
# frameworks and sidecar binaries: a Python tree copied in as a resource
# arrives at the notary unsigned and is refused. So the interpreter, the
# library and the two extension modules are signed here, in the staging
# directory, before tauri build copies them.
#
#   scripts/sign-runtime.sh                      ad hoc, for a local build
#   scripts/sign-runtime.sh "Developer ID Application: ..."
#
# A Mach-O signature lives inside the file, so signing the staging copy is
# signing what ships.

set -euo pipefail

identity="${1:--}"
here="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
runtime="$here/../src-tauri/runtime"

if [ ! -d "$runtime" ]; then
  echo "No staged runtime at src-tauri/runtime. Run: npm run stage" >&2
  exit 1
fi

entitlements="$here/../src-tauri/runtime.entitlements.plist"
signed=0

# An ad hoc signature cannot carry a trusted timestamp, and asking for one
# fails the whole run. A real identity always gets one, because a signature
# without a timestamp stops verifying the day the certificate expires.
flags=(--force --options runtime --entitlements "$entitlements")
if [ "$identity" != "-" ]; then flags+=(--timestamp); fi

while IFS= read -r file; do
  case "$(file -b "$file")" in
    *Mach-O*)
      codesign "${flags[@]}" --sign "$identity" "$file"
      echo "signed ${file#"$runtime"/}"
      signed=$((signed + 1))
      ;;
  esac
done < <(find "$runtime" -type f ! -type l)

if [ "$signed" -eq 0 ]; then
  echo "Nothing in src-tauri/runtime is a Mach-O file, which cannot be right." >&2
  exit 1
fi

echo "$signed Mach-O files signed with ${identity}"
