# SPDX-License-Identifier: Apache-2.0
#!/bin/bash
set -u
RUN=/Users/tylerjensen/airk-builds/p6f-focused-final-r2-20261104.Ijzrqk
CACHE=/Users/tylerjensen/airk-builds/p6a-20261103.jBR4Jh
TOOLS=/Users/tylerjensen/airk-builds/timing-1.6.4.PutU1P
export PATH="$TOOLS/tools:$TOOLS/tools/node-v24.15.0-darwin-arm64/bin:$HOME/.cargo/bin:$PATH"
export CARGO_BUILD_JOBS=1 CARGO_NET_OFFLINE=true MACOSX_DEPLOYMENT_TARGET=13.3
export CARGO_TARGET_DIR="$CACHE/target" AIRK_BUNDLED_MODEL_DIR="$CACHE/target/local-vector-model" AIRK_BUNDLED_RUNTIME_DIR="$CACHE/target/local-onnx-runtime"
export AIRK_TEST_MODEL_DIR="$AIRK_BUNDLED_MODEL_DIR/minilm-l6-v2-u8-v1" AIRK_PRIVATE_CENSUS_BASE="$RUN/private-census"
export AIRK_GUARDED_PAIR_MAIN="$RUN/source/artifacts/focused-native-runtime/AIRK.app/Contents/MacOS/airk"
cd "$RUN/source" || exit 1
python "$RUN/verify.py" "$RUN" > "$RUN/guardian-extra-source-before.log" 2>&1 || exit 1
cargo test -p airk-cli --lib --all-features --locked --offline actual_guardian_death_contains_helper_and_descendant_while_pool_input_is_open -- --ignored --test-threads=1 > "$RUN/guardian-extra.log" 2>&1
code=$?
python "$RUN/verify.py" "$RUN" > "$RUN/guardian-extra-source-after.log" 2>&1 || code=1
printf '%s\n' "$code" > "$RUN/guardian-extra-exit-code.txt"
exit "$code"
