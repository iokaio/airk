# SPDX-License-Identifier: Apache-2.0
import hashlib,json,os,subprocess,time
from pathlib import Path
root=Path(__file__).resolve().parents[3];os.chdir(root)
evidence=Path(__file__).resolve().parent
assert json.loads((evidence/'windows-debug-final-extension-native-r9.json').read_text())['exit_code']==0
os.environ.update(GIT_CONFIG_COUNT='1',GIT_CONFIG_KEY_0='safe.directory',GIT_CONFIG_VALUE_0=root.as_posix())
private=json.loads((evidence/'user-private-fixture-base.json').read_text())['path']
os.environ.update(TEMP=private,TMP=private,AIRK_PRIVATE_CENSUS_BASE=private,AIRK_EDITOR_SCENARIOS='multi')
failures=[]
def sha(file):return hashlib.sha256(file.read_bytes()).hexdigest()
def gate(name,args,required=True):
    start=time.monotonic()
    with (evidence/(name+'.log')).open('x') as log:run=subprocess.run(args,stdout=log,stderr=subprocess.STDOUT)
    result={'arguments':args,'exit_code':run.returncode,'elapsed_seconds':time.monotonic()-start,'qualification':'local debug VSIX; original r21 native reused; no final optimized candidate or publication'}
    with (evidence/(name+'.json')).open('x') as out:json.dump(result,out,indent=2)
    print(json.dumps(result),flush=True)
    if run.returncode:
        failures.append(name)
        if required:raise SystemExit(run.returncode)
gate('windows-debug-c8-installed-compile-r1',['npm.cmd','--prefix','packages/vscode','run','compile'])
gate('windows-debug-c8-installed-contracts-r1',['node','--test','packages/vscode/out/test/installedEditorContracts.test.js','scripts/tests/editor-ui.test.cjs','scripts/tests/native-editor-idle.test.cjs','scripts/tests/native-editor-receipts.test.cjs'])
output=evidence/'windows-debug-c8-vsix-r1'
gate('windows-debug-c8-vsix-r1',['python','scripts/package-extension.py','--target','x86_64-pc-windows-msvc','--native-dir',str(evidence/'windows-debug-c8-native-r1/publish'),'--output',str(output)])
manifest=json.loads((output/'publish/manifest.json').read_text());candidate=output/'publish'/manifest['archive'];assert sha(candidate)==manifest['archive_sha256']
with (evidence/'windows-debug-c8-installed-inputs-r1.json').open('x') as out:
    json.dump({'qualification':'uncommitted current extension sources; local debug test artifact; native r21 build identity unchanged','vsix_sha256':sha(candidate),'extension_sources':{p.relative_to(root).as_posix():sha(p) for p in (root/'packages/vscode/src').rglob('*') if p.is_file()}},out,indent=2)
editor='C:/Users/tdotn/AppData/Local/Programs/Microsoft VS Code/Code.exe';cli='C:/Users/tdotn/AppData/Local/Programs/Microsoft VS Code/c4453f8ea4/resources/app/out/cli.js'
assert sha(Path(editor))=='51dfee70fd38d8cc3e204f9a304f6fb6700dfa5147223c59d8c37e8176fb9b64'
assert sha(Path(cli))=='1fa196655d93959322f3d570b35c3def6d6cbb201cb1d776435a7dd1f5136fdd'
gate('windows-debug-c8-installed-native-editor-r1',['node','scripts/check-native-editor.cjs',editor,cli,str(candidate)],False)
baseline=root/'artifacts/local/1.8.12-explorer-20261107/vscode-final/publish/airk-1.8.12-win32-x64.vsix';baseline_sha='64609d630765a161fd84d887daa4253ea0ea387c33fcf0805e1e9cb7f1c8d54b'
assert sha(baseline)==baseline_sha
gate('windows-debug-c8-installed-explorer-update-r1',['node','scripts/check-explorer-hot-update.cjs',editor,cli,str(baseline),baseline_sha,str(candidate),sha(candidate)],False)
raise SystemExit(bool(failures))
