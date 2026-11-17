# SPDX-License-Identifier: Apache-2.0
import json,os,subprocess,sys
from pathlib import Path
r=Path.cwd(); env=dict(os.environ)
env.update(json.loads((r/'artifacts/advanced-split/ort130-x64-owned-r5/cargo-aggregate-env.json').read_bytes())['environment'])
env.update(CARGO_NET_OFFLINE='true',CARGO_BUILD_JOBS='8',CARGO_INCREMENTAL='1',AIRK_BUNDLED_MODEL_DIR=str(r/'artifacts/advanced-split/bundled-model-v2'))
cargo='C:/Users/tdotn/.cargo/bin/cargo.exe'
out=r/'artifacts/advanced-split/receipts/stage-integration-r90'
base=[sys.executable,'scripts/dev/receipt_runner.py','--groups','native','--out',str(out),'--target-dir',str(r/'artifacts/v190/p10-target')]
def run(name,cmd,extra=None):
 e=dict(env);e.update(extra or {})
 result=subprocess.run([*base,name,*cmd],env=e)
 if result.returncode:raise SystemExit(result.returncode)
run('clippy',[cargo,'clippy','--workspace','--all-targets','--all-features','--locked','--timings','--','-D','warnings'])
run('focused',json.loads((r/'artifacts/advanced-split/receipts/stage-integration-r88/focused.json').read_bytes())['command'])
p=json.loads((r/'artifacts/advanced-split/focus-native-qualification-preparation-r1/profile-emitter-command-r3.json').read_bytes())
run('focus-profile',[cargo,*p['command'][1:]],p['environment'])
run('inference-image',[cargo,'test','-p','airk-inference','--all-features','--lib','--no-run','--locked','--timings','--message-format=json'])
