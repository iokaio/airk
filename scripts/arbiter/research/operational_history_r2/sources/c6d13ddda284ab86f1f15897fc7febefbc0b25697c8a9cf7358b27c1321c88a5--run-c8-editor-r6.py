# SPDX-License-Identifier: Apache-2.0
import hashlib,json,os,subprocess,time
from pathlib import Path
root=Path(__file__).resolve().parents[3];os.chdir(root)
evidence=Path(__file__).resolve().parent
assert json.loads((evidence/'windows-debug-final-extension-native-r6.json').read_text())['exit_code']==0
private=json.loads((evidence/'user-private-fixture-base.json').read_text())['path']
os.environ.update(TEMP=private,TMP=private,AIRK_PRIVATE_CENSUS_BASE=private,AIRK_EDITOR_SCENARIOS='multi')
def gate(name,args):
    start=time.monotonic()
    with (evidence/(name+'.log')).open('x') as log: run=subprocess.run(args,stdout=log,stderr=subprocess.STDOUT)
    result={'arguments':args,'exit_code':run.returncode,'elapsed_seconds':time.monotonic()-start,'qualification':'r21 debug runtime and r3 stage reused; production output verified unchanged; revised test-only driver'}
    with (evidence/(name+'.json')).open('x') as out: json.dump(result,out,indent=2)
    print(json.dumps(result),flush=True)
    if run.returncode: raise SystemExit(run.returncode)
# Test-only CJS change; retain the existing r4 compiled driver.
gate('windows-debug-c8-editor-contracts-r6',['node','--test','packages/vscode/out/test/installedEditorContracts.test.js','scripts/tests/editor-ui.test.cjs','scripts/tests/native-editor-idle.test.cjs','scripts/tests/native-editor-receipts.test.cjs'])
stage=evidence/'windows-debug-c8-stage-r3/stage'
inventory=json.loads((stage/'airk-update.json').read_text())
checked={}
for name,expected in inventory['files'].items():
    if name.startswith('out/'):
        actual=hashlib.sha256((root/'packages/vscode'/name).read_bytes()).hexdigest()
        assert actual==expected,name
        checked[name]=actual
sources=['packages/vscode/src/test/installedEditor.ts','packages/vscode/src/test/installedEditorContracts.ts','packages/vscode/src/test/installedEditorContracts.test.ts','packages/vscode/src/test/installedEditorC8.ts','scripts/check-native-editor.cjs','scripts/editor-ui.cjs','scripts/native-editor-idle.cjs','scripts/tests/native-editor-idle.test.cjs']
receipt={'reuse':'r6 verified production outputs; original r21 native; r3 stage','unchanged_production_outputs':checked,'test_sources':{name:hashlib.sha256((root/name).read_bytes()).hexdigest() for name in sources}}
with (evidence/'windows-debug-c8-editor-r6-reuse.json').open('x') as out:json.dump(receipt,out,indent=2)
gate('windows-debug-c8-native-editor-r6',['node','scripts/check-native-editor.cjs','C:/Users/tdotn/AppData/Local/Programs/Microsoft VS Code/Code.exe','C:/Users/tdotn/AppData/Local/Programs/Microsoft VS Code/c4453f8ea4/resources/app/out/cli.js',str(stage)])


