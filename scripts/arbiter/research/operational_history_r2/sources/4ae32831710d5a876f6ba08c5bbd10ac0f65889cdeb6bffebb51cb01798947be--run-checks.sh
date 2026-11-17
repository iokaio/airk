# SPDX-License-Identifier: Apache-2.0
#!/bin/bash
# SPDX-License-Identifier: Apache-2.0
set -u
RUN=/Users/tylerjensen/airk-builds/status-1.8.3-20261101
PREVIOUS=/Users/tylerjensen/airk-builds/timing-1.6.4.PutU1P
export PATH="$PREVIOUS/tools:$PREVIOUS/tools/node-v24.15.0-darwin-arm64/bin:$HOME/.cargo/bin:$PATH"
export CARGO_BUILD_JOBS=1
export RUST_TEST_THREADS=2
export MACOSX_DEPLOYMENT_TARGET=13.3
export CARGO_TARGET_DIR="$PREVIOUS/source/target"
export AIRK_BUNDLED_MODEL_DIR="$PREVIOUS/source/target/local-vector-model"
export AIRK_BUNDLED_RUNTIME_DIR="$PREVIOUS/source/target/local-onnx-runtime"
cd "$RUN/source" || exit 1
trap 'printf "complete\n" > "$RUN/phase.txt"' EXIT
check() {
    local name="$1"
    shift
    printf '%s\n' "$name" > "$RUN/phase.txt"
    "$@" > "$RUN/$name.log" 2>&1
    local result=$?
    printf '%s %s\n' "$name" "$result" >> "$RUN/results.txt"
    return "$result"
}
python - <<'PY' > "$RUN/source-verification.log" 2>&1
from pathlib import Path
import hashlib,json
root=Path.cwd()
manifest=json.loads((root.parent/'source-manifest.json').read_text())
archive=root.parent/'source.tar.gz'
assert hashlib.sha256(archive.read_bytes()).hexdigest()==manifest['source_tar_sha256']
for row in manifest['files']:
    p=root/row['path']
    assert not p.is_symlink() and p.resolve().is_relative_to(root)
    assert hashlib.sha256(p.read_bytes()).hexdigest()==row['sha256'],row['path']
print('Verified source snapshot:',len(manifest['files']),'files; HEAD',manifest['head'])
PY
if [ "$?" -ne 0 ]; then printf 'source-verification 1\n' >> "$RUN/results.txt"; exit 1; fi
check formatting cargo fmt --all --check
check clippy cargo clippy --workspace --all-targets --all-features --locked -- -D warnings
check status-tests cargo test -p airk-cli --lib status --locked
check workspace cargo test --workspace --all-features --locked --no-fail-fast
check python-contracts python -m unittest discover -s scripts/tests -v
check npm-install npm ci --ignore-scripts --no-audit --no-fund --prefix packages/vscode
check extension npm test --prefix packages/vscode
if check native-build python scripts/build-service-runtime.py --profile release --destination artifacts/runtime/aarch64-apple-darwin; then
    RUNTIME="$RUN/source/artifacts/runtime/aarch64-apple-darwin"
    export AIRK_TEST_BINARY="$RUNTIME/AIRK.app/Contents/MacOS/airk"
    check model python scripts/check-transformer-model.py --binary "$AIRK_TEST_BINARY" --report "$RUN/vector-check.json"
    check native-clients python scripts/check-service-clients.py --runtime-dir "$RUNTIME" --files 16 --model --report "$RUN/native-clients.json"
    check native-install python scripts/check-service-install.py --runtime "$RUNTIME"
    if check native-package python scripts/package-native.py --target aarch64-apple-darwin --runtime-dir "$RUNTIME" --output artifacts/native-status-followup; then
        check vsix-package python scripts/package-extension.py --target aarch64-apple-darwin --native-dir artifacts/native-status-followup/publish --output artifacts/vscode-status-followup
    fi
fi
printf 'complete\n' > "$RUN/phase.txt"
