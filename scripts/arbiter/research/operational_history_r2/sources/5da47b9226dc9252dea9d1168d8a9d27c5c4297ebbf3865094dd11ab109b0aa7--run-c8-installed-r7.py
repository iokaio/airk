# SPDX-License-Identifier: Apache-2.0
import json,os,subprocess,time,hashlib
from pathlib import Path
base=Path('artifacts/local/one-airk-1.8.14').resolve();root=Path.cwd();os.environ.update(TEMP=json.loads((base/'user-private-fixture-base.json').read_text())['path'],TMP=json.loads((base/'user-private-fixture-base.json').read_text())['path'],AIRK_EDITOR_SCENARIOS='multi',AIRK_EDITOR_CONTROL_TRACE='0')
args=json.loads((base/'windows-debug-c8-installed-native-editor-r6.json').read_text())['arguments'];candidate=Path(args[-1]);assert hashlib.sha256(candidate.read_bytes()).hexdigest()=='cc72c5dd52c5c2830c4d733fffdcd3aae4a3e9d24d8a22c6f267859d1a9575b6'
start=time.monotonic()
with (base/'windows-debug-c8-installed-native-editor-r7.log').open('x') as f:r=subprocess.run(args,stdout=f,stderr=subprocess.STDOUT)
result={'arguments':args,'exit_code':r.returncode,'elapsed_seconds':time.monotonic()-start,'qualification':'same sealed r23 debug VSIX; test-only guarded-click correction; no replay of earlier unknown outcome','reuse_proof':'windows-debug-c8-installed-r7-reuse.json'}
with (base/'windows-debug-c8-installed-native-editor-r7.json').open('x') as f:json.dump(result,f,indent=2)
print(json.dumps(result));raise SystemExit(r.returncode)
