# SPDX-License-Identifier: Apache-2.0
#!/bin/bash
set -eu
RUN=/Users/tylerjensen/airk-builds/p6a-20261103.jBR4Jh
PREVIOUS=/Users/tylerjensen/airk-builds/timing-1.6.4.PutU1P
export PATH="$PREVIOUS/tools:$PREVIOUS/tools/node-v24.15.0-darwin-arm64/bin:$HOME/.cargo/bin:$PATH"
export CARGO_BUILD_JOBS=1
export CARGO_NET_OFFLINE=true
export MACOSX_DEPLOYMENT_TARGET=13.3
export CARGO_TARGET_DIR="$RUN/target"
export AIRK_BUNDLED_MODEL_DIR="$RUN/target/local-vector-model"
export AIRK_BUNDLED_RUNTIME_DIR="$RUN/target/local-onnx-runtime"
export AIRK_TEST_MODEL_DIR="$AIRK_BUNDLED_MODEL_DIR/minilm-l6-v2-u8-v1"
export PYTHONUTF8=1
cd "$RUN/source-r2"
step() {
    local name="$1"
    shift
    printf '%s\n' "$name" > "$RUN/p6a-r5-phase.txt"
    local code=0
    "$@" > "$RUN/p6a-r5-$name.log" 2>&1 || code=$?
    printf '%s %s\n' "$name" "$code" >> "$RUN/p6a-r5-results.txt"
    tail -n 8 "$RUN/p6a-r5-$name.log"
    if [ "$code" -ne 0 ]; then exit "$code"; fi
}
step format cargo fmt --all --check
step native-build python scripts/build-service-runtime.py --profile release --destination artifacts/p6a-r5-runtime
RUNTIME="$RUN/source-r2/artifacts/p6a-r5-runtime"
step cli-clippy cargo clippy -p airk-cli --all-targets --all-features --locked --offline -- -D warnings
step cli-tests cargo test -p airk-cli --lib --all-features --locked --offline -- --test-threads=2
python - <<'ASSERT_CURRENT_TESTS'
from pathlib import Path
log = Path('/Users/tylerjensen/airk-builds/p6a-20261103.jBR4Jh/p6a-r5-cli-tests.log').read_text()
for name in ['publishers_from_different_installations_share_one_lease_and_recover_owned_output', 'selected_mac_client_refresh_and_exact_resume_preserve_explicit_stop_and_unknown_bytes', 'mac_initial_and_repaired_launchers_use_the_independently_admitted_image']:
    assert any(name in line and line.endswith(' ... ok') for line in log.splitlines()), name
print('Current Mac publication-lock and launcher tests executed successfully')
ASSERT_CURRENT_TESTS
step python python -m unittest discover -s scripts/tests -v
step native-clients python scripts/check-service-clients.py --runtime-dir "$RUNTIME" --files 16 --model --report "$RUN/p6a-r5-clients.json"
step native-install python scripts/check-service-install.py --runtime "$RUNTIME"
printf '%s\n' complete > "$RUN/p6a-r5-phase.txt"
