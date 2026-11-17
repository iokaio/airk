# SPDX-License-Identifier: Apache-2.0
#!/bin/bash
set -u
RUN=/Users/tylerjensen/airk-builds/p6-full-20261104.zS15Hn
SOURCE="$RUN/source"
CACHE=/Users/tylerjensen/airk-builds/p6a-20261103.jBR4Jh
TOOLS=/Users/tylerjensen/airk-builds/timing-1.6.4.PutU1P
PREPARED=/Users/tylerjensen/airk-builds/p6f-focused-final-r2-20261104.Ijzrqk/prepared-tools
export PATH="$PREPARED/bin:$PREPARED/pwsh:$TOOLS/tools:$TOOLS/tools/node-v24.15.0-darwin-arm64/bin:$HOME/.cargo/bin:$PATH"
export CARGO_BUILD_JOBS=1 CARGO_NET_OFFLINE=true MACOSX_DEPLOYMENT_TARGET=13.3 PYTHONUTF8=1
export POWERSHELL_TELEMETRY_OPTOUT=1 DOTNET_CLI_TELEMETRY_OPTOUT=1
export CARGO_TARGET_DIR="$CACHE/target"
export AIRK_BUNDLED_MODEL_DIR="$CACHE/target/local-vector-model" AIRK_BUNDLED_RUNTIME_DIR="$CACHE/target/local-onnx-runtime"
export AIRK_TEST_MODEL_DIR="$AIRK_BUNDLED_MODEL_DIR/minilm-l6-v2-u8-v1" AIRK_PRIVATE_CENSUS_BASE="$RUN/private-census"
umask 077
mkdir "$AIRK_PRIVATE_CENSUS_BASE"
cd "$SOURCE" || exit 1
failed=0
step(){
 local name="$1";shift
 printf '%s\n' "$name" > "$RUN/phase.txt"
 "$@" > "$RUN/$name.log" 2>&1
 local code=$?
 printf '%s %s\n' "$name" "$code" >> "$RUN/results.txt"
 tail -n 8 "$RUN/$name.log"
 if [ "$code" -ne 0 ];then failed=1;fi
 return "$code"
}
step source-before python "$RUN/verify.py" "$RUN" || exit 1
step toolchain python scripts/release-version.py check --expected 1.8.8
step cargo-inputs env CARGO_NET_OFFLINE=false cargo fetch --locked || exit 1
step advisory-cache python "$RUN/prepare-dependency-cache.py" "$RUN" || exit 1
step dependency-policy python scripts/dependency-audit.py --offline --output "$RUN/dependency-policy.json"
step npm-inputs npm ci --ignore-scripts --no-audit --no-fund --prefix packages/vscode || exit 1
step extension-tests npm test --prefix packages/vscode
step model-inputs python scripts/provision-local-model.py --destination "$AIRK_BUNDLED_MODEL_DIR" || exit 1
step runtime-inputs python scripts/provision-onnx-runtime.py || exit 1
step native-build python scripts/build-service-runtime.py --profile release --destination artifacts/neo-full-runtime || exit 1
RUNTIME="$SOURCE/artifacts/neo-full-runtime"
export AIRK_TEST_RUNTIME="$RUNTIME" AIRK_TEST_BINARY="$RUNTIME/AIRK.app/Contents/MacOS/airk"
step workspace-build cargo build --workspace --locked --offline
step format cargo fmt --all --check
step clippy cargo clippy --workspace --all-targets --all-features --locked --offline -- -D warnings
step workspace-tests cargo nextest run --workspace --all-features --locked --offline --profile ci --ignore-default-filter --test-threads 2
step doctests cargo test --doc --workspace --all-features --locked --offline -- --test-threads=1
step native-smoke node --test packages/vscode/out/test/nativeSmoke.test.js
step native-clients python scripts/check-service-clients.py --runtime-dir "$RUNTIME" --files 16 --model --report "$RUN/native-clients.json"
step native-installer python scripts/check-service-install.py --runtime "$RUNTIME"
step actual-upgrade python "$RUN/actual-upgrade.py"
step vector-components cargo test -p airk-kernel --test vector_performance --features local-inference --locked --offline -- --ignored --test-threads=1
step licenses python scripts/license-notices.py --output "$RUN/notices" --release
step python-tests python -m unittest discover -s scripts/tests -v
step repository pwsh -NoProfile -File scripts/check-repo.ps1
step whitespace git diff --check
step native-package python scripts/package-native.py --target aarch64-apple-darwin --runtime-dir "$RUNTIME" --output artifacts/neo-full-native-package
if [ -f artifacts/neo-full-native-package/publish/manifest.json ];then
 step extension-package python scripts/package-extension.py --target aarch64-apple-darwin --native-dir artifacts/neo-full-native-package/publish --output artifacts/neo-full-extension-package
fi
step source-after python "$RUN/verify.py" "$RUN"
printf '%s\n' "$failed" > "$RUN/exit-code.txt"
printf '%s\n' complete > "$RUN/phase.txt"
exit "$failed"
