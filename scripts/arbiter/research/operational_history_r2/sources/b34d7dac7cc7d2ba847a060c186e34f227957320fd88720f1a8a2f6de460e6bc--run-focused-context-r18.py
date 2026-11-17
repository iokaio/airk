# SPDX-License-Identifier: Apache-2.0
import json,os,subprocess,sys
from pathlib import Path
root=Path.cwd();env={k:v for k,v in os.environ.items() if not k.upper().startswith('GIT_')}
env.update(json.loads((root/'artifacts/advanced-split/ort130-x64/cargo-aggregate-env.json').read_text())['environment'])
env.update(CARGO_BUILD_JOBS='8',CARGO_INCREMENTAL='1',CARGO_NET_OFFLINE='true',AIRK_BUNDLED_MODEL_DIR=str(root/'artifacts/advanced-split/bundled-model-v2'))
prefix=[sys.executable,'scripts/dev/receipt_runner.py','--groups','native,harness,fixtures,frontend','--out','artifacts/advanced-split/receipts/native-context-r18','--target-dir','artifacts/v190/p10-target']
rows=[('core-all','airk-core',None),('kernel-joint-combined','airk-kernel','test(arbiter::joint::tests::) | test(combined_reads::tests::context::)'),('service-joint-custody','airk-service','test(joint_ledger::tests::) | test(joint_scoring::tests::) | test(policy_delivery::tests::) | test(source_delivery::tests::) | test(singleton_admission) | test(singleton_answer_narrows) | test(joint_and_legacy_tasks_never)'),('host-original-carrier','airk-host','test(read_attempt::tests::) | test(combined_context_wire)'),('frontend-joint-budget','airk-frontend','test(malformed_context_success) | test(escaped_plan_transport_preserves) | test(read_lifetime::tests::)'),('https-actual-write','airk-https','test(native_policy_fences) | test(policy_lock_covers) | test(native_source_delivery) | test(native_original_delivery_custody) | test(original_deadline) | test(catalog_close_stops_actual) | test(tls_read_driven_alert)'),('inference-known-work','airk-inference','test(actual_score_call_error) | test(post_score_delivery_refusal) | test(canceled_or_expired_before_actual_score)'),('mcp-all','airk-mcp',None)]
for name,crate,selector in rows:
    command=['C:/Users/tdotn/.cargo/bin/cargo.exe','nextest','run','--profile','ci','-p',crate,'--lib','--all-features','--locked','--timings']
    if selector:command+=['-E',selector]
    status=subprocess.run(prefix+[name]+command,env=env).returncode
    if status:raise SystemExit(status)
