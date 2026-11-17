# SPDX-License-Identifier: Apache-2.0
import hashlib,importlib.util,json,os,subprocess
from pathlib import Path
root=Path(__file__).resolve().parents[2]; out=Path(__file__).resolve().parent
spec=importlib.util.spec_from_file_location('builder',root/'scripts/build-service-runtime.py')
builder=importlib.util.module_from_spec(spec);spec.loader.exec_module(builder)
scope=['Cargo.toml','Cargo.lock','rust-toolchain.toml','.cargo','crates','scripts/build-service-runtime.py','scripts/provision-onnx-runtime.py','scripts/provision-local-model.py','scripts/int8-policy.py','scripts/macho-identity.py','scripts/license-notices.py','LICENSE','NOTICE','THIRD_PARTY_NOTICES.md','third_party']
names=subprocess.check_output(['git','-c','safe.directory='+root.as_posix(),'ls-files','-z','--cached','--others','--exclude-standard','--',*scope],cwd=root)
inputs={}; identity=hashlib.sha256()
for raw in sorted(set(name for name in names.split(b'\0') if name)):
    name=os.fsdecode(raw); path=root/name
    assert path.is_file() and not path.is_symlink() and path.resolve().is_relative_to(root)
    digest=hashlib.sha256(path.read_bytes()).hexdigest();inputs[name]=digest
    identity.update(len(raw).to_bytes(8,'big'));identity.update(raw);identity.update(bytes.fromhex(digest))
native={'sha256':identity.hexdigest(),'files':len(inputs)}
assert native==builder.source_identity()=={'sha256':'b409ff3d25c46ad22cf4f179edf343492780dcb048d344cfa59a9034920ac209','files':973}
def hashes(paths):
    return {p.relative_to(root).as_posix():hashlib.sha256(p.read_bytes()).hexdigest() for p in sorted(paths) if p.is_file()}
extension_sources=hashes((root/'packages/vscode/src').rglob('*'))
compiled=hashes((root/'packages/vscode/out').rglob('*'))
assert compiled and extension_sources
compile_receipt=json.loads((out/'windows-r1/extension-compile-pinned-r3.receipt.json').read_text())
assert compile_receipt['exit_code']==0 and compile_receipt['source_after']==native
value={'status':'reviewed_integrated_source','reviewed':True,'source_root':str(root),'native_source':native,'native_inputs':inputs,'compiled_extension_reviewed':True,'extension_sources':extension_sources,'compiled_extension':compiled,'compile_receipt_sha256':hashlib.sha256((out/'windows-r1/extension-compile-pinned-r3.receipt.json').read_bytes()).hexdigest(),'entry_commit':subprocess.check_output(['git','rev-parse','HEAD'],cwd=root,text=True).strip(),'qualification':'Source freeze for current review validation; not a test or publication pass'}
path=out/'source-freeze-r1.json'
with path.open('x',encoding='utf-8') as stream:json.dump(value,stream,indent=2)
print(json.dumps({'path':str(path),'sha256':hashlib.sha256(path.read_bytes()).hexdigest(),'native_source':native,'extension_source_files':len(extension_sources),'compiled_files':len(compiled)}))
