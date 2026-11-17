# SPDX-License-Identifier: Apache-2.0
import hashlib
import importlib.util
import json
import os
from pathlib import Path
import subprocess
import sys
import time
import uuid

root=Path(__file__).resolve().parents[4]
os.chdir(root)
base=Path(__file__).resolve().parents[1]
attempt=sys.argv[1] if len(sys.argv)==2 else 'r1'
assert attempt.startswith('r') and attempt[1:].isdigit()
runtime=base/'windows-debug-r24/runtime'
spec=importlib.util.spec_from_file_location('builder',root/'scripts/build-service-runtime.py')
builder=importlib.util.module_from_spec(spec);spec.loader.exec_module(builder)
build=json.loads((runtime/'airk-build.json').read_text())
assert builder.source_identity()==build['source'],'Current native production inputs differ from sealed r24'
inventory=json.loads((runtime/'airk-install.json').read_text())
sha=lambda path:hashlib.sha256(path.read_bytes()).hexdigest()
for name,expected in inventory['files'].items():
    path=(runtime/name).resolve(strict=True)
    assert path.is_relative_to(runtime.resolve()) and sha(path)==expected,name
assert {path.relative_to(runtime).as_posix() for path in runtime.rglob('*') if path.is_file()}==set(inventory['files'])|{'airk-install.json'}
private=json.loads((base/'user-private-fixture-base.json').read_text())['path']
os.environ.update(TEMP=private,TMP=private,AIRK_PRIVATE_CENSUS_BASE=private)
proof={'compiled_source':build['source'],'current_native_source':builder.source_identity(),'sealed_files':len(inventory['files']),
       'manifest_sha256':sha(runtime/'airk-install.json'),'runtime_reuse':'fresh r24 sealed artifact; actual client qualification, no new compilation',
       'runner_sha256':sha(Path(__file__).resolve()),'client_fixture_sha256':sha(Path(__file__).with_name('codex-native-acceptance.py'))}
with (base/f'windows-debug-r24-actual-codex-{attempt}-reuse.json').open('x') as output:json.dump(proof,output,indent=2)
args=['python',str(Path(__file__).with_name('codex-native-acceptance.py')),'--runtime-dir',str(runtime),
      '--codex','C:/Users/tdotn/.vscode/extensions/openai.chatgpt-26.1021.22027-win32-x64/bin/windows-x86_64/codex.exe',
      '--output',str(Path(private)/('actual-codex-r24-'+uuid.uuid4().hex)),
      '--client-cwd-base',str(Path(os.environ['LOCALAPPDATA'])/'Temp'),
      '--report',str(base/f'windows-debug-r24-actual-codex-result-{attempt}.json')]
started=time.monotonic()
with (base/f'windows-debug-r24-actual-codex-{attempt}.log').open('x') as log:
    result=subprocess.run(args,stdout=log,stderr=subprocess.STDOUT)
receipt={'arguments':args,'exit_code':result.returncode,'elapsed_seconds':time.monotonic()-started,'reuse_proof':f'windows-debug-r24-actual-codex-{attempt}-reuse.json'}
with (base/f'windows-debug-r24-actual-codex-{attempt}.json').open('x') as output:json.dump(receipt,output,indent=2)
print(json.dumps(receipt),flush=True)
raise SystemExit(result.returncode)
