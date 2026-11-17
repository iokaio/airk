# SPDX-License-Identifier: Apache-2.0
import json,os,subprocess,time,hashlib
from pathlib import Path
base=Path(__file__).resolve().parent;root=base.parents[2];os.chdir(root)
assert json.loads((base/'windows-debug-final-extension-native-r14.json').read_text())['exit_code']==0
private=json.loads((base/'user-private-fixture-base.json').read_text())['path'];os.environ.update(TEMP=private,TMP=private,AIRK_PRIVATE_CENSUS_BASE=private,AIRK_EDITOR_SCENARIOS='multi',AIRK_EDITOR_CONTROL_TRACE='0',GIT_CONFIG_COUNT='1',GIT_CONFIG_KEY_0='safe.directory',GIT_CONFIG_VALUE_0=root.as_posix());os.environ['PATH']='C:/Users/tdotn/.cargo/bin'+os.pathsep+os.environ['PATH']
def gate(name,args):
 start=time.monotonic()
 with (base/(name+'.log')).open('x') as f:r=subprocess.run(args,stdout=f,stderr=subprocess.STDOUT)
 result={'arguments':args,'exit_code':r.returncode,'elapsed_seconds':time.monotonic()-start,'qualification':'r23 debug native reused unchanged; fresh tested extension with reload attachment and startup publication corrections; local only'}
 with (base/(name+'.json')).open('x') as f:json.dump(result,f,indent=2)
 print(json.dumps(result),flush=True)
 if r.returncode:raise SystemExit(r.returncode)
gate('windows-debug-c8-vsix-r4',['python','scripts/package-extension.py','--target','x86_64-pc-windows-msvc','--native-dir',str(base/'windows-debug-c8-native-r2/publish'),'--output',str(base/'windows-debug-c8-vsix-r4')])
output=base/'windows-debug-c8-vsix-r4/publish';manifest=json.loads((output/'manifest.json').read_text());candidate=output/manifest['archive'];digest=hashlib.sha256(candidate.read_bytes()).hexdigest();assert digest==manifest['archive_sha256']
inputs={p.relative_to(root).as_posix():hashlib.sha256(p.read_bytes()).hexdigest() for p in (root/'packages/vscode/src').rglob('*') if p.is_file()};assert inputs==json.loads((base/'windows-debug-final-extension-r14-reuse.json').read_text())['extension_test_sources']
with (base/'windows-debug-c8-installed-inputs-r14.json').open('x') as f:json.dump({'vsix_sha256':digest,'native_build':'r23 unchanged debug','extension_sources':inputs,'matches_full_suite':'r14'},f,indent=2)
args=json.loads((base/'windows-debug-c8-installed-native-editor-r6.json').read_text())['arguments'];args[-1]=str(candidate)
gate('windows-debug-c8-installed-native-editor-r14',args)

