# SPDX-License-Identifier: Apache-2.0
import json,os,subprocess,time,hashlib
from pathlib import Path
base=Path('artifacts/local/one-airk-1.8.14').resolve();root=Path.cwd();os.environ.update(TEMP=json.loads((base/'user-private-fixture-base.json').read_text())['path'],TMP=json.loads((base/'user-private-fixture-base.json').read_text())['path'],AIRK_EDITOR_SCENARIOS='multi',AIRK_EDITOR_CONTROL_TRACE='0')
args=json.loads((base/'windows-debug-c8-installed-native-editor-r8.json').read_text())['arguments'];candidate=Path(args[-1]);assert hashlib.sha256(candidate.read_bytes()).hexdigest()=='ab8d8216e46c01c4a533eb26bd8f3602c495a77d1b2558650faed5373a5049b3'
for name,compile_args in [('windows-debug-c8-path-compile-r12',['npm.cmd','--prefix','packages/vscode','run','compile'])]:
 start=time.monotonic()
 with (base/(name+'.log')).open('x') as f:compiled=subprocess.run(compile_args,stdout=f,stderr=subprocess.STDOUT)
 with (base/(name+'.json')).open('x') as f:json.dump({'arguments':compile_args,'exit_code':compiled.returncode,'elapsed_seconds':time.monotonic()-start},f,indent=2)
 assert compiled.returncode==0
test_args=['node','--test','packages/vscode/out/test/installedEditorContracts.test.js','packages/vscode/out/test/installedControlTrace.test.js','scripts/tests/editor-ui.test.cjs']
start=time.monotonic()
with (base/'windows-debug-c8-selector-contracts-r12.log').open('x') as f:checked_tests=subprocess.run(test_args,stdout=f,stderr=subprocess.STDOUT)
with (base/'windows-debug-c8-selector-contracts-r12.json').open('x') as f:json.dump({'arguments':test_args,'exit_code':checked_tests.returncode,'elapsed_seconds':time.monotonic()-start},f,indent=2)
assert checked_tests.returncode==0
stage=base/'windows-debug-c8-vsix-r3/stage';inventory=json.loads((stage/'airk-update.json').read_text());checked={}
for name,expected in inventory['files'].items():
 if name.startswith('out/'):
  actual=hashlib.sha256((root/'packages/vscode'/name).read_bytes()).hexdigest();assert actual==expected,name;checked[name]=actual
with (base/'windows-debug-c8-installed-r12-reuse.json').open('x') as f:json.dump({'vsix_sha256':'ab8d8216e46c01c4a533eb26bd8f3602c495a77d1b2558650faed5373a5049b3','unchanged_production_outputs':checked,'current_test_driver_sha256':hashlib.sha256((root/'packages/vscode/src/test/installedEditor.ts').read_bytes()).hexdigest(),'qualification':'same sealed archive; test driver path normalization only'},f,indent=2)
start=time.monotonic()
with (base/'windows-debug-c8-installed-native-editor-r12.log').open('x') as f:r=subprocess.run(args,stdout=f,stderr=subprocess.STDOUT)
result={'arguments':args,'exit_code':r.returncode,'elapsed_seconds':time.monotonic()-start,'qualification':'same sealed r23 debug VSIX; test-only canonical paths and full native selector activation proof; no replay of earlier unknown outcome','reuse_proof':'windows-debug-c8-installed-r12-reuse.json'}
with (base/'windows-debug-c8-installed-native-editor-r12.json').open('x') as f:json.dump(result,f,indent=2)
print(json.dumps(result));raise SystemExit(r.returncode)
