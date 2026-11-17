# SPDX-License-Identifier: Apache-2.0
#!/bin/bash
set -u
RUN=/Users/tylerjensen/airk-builds/p6-full-20261104.zS15Hn
CACHE=/Users/tylerjensen/airk-builds/p6a-20261103.jBR4Jh
TOOLS=/Users/tylerjensen/airk-builds/timing-1.6.4.PutU1P
export PATH="$TOOLS/tools:$TOOLS/tools/node-v24.15.0-darwin-arm64/bin:$HOME/.cargo/bin:$PATH"
export CARGO_BUILD_JOBS=1 CARGO_NET_OFFLINE=true MACOSX_DEPLOYMENT_TARGET=13.3
export CARGO_TARGET_DIR="$CACHE/target" AIRK_BUNDLED_MODEL_DIR="$CACHE/target/local-vector-model" AIRK_BUNDLED_RUNTIME_DIR="$CACHE/target/local-onnx-runtime"
export AIRK_TEST_MODEL_DIR="$AIRK_BUNDLED_MODEL_DIR/minilm-l6-v2-u8-v1" AIRK_PRIVATE_CENSUS_BASE="$RUN/private-census"
cd "$RUN/source" || exit 1
python "$RUN/verify.py" "$RUN" > "$RUN/fd-capacity-source-before.log" 2>&1 || exit 1
{ ulimit -Sn; ulimit -Hn; } > "$RUN/fd-capacity-inherited-limits.txt"
ulimit -Sn 4096 || exit 1
ulimit -Sn > "$RUN/fd-capacity-child-limit.txt"
cargo test -p airk-cli --test service_runtime --all-features --locked --offline configured_query_capacity_updates_live_worker_and_reads_through_matching_channels -- --exact --test-threads=1 > "$RUN/fd-capacity-test.log" 2>&1
code=$?
python "$RUN/verify.py" "$RUN" > "$RUN/fd-capacity-source-after.log" 2>&1 || code=1
printf '%s\n' "$code" > "$RUN/fd-capacity-exit-code.txt"
exit "$code"
