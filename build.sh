#!/bin/sh
# SPDX-License-Identifier: Apache-2.0
# Build an installation-free AIRK runtime for this platform under
# builds/<UTC timestamp>-<platform>/. For advanced users experimenting with the
# native engine from a terminal, without VS Code. Nothing is installed; the
# output README.md explains how to start each process headlessly.
# Usage: ./build.sh [release|debug]
set -eu
profile=${1:-release}
case "$profile" in release|debug) ;; *) echo "usage: $0 [release|debug]" >&2; exit 2 ;; esac
cd "$(dirname "$0")"
if ! command -v cargo >/dev/null 2>&1 && [ -x "$HOME/.cargo/bin/cargo" ]; then PATH="$HOME/.cargo/bin:$PATH"; fi
python=$(command -v python3 || command -v python || true)
command -v cargo >/dev/null 2>&1 || { echo "cargo is required on PATH (see CONTRIBUTING.md)" >&2; exit 1; }
[ -n "$python" ] || { echo "Python 3.11 or later is required" >&2; exit 1; }
"$python" -c 'import sys; sys.exit(0 if sys.version_info >= (3, 11) else 1)' || { echo "Python 3.11 or later is required" >&2; exit 1; }
exec "$python" scripts/local-build.py --profile "$profile"
