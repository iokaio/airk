# SPDX-License-Identifier: Apache-2.0
import json,os,subprocess,sys
from pathlib import Path
r=Path.cwd();env=dict(os.environ)
env.update(json.loads((r/'artifacts/advanced-split/ort130-x64-owned-r5/cargo-aggregate-env.json').read_bytes())['environment'])
env.update(CARGO_NET_OFFLINE='true',CARGO_BUILD_JOBS='8',CARGO_INCREMENTAL='1',AIRK_BUNDLED_MODEL_DIR=str(r/'artifacts/advanced-split/bundled-model-v2'))
cargo='C:/Users/tdotn/.cargo/bin/cargo.exe';out=r/'artifacts/advanced-split/receipts/stage-integration-r94'
prefix=[sys.executable,'scripts/dev/receipt_runner.py','--groups','native','--out',str(out),'--target-dir',str(r/'artifacts/v190/p10-target')]
def run(name,command,extra=None,no_tests=False):
 e=dict(env);e.update(extra or {});p=[*prefix]
 if no_tests:p+=['--allow-no-tests']
 result=subprocess.run([*p,name,*command],env=e)
 if result.returncode:raise SystemExit(result.returncode)
run('clippy',[cargo,'clippy','--workspace','--all-targets','--all-features','--locked','--timings','--','-D','warnings'])
cmd=json.loads((r/'artifacts/advanced-split/receipts/stage-integration-r88/focused.json').read_bytes())['command']
cmd[-1]+=' | test(parent_bag_identity_gate_retains_all64_including44_refusals) | test(parent_bag_private_base_refuses_missing_relative_file_and_alias_before_native_lease)'
run('focused',cmd)
requests=[]
base=r/'artifacts/advanced-split/models/qwen3-challengers-r1/context-fp32-storage-r1'
for label in ['17','4']:requests+=json.loads((base/f'profile-request-{label}-r2.json').read_bytes())
combined=base/'profile-requests-both-root-r2.json';assert not combined.exists();combined.write_text(json.dumps(requests,indent=2)+'\n')
run('context-profiles',[cargo,'nextest','run','--profile','ci','-p','airk-retrieval','--all-features','--locked','--timings','-E','test(arbiter::runtime::qualification::tests::emit_selected_experimental_profiles_from_reviewed_assets)','--run-ignored','only'],{'AIRK_SELECTED_PROFILE_INPUTS':str(combined),'AIRK_SELECTED_PROFILE_OUTPUT':str(base/'selected-context-profiles-r2')})
for crate in ['airk-inference','airk-kernel','airk-service']:
 run(crate+'-image',[cargo,'test','-p',crate,'--all-features','--lib','--no-run','--locked','--timings','--message-format=json'],no_tests=True)
