# SPDX-License-Identifier: Apache-2.0
import json,os,subprocess,sys
from pathlib import Path
r=Path.cwd();env=dict(os.environ)
env.update(json.loads((r/'artifacts/advanced-split/ort130-x64-owned-r5/cargo-aggregate-env.json').read_bytes())['environment'])
env.update(CARGO_NET_OFFLINE='true',CARGO_BUILD_JOBS='8',CARGO_INCREMENTAL='1',AIRK_BUNDLED_MODEL_DIR=str(r/'artifacts/advanced-split/bundled-model-v2'))
cargo='C:/Users/tdotn/.cargo/bin/cargo.exe';out=r/'artifacts/advanced-split/receipts/stage-integration-r95'
prefix=[sys.executable,'scripts/dev/receipt_runner.py','--groups','native','--out',str(out),'--target-dir',str(r/'artifacts/v190/p10-target')]
def run(name,command,extra=None,no_tests=False):
 e=dict(env);e.update(extra or {});p=[*prefix]
 if no_tests:p+=['--allow-no-tests']
 result=subprocess.run([*p,name,*command],env=e)
 if result.returncode:raise SystemExit(result.returncode)
run('clippy',[cargo,'clippy','--workspace','--all-targets','--all-features','--locked','--timings','--','-D','warnings'])
for label in ['17','4']:
 p=json.loads((r/f'artifacts/advanced-split/models/qwen3-challengers-r1/context-native-owned-r1/profile-emitter-command-{label}.json').read_bytes())
 run('context-profile-'+label,[cargo,*p['command'][1:]],p['environment'])
for crate in ['airk-inference','airk-kernel']:
 run(crate+'-image',[cargo,'test','-p',crate,'--all-features','--lib','--no-run','--locked','--timings','--message-format=json'],no_tests=True)
