# SPDX-License-Identifier: Apache-2.0
from pathlib import Path
import hashlib, importlib.util, json, os, subprocess, sys
B=Path(__file__).resolve().parent
W=B/'p10-worktree'
spec=importlib.util.spec_from_file_location('native_builder',W/'scripts/build-service-runtime.py')
builder=importlib.util.module_from_spec(spec);spec.loader.exec_module(builder)
paths=['Cargo.toml','Cargo.lock','rust-toolchain.toml','.cargo','crates','scripts/build-service-runtime.py','scripts/provision-onnx-runtime.py','scripts/provision-local-model.py','scripts/int8-policy.py','scripts/macho-identity.py','scripts/license-notices.py','LICENSE','NOTICE','THIRD_PARTY_NOTICES.md','third_party']
raw=subprocess.check_output(['git','-c',f'safe.directory={W.as_posix()}','ls-files','-z','--cached','--others','--exclude-standard','--',*paths],cwd=W)
files=sorted(set(n for n in raw.split(b'\0') if n))
digest=hashlib.sha256();inputs={}
for n in files:
    p=W/os.fsdecode(n)
    assert not p.is_symlink() and p.resolve().is_relative_to(W)
    h=hashlib.sha256(p.read_bytes()).hexdigest() if p.is_file() else None
    digest.update(len(n).to_bytes(8,'big'));digest.update(n);digest.update(bytes.fromhex(h) if h else b'missing')
    inputs[os.fsdecode(n)]=h
identity={'sha256':digest.hexdigest(),'files':len(files)}
assert identity==builder.source_identity()
out=B/sys.argv[1]
with out.open('x',encoding='utf-8') as f:json.dump({'source_root':str(W),'native_source':identity,'native_inputs':inputs},f,indent=2)
print(json.dumps({'path':str(out),'native_source':identity,'sha256':hashlib.sha256(out.read_bytes()).hexdigest()}))
