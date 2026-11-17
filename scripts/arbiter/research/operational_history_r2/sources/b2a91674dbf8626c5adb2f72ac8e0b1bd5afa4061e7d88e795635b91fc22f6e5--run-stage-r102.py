# SPDX-License-Identifier: Apache-2.0
import json,os,subprocess,sys
from pathlib import Path
r=Path.cwd();env=dict(os.environ)
env.update(json.loads((r/'artifacts/advanced-split/ort130-x64-owned-r5/cargo-aggregate-env.json').read_bytes())['environment'])
env.update(CARGO_NET_OFFLINE='true',CARGO_BUILD_JOBS='8',CARGO_INCREMENTAL='1',AIRK_BUNDLED_MODEL_DIR=str(r/'artifacts/advanced-split/bundled-model-v3'))
cargo='C:/Users/tdotn/.cargo/bin/cargo.exe';out=r/'artifacts/advanced-split/receipts/stage-integration-r102'
prefix=[sys.executable,'scripts/dev/receipt_runner.py','--groups','native','--out',str(out),'--target-dir',str(r/'artifacts/v190/p10-target')]
def run(name,argv,no_tests=False):
 p=[*prefix]
 if no_tests:p+=['--allow-no-tests']
 result=subprocess.run([*p,name,*argv],env=env)
 if result.returncode:raise SystemExit(result.returncode)
cmd=json.loads((r/'artifacts/advanced-split/receipts/stage-integration-r97/focused.json').read_bytes())['command'];cmd[5:5]=['-p','airk-retrieval']
cmd[-1]+=' | test(same_runtime_unapproved_model_metadata_requires_upgrade_without_payload_reads) | test(active_manifest_metadata_is_lazy_and_same_runtime_weight_change_cannot_mix_cache_keys) | test(same_runtime_retired_weights_arm_the_shared_semantic_upgrade_fence) | test(missing_retired_shared_metadata_is_pending_but_missing_active_metadata_stays_error) | test(active_manifest_rejects_resealed_wrong_neural_lengths_without_payload_reads) | test(original_s8_selector_refuses_foreign_duplicate_and_unpinned_inputs_without_model_load) | test(model_catalog::policy_cases::) | test(model_catalog::tests::)'
run('focused',cmd)
for crate in ['airk-service','airk-kernel']:
 run(crate+'-image',[cargo,'test','-p',crate,'--all-features','--lib','--no-run','--locked','--timings','--message-format=json'],no_tests=True)
