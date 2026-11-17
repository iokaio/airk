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
    printf '%s\n' "$name" > "$RUN/p6d-phase.txt"
    local code=0
    "$@" > "$RUN/p6d-$name.log" 2>&1 || code=$?
    printf '%s %s\n' "$name" "$code" >> "$RUN/p6d-results.txt"
    tail -n 12 "$RUN/p6d-$name.log"
    if [ "$code" -ne 0 ]; then exit "$code"; fi
}
step source-before python "$RUN/neo-p6d-verify.py"
step format cargo fmt --all --check
step workspace-clippy cargo clippy --workspace --all-targets --all-features --locked --offline -- -D warnings
step store-background cargo test -p airk-store --lib --all-features --locked --offline service::registry::background::tests -- --test-threads=2
step store-watch cargo test -p airk-store --lib --all-features --locked --offline service::registry::watch::tests -- --test-threads=2
step store-jobs cargo test -p airk-store --lib --all-features --locked --offline service::registry::jobs::tests -- --test-threads=2
step kernel-enrichment cargo test -p airk-kernel --test service_enrichment --all-features --locked --offline -- --test-threads=1
step cli-source-yield cargo test -p airk-cli --lib --all-features --locked --offline project_protocol::helper_admission_tests::source_yield_control_requires_an_explicit_compatible_private_frame -- --exact --test-threads=1
step native-build python scripts/build-service-runtime.py --profile release --destination artifacts/p6d-runtime
step native-clients python scripts/check-service-clients.py --runtime-dir "$RUN/source-r2/artifacts/p6d-runtime" --files 16 --model --report "$RUN/p6d-native-clients.json"
step native-install python scripts/check-service-install.py --runtime "$RUN/source-r2/artifacts/p6d-runtime"
step source-after python "$RUN/neo-p6d-verify.py"
printf '%s\n' complete > "$RUN/p6d-phase.txt"
