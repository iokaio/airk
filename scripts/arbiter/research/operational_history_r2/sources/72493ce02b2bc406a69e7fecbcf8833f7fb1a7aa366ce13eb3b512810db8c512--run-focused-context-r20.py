# SPDX-License-Identifier: Apache-2.0
import json,os,subprocess,sys
from pathlib import Path
root=Path.cwd();env={k:v for k,v in os.environ.items() if not k.upper().startswith('GIT_')}
env.update(json.loads((root/'artifacts/advanced-split/ort130-x64/cargo-aggregate-env.json').read_text())['environment'])
env.update(CARGO_BUILD_JOBS='8',CARGO_INCREMENTAL='1',CARGO_NET_OFFLINE='true',AIRK_BUNDLED_MODEL_DIR=str(root/'artifacts/advanced-split/bundled-model-v2'))
prefix=[sys.executable,'scripts/dev/receipt_runner.py','--groups','native,fixtures','--out','artifacts/advanced-split/receipts/native-context-r20','--target-dir','artifacts/v190/p10-target']
cargo='C:/Users/tdotn/.cargo/bin/cargo.exe'
commands=[['kernel-harness-clippy',cargo,'clippy','-p','airk-kernel','--all-targets','--all-features','--locked','--timings','--','-D','warnings']]
selector='package(airk-mcp) | package(airk-ipc) | (package(airk-inference) & (test(actual_score_call_error) | test(post_score_delivery_refusal) | test(canceled_or_expired_before_actual_score))) | (package(airk-https) & (test(native_policy_fences) | test(policy_lock_covers) | test(native_source_delivery) | test(native_original_delivery_custody) | test(original_deadline) | test(catalog_close_stops_actual) | test(tls_read_driven_alert))) | (package(airk-kernel) & (test(native_context_action_labels_refuse) | test(fresh_final_population_preserves205)))'
commands += [['native-consumer-regressions',cargo,'nextest','run','--profile','ci','-p','airk-mcp','-p','airk-ipc','-p','airk-inference','-p','airk-https','-p','airk-kernel','--lib','--all-features','--locked','--timings','-E',selector]]
for command in commands:
    status=subprocess.run(prefix+command,env=env).returncode
    if status:raise SystemExit(status)
