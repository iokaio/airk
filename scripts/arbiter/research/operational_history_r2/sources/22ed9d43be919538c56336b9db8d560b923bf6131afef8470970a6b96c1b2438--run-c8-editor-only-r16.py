# SPDX-License-Identifier: Apache-2.0
import json,os,subprocess,time,hashlib
from pathlib import Path
base=Path('artifacts/local/one-airk-1.8.14').resolve();root=Path.cwd();os.environ.update(TEMP=json.loads((base/'user-private-fixture-base.json').read_text())['path'],TMP=json.loads((base/'user-private-fixture-base.json').read_text())['path'],AIRK_EDITOR_SCENARIOS='multi',AIRK_EDITOR_CONTROL_TRACE='0')
args=json.loads((base/'windows-debug-c8-installed-native-editor-r14.json').read_text())['arguments'];candidate=Path(args[-1]);assert hashlib.sha256(candidate.read_bytes()).hexdigest()=='1f0aed1eabf7ec69a0be2ee8d8f3c01d560c3b70ce7f23f3a09348fd0f00efb1'
assert json.loads((base/'windows-debug-c8-path-compile-r16.json').read_text())['exit_code']==0
assert json.loads((base/'windows-debug-c8-selector-contracts-r16.json').read_text())['exit_code']==0
reuse=json.loads((base/'windows-debug-c8-installed-r16-reuse.json').read_text())
for name,digest in reuse['unchanged_production_outputs'].items():assert hashlib.sha256((root/'packages/vscode'/name).read_bytes()).hexdigest()==digest,name
for name,digest in reuse['current_test_sources'].items():assert hashlib.sha256((root/name).read_bytes()).hexdigest()==digest,name
start=time.monotonic()
with (base/'windows-debug-c8-installed-native-editor-r16.log').open('x') as f:r=subprocess.run(args,stdout=f,stderr=subprocess.STDOUT)
result={'arguments':args,'exit_code':r.returncode,'elapsed_seconds':time.monotonic()-start,'qualification':'same sealed r23 debug VSIX; consolidated audited harness: canonical paths/selectors, bounded restore, complete transport cohorts and forward catalog clocks; no replay of earlier unknown outcome','reuse_proof':'windows-debug-c8-installed-r16-reuse.json'}
with (base/'windows-debug-c8-installed-native-editor-r16.json').open('x') as f:json.dump(result,f,indent=2)
print(json.dumps(result));raise SystemExit(r.returncode)



