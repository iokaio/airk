# SPDX-License-Identifier: Apache-2.0
import hashlib
import importlib.util
import json
import os
from pathlib import Path
import subprocess
import sys
import time

root=Path(__file__).resolve().parents[4];os.chdir(root)
base=Path(__file__).resolve().parents[1]
attempt=sys.argv[1] if len(sys.argv)==2 else 'r1'
assert attempt.startswith('r') and attempt[1:].isdigit()
runtime=base/'windows-debug-r24/runtime'
stage=base/'windows-debug-reconcile-vsix-r1/stage'
vsix=base/'windows-debug-reconcile-vsix-r1/publish/airk-1.8.14-win32-x64.vsix'
sha=lambda path:hashlib.sha256(path.read_bytes()).hexdigest()
spec=importlib.util.spec_from_file_location('builder',root/'scripts/build-service-runtime.py')
builder=importlib.util.module_from_spec(spec);spec.loader.exec_module(builder)
compiled=json.loads((runtime/'airk-build.json').read_text())['source'];assert builder.source_identity()==compiled
inventory=json.loads((runtime/'airk-install.json').read_text())
assert sha(stage/'native/runtime/airk-install.json')==sha(runtime/'airk-install.json')
for name,expected in inventory['files'].items():
    assert sha(runtime/name)==expected,name
    assert sha(stage/'native/runtime'/name)==expected,name
assert {path.relative_to(runtime).as_posix() for path in runtime.rglob('*') if path.is_file()}==set(inventory['files'])|{'airk-install.json'}
update=json.loads((stage/'airk-update.json').read_text());outputs={}
for name,expected in update['files'].items():
    assert sha(stage/name)==expected,name
    if name.startswith('out/'):
        assert sha(root/'packages/vscode'/name)==expected,name
        outputs[name]=expected
private=json.loads((base/'user-private-fixture-base.json').read_text())['path']
os.environ.update(TEMP=private,TMP=private,AIRK_PRIVATE_CENSUS_BASE=private,AIRK_EDITOR_CONTROL_TRACE='0')
helpers={path.name:sha(path) for path in Path(__file__).parent.iterdir() if path.is_file() and path.suffix in ('.cjs','.py','.json')}
proof={'compiled_native_source':compiled,'runtime_manifest_sha256':sha(runtime/'airk-install.json'),
       'native_sealed_files':len(inventory['files']),'vsix_sha256':sha(vsix),'current_extension_outputs':outputs,
       'current_installed_test_sources':{path.relative_to(root).as_posix():sha(path) for path in (root/'packages/vscode/out/test').glob('installed*.js')},
       'ignored_actual_host_helpers':helpers,'scope':'same sealed r24 native and matched debugVSIX; full installed multi plus actual CodeMCP host; no fresh build/model turn/OS trust mutation'}
proof_file=f'windows-debug-r24-actual-code-{attempt}-reuse.json'
with (base/proof_file).open('x') as output:json.dump(proof,output,indent=2)
args=['node',str(Path(__file__).with_name('run-code-mcp-host.cjs')),
      'C:/Users/tdotn/AppData/Local/Programs/Microsoft VS Code/Code.exe',
      'C:/Users/tdotn/AppData/Local/Programs/Microsoft VS Code/c4453f8ea4/resources/app/out/cli.js',str(vsix)]
started=time.monotonic()
with (base/f'windows-debug-r24-actual-code-{attempt}.log').open('x') as log:
    result=subprocess.run(args,stdout=log,stderr=subprocess.STDOUT)
receipt={'arguments':args,'exit_code':result.returncode,'elapsed_seconds':time.monotonic()-started,'reuse_proof':proof_file,
         'selected_scenarios':['multi'],'scope':'actual Code MCP host plus canonical full native multi journey'}
with (base/f'windows-debug-r24-actual-code-{attempt}.json').open('x') as output:json.dump(receipt,output,indent=2)
print(json.dumps(receipt),flush=True)
raise SystemExit(result.returncode)
