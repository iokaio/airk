# SPDX-License-Identifier: Apache-2.0
import json,os,subprocess,time,hashlib
from pathlib import Path
root=Path(__file__).resolve().parents[3];os.chdir(root);base=Path(__file__).resolve().parent
private=json.loads((base/'user-private-fixture-base.json').read_text())['path'];os.environ.update(TEMP=private,TMP=private,AIRK_PRIVATE_CENSUS_BASE=private,AIRK_EDITOR_SCENARIOS='multi',AIRK_EDITOR_CONTROL_TRACE='0',GIT_CONFIG_COUNT='1',GIT_CONFIG_KEY_0='safe.directory',GIT_CONFIG_VALUE_0=root.as_posix())
os.environ['PATH']='C:/Users/tdotn/.cargo/bin'+os.pathsep+os.environ['PATH']
def gate(name,args):
 start=time.monotonic()
 with (base/(name+'.log')).open('x') as f:r=subprocess.run(args,stdout=f,stderr=subprocess.STDOUT)
 result={'arguments':args,'exit_code':r.returncode,'elapsed_seconds':time.monotonic()-start,'qualification':'fresh r23 debug native with shared index access retries; local artifact only; not final optimized candidate'}
 with (base/(name+'.json')).open('x') as f:json.dump(result,f,indent=2)
 print(json.dumps(result),flush=True)
 if r.returncode:raise SystemExit(r.returncode)
gate('windows-debug-c8-native-package-r3',['python','scripts/package-native.py','--target','x86_64-pc-windows-msvc','--runtime-dir',str(base/'windows-debug-r23/runtime'),'--output',str(base/'windows-debug-c8-native-r2')])
gate('windows-debug-c8-vsix-r2',['python','scripts/package-extension.py','--target','x86_64-pc-windows-msvc','--native-dir',str(base/'windows-debug-c8-native-r2/publish'),'--output',str(base/'windows-debug-c8-vsix-r2')])
output=base/'windows-debug-c8-vsix-r2/publish';manifest=json.loads((output/'manifest.json').read_text());candidate=output/manifest['archive'];digest=hashlib.sha256(candidate.read_bytes()).hexdigest();assert digest==manifest['archive_sha256']
editor='C:/Users/tdotn/AppData/Local/Programs/Microsoft VS Code/Code.exe';cli='C:/Users/tdotn/AppData/Local/Programs/Microsoft VS Code/c4453f8ea4/resources/app/out/cli.js'
assert hashlib.sha256(Path(editor).read_bytes()).hexdigest()=='51dfee70fd38d8cc3e204f9a304f6fb6700dfa5147223c59d8c37e8176fb9b64'
assert hashlib.sha256(Path(cli).read_bytes()).hexdigest()=='1fa196655d93959322f3d570b35c3def6d6cbb201cb1d776435a7dd1f5136fdd'
with (base/'windows-debug-c8-installed-inputs-r6.json').open('x') as f:json.dump({'vsix_sha256':digest,'native_build':'r23 fresh debug','extension_sources':{p.relative_to(root).as_posix():hashlib.sha256(p.read_bytes()).hexdigest() for p in (root/'packages/vscode/src').rglob('*') if p.is_file()}},f,indent=2)
gate('windows-debug-c8-installed-native-editor-r6',['node','scripts/check-native-editor.cjs',editor,cli,str(candidate)])
