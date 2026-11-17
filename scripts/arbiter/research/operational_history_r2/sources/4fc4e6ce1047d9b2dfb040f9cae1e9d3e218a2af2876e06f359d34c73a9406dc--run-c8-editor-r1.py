# SPDX-License-Identifier: Apache-2.0
import json, os, subprocess, time
from pathlib import Path
root=Path(__file__).resolve().parents[3];os.chdir(root)
evidence=Path(__file__).resolve().parent
assert json.loads((evidence/'windows-debug-final-extension-native-r3.json').read_text())['exit_code']==0
os.environ['PATH']='C:/Users/tdotn/.cargo/bin;'+os.environ['PATH']
os.environ.update(GIT_CONFIG_COUNT='1',GIT_CONFIG_KEY_0='safe.directory',GIT_CONFIG_VALUE_0=root.as_posix())
private=json.loads((evidence/'user-private-fixture-base.json').read_text())['path']
os.environ.update(TEMP=private,TMP=private,AIRK_PRIVATE_CENSUS_BASE=private,AIRK_EDITOR_SCENARIOS='multi')
def gate(name,args):
    start=time.monotonic()
    with (evidence/(name+'.log')).open('x') as log: run=subprocess.run(args,stdout=log,stderr=subprocess.STDOUT)
    result={'arguments':args,'exit_code':run.returncode,'elapsed_seconds':time.monotonic()-start,'qualification':'r21 debug runtime reused; development stage; actual installed Code; no final VSIX'}
    with (evidence/(name+'.json')).open('x') as out: json.dump(result,out,indent=2)
    print(json.dumps(result),flush=True)
    if run.returncode: raise SystemExit(run.returncode)
native=evidence/'windows-debug-c8-native-r1'
extension=evidence/'windows-debug-c8-stage-r1'
gate('windows-debug-c8-editor-compile-r1',['npm.cmd','--prefix','packages/vscode','run','compile'])
gate('windows-debug-c8-native-package-r1',['python','scripts/package-native.py','--target','x86_64-pc-windows-msvc','--runtime-dir',str(evidence/'windows-debug-r21/runtime'),'--output',str(native)])
gate('windows-debug-c8-stage-r1',['python','scripts/package-extension.py','--target','x86_64-pc-windows-msvc','--native-dir',str(native/'publish'),'--output',str(extension),'--stage-only'])
editor='C:/Users/tdotn/AppData/Local/Programs/Microsoft VS Code/Code.exe'
cli='C:/Users/tdotn/AppData/Local/Programs/Microsoft VS Code/c4453f8ea4/resources/app/out/cli.js'
gate('windows-debug-c8-native-editor-r1',['node','scripts/check-native-editor.cjs',editor,cli,str(extension/'stage')])
