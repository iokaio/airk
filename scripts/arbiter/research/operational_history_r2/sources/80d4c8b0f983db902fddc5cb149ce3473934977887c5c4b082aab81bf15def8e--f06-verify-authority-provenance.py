# SPDX-License-Identifier: Apache-2.0
import hashlib,importlib.util,json
from pathlib import Path
repo=Path.cwd()
location=json.loads((repo/'artifacts/optimizations/f06-authority-runtime-location.json').read_text())
runtime=Path(location['runtime'])
def sha(path): return hashlib.sha256(path.read_bytes()).hexdigest()
pair=json.loads((runtime/'airk-runtime.json').read_text())
build=json.loads((runtime/'airk-build.json').read_text())
inventory=json.loads((runtime/'airk-install.json').read_text())
spec=importlib.util.spec_from_file_location('f06_builder',repo/'scripts/build-service-runtime.py')
builder=importlib.util.module_from_spec(spec);spec.loader.exec_module(builder)
source=builder.source_identity()
assert source=={'sha256':'5c4333f298c502844abb36a8eedf996ed6aa70fbbc8791876c899845024f5466','files':708}
actual=set()
for path in runtime.rglob('*'):
    metadata=path.lstat()
    assert not path.is_symlink() and not getattr(metadata,'st_file_attributes',0)&0x400
    if path.is_file():actual.add(path.relative_to(runtime).as_posix())
assert actual==set(inventory['files'])|{'airk-install.json'}
for name,expected in inventory['files'].items():
    assert not Path(name).is_absolute() and '..' not in Path(name).parts and chr(92) not in name
    assert sha(runtime/name)==expected,name
assert build['profile']=='release'

main,helper=(runtime/name for name in builder.runtime_names(pair['os']))
assert sha(main)==pair['airk_sha256']==inventory['files'][main.relative_to(runtime).as_posix()]
assert sha(helper)==pair['embed_sha256']==inventory['files'][helper.relative_to(runtime).as_posix()]
assert build['runtime']==pair and source==build['source']
value={'runtime':str(runtime),'install_manifest_sha256':sha(runtime/'airk-install.json'),'pair':pair,'build':build,'main_bytes':main.stat().st_size,'helper_bytes':helper.stat().st_size,'inventory_files':len(inventory['files']),'source_unchanged_after_build':True,'all_inventory_digests_and_exact_file_set_verified':True,'builder_script_sha256':sha(repo/'scripts/build-service-runtime.py'),'build_command':['python','scripts/build-service-runtime.py','--destination',str(runtime),'--profile','release'],'separate_build_contract':{'helper':['cargo','build','--locked','-p','airk-cli','--bin','airk-embed','--no-default-features','--message-format=json-render-diagnostics','--release','--features','embedding-worker'],'main':['cargo','build','--locked','-p','airk-cli','--bin','airk','--no-default-features','--message-format=json-render-diagnostics','--release']},'feature_provenance':'actual documented builder execution/source/log; checksum alone not feature proof'}
(repo/'artifacts/optimizations/f06-authority-runtime-provenance.json').write_text(json.dumps(value,indent=2)+'\n')
print(json.dumps(value,indent=2))
