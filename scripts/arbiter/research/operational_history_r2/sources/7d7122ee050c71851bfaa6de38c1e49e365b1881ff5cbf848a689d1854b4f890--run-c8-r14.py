# SPDX-License-Identifier: Apache-2.0
import hashlib, importlib.util, json, os, subprocess, time
from pathlib import Path
root=Path(__file__).resolve().parents[3]
os.chdir(root)
evidence=Path(__file__).resolve().parent
runtime=evidence/'windows-debug-r23/runtime'
def sha(path): return hashlib.sha256(path.read_bytes()).hexdigest()
spec=importlib.util.spec_from_file_location('builder',root/'scripts/build-service-runtime.py')
builder=importlib.util.module_from_spec(spec);spec.loader.exec_module(builder)
build=json.loads((runtime/'airk-build.json').read_text())
current=builder.source_identity()
assert current==build['source'], (current,build['source'])
manifest=json.loads((runtime/'airk-install.json').read_text())
for name,expected in manifest['files'].items():
    path=(runtime/name).resolve(strict=True)
    assert path.is_relative_to(runtime.resolve()) and sha(path)==expected,name
actual={p.relative_to(runtime).as_posix() for p in runtime.rglob('*') if p.is_file()}
assert actual==set(manifest['files'])|{'airk-install.json'},actual-set(manifest['files'])
receipt={'reuse':'original r23 debug native reused; attachment/startup correction in extension only','compiled_source':build['source'],'current_production_source':current,'manifest_sha256':sha(runtime/'airk-install.json'),'sealed_files':len(manifest['files']),'extension_test_sources':{p.relative_to(root).as_posix():sha(p) for p in (root/'packages/vscode/src').rglob('*') if p.is_file()}}
path=evidence/'windows-debug-final-extension-r14-reuse.json'
with path.open('x') as out: json.dump(receipt,out,indent=2)
private=json.loads((evidence/'user-private-fixture-base.json').read_text())['path']
os.environ.update(TEMP=private,TMP=private,AIRK_PRIVATE_CENSUS_BASE=private,AIRK_TEST_BINARY=str(runtime/'airk.exe'),AIRK_NATIVE_TEST_BINARY=str(runtime/'airk.exe'),AIRK_TEST_F09_QUALIFICATION='1',AIRK_TEST_MODEL_DIR=str(root/'target/local-vector-model/minilm-l6-v2-u8-v1'))
def gate(name,args):
    start=time.monotonic()
    with (evidence/(name+'.log')).open('x') as log:
        run=subprocess.run(args,stdout=log,stderr=subprocess.STDOUT)
    result={'arguments':args,'exit_code':run.returncode,'elapsed_seconds':time.monotonic()-start,'runtime_reuse':'windows-debug-final-extension-r14-reuse.json'}
    with (evidence/(name+'.json')).open('x') as out: json.dump(result,out,indent=2)
    print(json.dumps(result),flush=True)
    if run.returncode: raise SystemExit(run.returncode)
tests=sorted(str(p) for p in (root/'packages/vscode/out/test').glob('*.test.js'))
assert tests
gate('windows-debug-final-extension-native-r14',['node','--test','--test-concurrency=2',*tests])





