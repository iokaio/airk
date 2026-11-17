# SPDX-License-Identifier: Apache-2.0
#!/bin/bash
set -eu
RUN=/Users/tylerjensen/airk-builds/p6a-20261103.jBR4Jh
PREVIOUS=/Users/tylerjensen/airk-builds/timing-1.6.4.PutU1P
export PATH="$PREVIOUS/tools:$PREVIOUS/tools/node-v24.15.0-darwin-arm64/bin:$HOME/.cargo/bin:$PATH"
export CARGO_BUILD_JOBS=1 CARGO_NET_OFFLINE=true MACOSX_DEPLOYMENT_TARGET=13.3
export CARGO_TARGET_DIR="$RUN/target"
export AIRK_BUNDLED_MODEL_DIR="$RUN/target/local-vector-model" AIRK_BUNDLED_RUNTIME_DIR="$RUN/target/local-onnx-runtime"
export AIRK_TEST_MODEL_DIR="$AIRK_BUNDLED_MODEL_DIR/minilm-l6-v2-u8-v1" PYTHONUTF8=1
cd "$RUN/source-r2"
python "$RUN/neo-p6a-finalize.py"
printf '%s\n' native-build > "$RUN/p6a-r6-phase.txt"
python scripts/build-service-runtime.py --profile release --destination artifacts/p6a-r6-runtime > "$RUN/p6a-r6-native-build.log" 2>&1
printf '%s\n' native-integration > "$RUN/p6a-r6-phase.txt"
cargo test -p airk-cli --test mcp --test native_read_retry --all-features --locked --offline -- --test-threads=2 > "$RUN/p6a-r6-native-integration.log" 2>&1
python "$RUN/neo-p6a-finalize.py"
tail -n 9 "$RUN/p6a-r6-native-integration.log"
printf '%s\n' complete > "$RUN/p6a-r6-phase.txt"
