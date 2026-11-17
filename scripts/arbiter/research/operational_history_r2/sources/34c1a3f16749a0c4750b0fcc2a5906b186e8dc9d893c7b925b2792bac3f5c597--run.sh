# SPDX-License-Identifier: Apache-2.0
#!/bin/bash
set -u
RUN=/Users/tylerjensen/airk-builds/p6f-focused-final-20261104.IsswrU
CACHE=/Users/tylerjensen/airk-builds/p6a-20261103.jBR4Jh
TOOLS=/Users/tylerjensen/airk-builds/timing-1.6.4.PutU1P
export PATH="$TOOLS/tools:$TOOLS/tools/node-v24.15.0-darwin-arm64/bin:$HOME/.cargo/bin:$PATH"
export CARGO_BUILD_JOBS=1 CARGO_NET_OFFLINE=true MACOSX_DEPLOYMENT_TARGET=13.3 PYTHONUTF8=1
export CARGO_TARGET_DIR="$CACHE/target"
export AIRK_BUNDLED_MODEL_DIR="$CACHE/target/local-vector-model" AIRK_BUNDLED_RUNTIME_DIR="$CACHE/target/local-onnx-runtime"
export AIRK_TEST_MODEL_DIR="$AIRK_BUNDLED_MODEL_DIR/minilm-l6-v2-u8-v1"
export AIRK_PRIVATE_CENSUS_BASE="$RUN/private-census"
umask 077
mkdir "$AIRK_PRIVATE_CENSUS_BASE"
cd "$RUN/source" || exit 1
failed=0
step() {
 local name="$1"; shift
 printf '%s\n' "$name" > "$RUN/phase.txt"
 "$@" > "$RUN/$name.log" 2>&1
 local code=$?
 printf '%s %s\n' "$name" "$code" >> "$RUN/results.txt"
 tail -n 8 "$RUN/$name.log"
 if [ "$code" -ne 0 ]; then failed=1; fi
 return "$code"
}
step source-before python "$RUN/verify.py" "$RUN" || exit 1
step format cargo fmt --all --check || exit 1
step clippy cargo clippy -p airk-cli --all-targets --all-features --locked --offline -- -D warnings || exit 1
step native-unit cargo nextest run -p airk-cli --lib --all-features --locked --offline -E 'test(runtime_install::catalog_upgrade) | test(runtime_install::managed::catalog) | test(embedding_pool_process::shared::tests) | test(service_runtime::connection_failure_tests) | test(only_temporary_pre_submission_setup_failures_get_a_retry_receipt)'
step store-unit cargo nextest run -p airk-store --all-features --locked --offline -E 'test(/genesis/) | test(/source_isolation/)'
step native-build python scripts/build-service-runtime.py --profile debug --destination artifacts/focused-native-runtime || exit 1
export AIRK_F03_RUNTIME="$RUN/source/artifacts/focused-native-runtime"
export AIRK_GUARDED_PAIR_MAIN="$AIRK_F03_RUNTIME/AIRK.app/Contents/MacOS/airk"
export AIRK_F03_APPROVED_SHA256="$(python -c 'import hashlib,os,pathlib;print(hashlib.sha256((pathlib.Path(os.environ["AIRK_F03_RUNTIME"])/"airk-install.json").read_bytes()).hexdigest())')"
step helper-maintenance cargo test -p airk-cli --lib --all-features --locked --offline installer_maintenance_recovers_last_service_helper_without_new_service_identity -- --ignored --test-threads=1
step helper-abrupt-owner cargo test -p airk-cli --lib --all-features --locked --offline abrupt_service_exit_recovers_real_guarded_helper_before_predecessor_proof -- --ignored --test-threads=1
step helper-native-completion cargo test -p airk-cli --lib --all-features --locked --offline guarded_real_pair_starts_and_records_actual_native_completion -- --ignored --test-threads=1
step copied-genesis cargo test -p airk-cli --lib --all-features --locked --offline actual_copied_first_install_qualifies_genesis_before_first_service -- --ignored --test-threads=1
step managed-setup cargo test -p airk-cli --lib --all-features --locked --offline actual_managed_first_pin_settles_before_connection_and_external_health -- --ignored --test-threads=1
step counts python "$RUN/counts.py" "$RUN"
step source-after python "$RUN/verify.py" "$RUN"
printf '%s\n' "$failed" > "$RUN/exit-code.txt"
printf '%s\n' complete > "$RUN/phase.txt"
exit "$failed"
