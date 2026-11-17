# SPDX-License-Identifier: Apache-2.0
#!/bin/bash
set -u
RUN=/Users/tylerjensen/airk-builds/p6-full-20261104.zS15Hn
CACHE=/Users/tylerjensen/airk-builds/p6a-20261103.jBR4Jh
TOOLS=/Users/tylerjensen/airk-builds/timing-1.6.4.PutU1P
PREPARED=/Users/tylerjensen/airk-builds/p6f-focused-final-r2-20261104.Ijzrqk/prepared-tools
export PATH="$PREPARED/bin:$PREPARED/pwsh:$TOOLS/tools:$TOOLS/tools/node-v24.15.0-darwin-arm64/bin:$HOME/.cargo/bin:$PATH"
export CARGO_BUILD_JOBS=1 CARGO_NET_OFFLINE=true MACOSX_DEPLOYMENT_TARGET=13.3 PYTHONUTF8=1
export POWERSHELL_TELEMETRY_OPTOUT=1 DOTNET_CLI_TELEMETRY_OPTOUT=1
export CARGO_TARGET_DIR="$CACHE/target"
export AIRK_BUNDLED_MODEL_DIR="$CACHE/target/local-vector-model" AIRK_BUNDLED_RUNTIME_DIR="$CACHE/target/local-onnx-runtime"
export AIRK_TEST_MODEL_DIR="$AIRK_BUNDLED_MODEL_DIR/minilm-l6-v2-s8-v1" AIRK_PRIVATE_CENSUS_BASE="$RUN/private-census"
export AIRK_TEST_BINARY="$RUN/source/artifacts/neo-full-runtime/AIRK.app/Contents/MacOS/airk"
umask 077
ulimit -Sn 4096 || exit 1
python "$RUN/native-correction.py" > "$RUN/native-correction-controller.log" 2>&1
code=$?
printf '%s\n' "$code" > "$RUN/native-correction-exit-code.txt"
exit "$code"
