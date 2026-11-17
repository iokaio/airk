# SPDX-License-Identifier: Apache-2.0
#!/bin/bash
# SPDX-License-Identifier: Apache-2.0
set -eu
RUN=/Users/tylerjensen/airk-builds/status-1.8.3-20261101
PREVIOUS=/Users/tylerjensen/airk-builds/timing-1.6.4.PutU1P
export PATH="$PREVIOUS/tools:$PREVIOUS/tools/node-v24.15.0-darwin-arm64/bin:$HOME/.cargo/bin:$PATH"
export CARGO_BUILD_JOBS=1
export RUST_TEST_THREADS=2
export MACOSX_DEPLOYMENT_TARGET=13.3
export CARGO_TARGET_DIR="$PREVIOUS/source/target"
export AIRK_BUNDLED_MODEL_DIR="$PREVIOUS/source/target/local-vector-model"
export AIRK_BUNDLED_RUNTIME_DIR="$PREVIOUS/source/target/local-onnx-runtime"
cd "$RUN"
python - <<'PY' > reviewed-source-verification.log 2>&1
from pathlib import Path
import hashlib,json,tarfile
base=Path.cwd()
m=json.loads((base/'final-source-manifest.json').read_text())
archive=base/'reviewed-overlay.tar.gz'
assert hashlib.sha256(archive.read_bytes()).hexdigest()==m['overlay_sha256']
with tarfile.open(archive) as tar:
    assert sorted(tar.getnames())==sorted(m['overlay_files'])
    tar.extractall(base/'source',filter='data')
for row in m['files']:
    p=base/'source'/row['path']
    assert p.resolve().is_relative_to(base/'source') and not p.is_symlink()
    assert hashlib.sha256(p.read_bytes()).hexdigest()==row['sha256'],row['path']
print('Verified reviewed source:',len(m['files']),'files; HEAD',m['head'])
PY
cd "$RUN/source"
cargo clippy --workspace --all-targets --all-features --locked -- -D warnings > "$RUN/clippy-reviewed.log" 2>&1
printf 'clippy-reviewed 0\n' > "$RUN/reviewed-results.txt"
cargo fmt --all --check > "$RUN/formatting-reviewed.log" 2>&1
printf 'formatting-reviewed 0\n' >> "$RUN/reviewed-results.txt"
