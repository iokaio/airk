#!/bin/sh
# SPDX-License-Identifier: Apache-2.0
# Install a locally built AIRK VSIX, replacing an installed copy with the same version.
# VS Code skips a VSIX whose version is already installed; --force replaces it. The
# installed code is then verified against the VSIX. A forced reinstall is an update, not
# an uninstall, so AIRK's uninstall cleanup does not run. Reload VS Code windows afterwards.
# Usage: scripts/install-local-vsix.sh [path/to.vsix] [code|code-insiders]
set -eu
cd "$(dirname "$0")/.."
vsix=${1:-$(find artifacts -path '*/extension/publish/*.vsix' -type f -exec ls -t {} + 2>/dev/null | head -n 1)}
code=${2:-code}
[ -n "$vsix" ] && [ -f "$vsix" ] || { echo "No VSIX found; build one first or pass its path" >&2; exit 1; }
command -v "$code" >/dev/null 2>&1 || { echo "$code not found on PATH (VS Code: Shell Command: Install 'code' command in PATH)" >&2; exit 1; }
python=$(command -v python3 || command -v python)
id_version=$("$python" -c 'import json,sys,zipfile; p=json.loads(zipfile.ZipFile(sys.argv[1]).read("extension/package.json")); print(p["publisher"]+"."+p["name"]+"-"+p["version"])' "$vsix")
case "$code" in *insiders*) extensions="$HOME/.vscode-insiders/extensions" ;; *) extensions="$HOME/.vscode/extensions" ;; esac
echo "Installing $id_version from $vsix (forced; replaces the same version)"
"$code" --install-extension "$vsix" --force
"$python" -c 'import hashlib,sys,zipfile
expected=hashlib.sha256(zipfile.ZipFile(sys.argv[1]).read("extension/out/serviceExtension.js")).hexdigest()
actual=hashlib.sha256(open(sys.argv[2],"rb").read()).hexdigest()
sys.exit(0 if expected==actual else "The installed code does not match the VSIX; close all VS Code windows and retry.")' "$vsix" "$extensions/$id_version/out/serviceExtension.js"
echo "Verified: $extensions/$id_version matches the VSIX. Reload each open VS Code window to run the new code."
