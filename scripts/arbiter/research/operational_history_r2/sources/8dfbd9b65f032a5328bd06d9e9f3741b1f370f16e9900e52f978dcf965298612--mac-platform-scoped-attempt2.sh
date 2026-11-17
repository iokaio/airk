# SPDX-License-Identifier: Apache-2.0
#!/bin/sh
# Root Cargo owner only. Do not execute until source owners are frozen and sync is verified.
set -eu
if [ "${AIRK_CARGO_OWNER_BATON:-}" != "mac-platform-ec3c3238ca7a52ae" ]; then echo "Root Cargo-owner baton required" >&2; exit 29; fi
cd "$HOME/code/airk"
platform_python="$PWD/artifacts/tools/neo-python-3.13.18/bin/python3.13"
platform_target="$PWD/artifacts/v190/p10-target"
platform_receipts="$PWD/artifacts/advanced-split/receipts/mac-platform-scoped"
PATH="$PWD/artifacts/tools/neo-nextest-0.9.148:$HOME/.cargo/bin:$PATH"
export PATH
export CARGO_TARGET_DIR="$platform_target" CARGO_INCREMENTAL=0 PYTHONDONTWRITEBYTECODE=1
unset AIRK_BUNDLED_MODEL_DIR AIRK_VECTOR_MODEL AIRK_VECTOR_MODEL_MANIFEST AIRK_VECTOR_TOKENIZER
mkdir -p "$platform_receipts"
source_check() {
 "$platform_python" -I -B - <<'PY'
import hashlib,json
from pathlib import Path
source=Path('artifacts/advanced-split/receipts/mac-platform-format-r1-source.json')
assert hashlib.sha256(source.read_bytes()).hexdigest()=='ae11cb3581a8018cdde8404f0f447373208c7a3326c46abd391460795f1e608f'
manifest=json.loads(source.read_bytes())
for name,row in manifest['files'].items():
 path=Path(name)
 assert path.is_file() and not path.is_symlink() and hashlib.sha256(path.read_bytes()).hexdigest()==row['sha256'],name
print('Exact formatted source seal verified',manifest['source_set_sha256'],len(manifest['files']))
PY
}
source_check
# Formatting and selected Clippy run before any native tests. Receipt runner seals
# native/fixture source; retain JUnit/timings outside target before root cleans.
"$platform_python" scripts/dev/receipt_runner.py --groups native,fixtures --tier T1 --out "$platform_receipts" --target-dir "$platform_target" mac-platform-clippy-attempt2 "$platform_python" scripts/dev-check.py -p airk-core -p airk-process -p airk-ipc -p airk-host --target-dir "$platform_target" --incremental off
"$platform_python" scripts/dev/receipt_runner.py --groups native,fixtures --tier T1 --out "$platform_receipts" --target-dir "$platform_target" mac-platform-tests-attempt2 cargo nextest run --profile ci -p airk-core -p airk-process -p airk-ipc -p airk-host --no-default-features --locked --timings --target-dir "$platform_target" --test-threads 2
source_check
# On failure: stop, collect original diagnostics/source+target identity first.
# On success or failure, root must confirm compiler descendants have exited,
# retain cargo-timings/JUnit/receipts outside target, then issue:
# cargo clean --target-dir "$platform_target"
# Cleanup is deliberately a separate reviewed command; never delete source,
# static-ORT producer trees, isolated Python, private data or retained receipts.
