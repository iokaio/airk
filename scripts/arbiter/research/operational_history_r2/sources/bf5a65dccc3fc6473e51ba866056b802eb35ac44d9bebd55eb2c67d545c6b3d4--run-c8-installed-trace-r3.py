# SPDX-License-Identifier: Apache-2.0
import hashlib,json,os,subprocess,time
from pathlib import Path
root=Path(__file__).resolve().parents[3];os.chdir(root);evidence=Path(__file__).resolve().parent
private=json.loads((evidence/'user-private-fixture-base.json').read_text())['path']
os.environ.update(TEMP=private,TMP=private,AIRK_PRIVATE_CENSUS_BASE=private,AIRK_EDITOR_SCENARIOS='multi',AIRK_EDITOR_CONTROL_TRACE='1')
def gate(name,args):
    start=time.monotonic()
    with (evidence/(name+'.log')).open('x') as log:result=subprocess.run(args,stdout=log,stderr=subprocess.STDOUT)
    receipt={'arguments':args,'exit_code':result.returncode,'elapsed_seconds':time.monotonic()-start,'qualification':'same sealed debug VSIX; test-only content-free control trace; no native rebuild or generic-error replay'}
    with (evidence/(name+'.json')).open('x') as out:json.dump(receipt,out,indent=2)
    print(json.dumps(receipt),flush=True)
    if result.returncode:raise SystemExit(result.returncode)
gate('windows-debug-c8-control-trace-compile-r3',['npm.cmd','--prefix','packages/vscode','run','compile'])
gate('windows-debug-c8-control-trace-contracts-r3',['node','--test','packages/vscode/out/test/installedControlTrace.test.js','packages/vscode/out/test/installedEditorContracts.test.js','scripts/tests/editor-ui.test.cjs','scripts/tests/native-editor-idle.test.cjs','scripts/tests/native-editor-receipts.test.cjs'])
stage=evidence/'windows-debug-c8-vsix-r1/stage';inventory=json.loads((stage/'airk-update.json').read_text());checked={}
for name,expected in inventory['files'].items():
    if name.startswith('out/'):
        actual=hashlib.sha256((root/'packages/vscode'/name).read_bytes()).hexdigest();assert actual==expected,name;checked[name]=actual
args=json.loads((evidence/'windows-debug-c8-installed-native-editor-r1.json').read_text())['arguments']
candidate=Path(args[-1]);assert hashlib.sha256(candidate.read_bytes()).hexdigest()=='b6a441de0d3471494f9272e45f40b524e983cd749d18f219c25223c72b823ef0'
with (evidence/'windows-debug-c8-control-trace-r3-reuse.json').open('x') as out:json.dump({'same_vsix_sha256':'b6a441de0d3471494f9272e45f40b524e983cd749d18f219c25223c72b823ef0','unchanged_production_outputs':checked,'test_sources':{p.relative_to(root).as_posix():hashlib.sha256(p.read_bytes()).hexdigest() for p in (root/'packages/vscode/src/test').glob('installed*.ts')}},out,indent=2)
gate('windows-debug-c8-installed-native-editor-r3',args)

