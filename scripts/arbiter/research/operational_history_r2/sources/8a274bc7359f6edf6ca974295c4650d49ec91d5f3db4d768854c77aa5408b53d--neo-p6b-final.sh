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
    printf '%s\n' "$name" > "$RUN/p6b-final-phase.txt"
    local code=0
    "$@" > "$RUN/p6b-final-$name.log" 2>&1 || code=$?
    printf '%s %s\n' "$name" "$code" >> "$RUN/p6b-final-results.txt"
    tail -n 12 "$RUN/p6b-final-$name.log"
    if [ "$code" -ne 0 ]; then exit "$code"; fi
}
step source-before python "$RUN/neo-p6b-final-verify.py"
step format cargo fmt --all --check
step cli-clippy cargo clippy -p airk-cli --all-targets --all-features --locked --offline -- -D warnings
step cli-recovery cargo test -p airk-cli --lib --all-features --locked --offline helper_recovery -- --test-threads=2
step cli-pool-process cargo test -p airk-cli --lib --all-features --locked --offline embedding_pool_process -- --test-threads=2
step cli-native-process cargo test -p airk-cli --lib --all-features --locked --offline native_process -- --test-threads=2
step embed-all cargo test -p airk-cli --bin airk-embed --all-features --locked --offline -- --test-threads=2
step grant-recovery cargo test -p airk-cli --test helper_grant_recovery --all-features --locked --offline -- --test-threads=2
step service-process-logs cargo test -p airk-cli --test service_runtime --all-features --locked --offline process_logs_survive_shutdown_and_unknown_log_state_does_not_disable_health -- --test-threads=2
step native-build python scripts/build-service-runtime.py --profile release --destination artifacts/p6b-runtime
step supervisor-native env AIRK_TEST_RUNTIME="$RUN/source-r2/artifacts/p6b-runtime" cargo test -p airk-cli --all-features --lib project_supervisor::pool_tests::native_supervisor_repairs_missing_grant_without_restarting_owners --locked --offline -- --ignored --exact --test-threads=1
step native-install python scripts/check-service-install.py --runtime "$RUN/source-r2/artifacts/p6b-runtime"
step source-after python "$RUN/neo-p6b-final-verify.py"
printf '%s\n' complete > "$RUN/p6b-final-phase.txt"
