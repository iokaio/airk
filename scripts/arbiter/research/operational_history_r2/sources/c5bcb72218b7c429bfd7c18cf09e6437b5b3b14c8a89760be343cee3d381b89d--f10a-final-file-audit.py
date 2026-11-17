# SPDX-License-Identifier: Apache-2.0
from pathlib import Path
import datetime,hashlib,importlib.util,json
root=Path.cwd();a=root/'artifacts/optimizations';loc=json.loads((a/'f10a-runtime-location.json').read_text());task=Path(loc['task']);runtime=Path(loc['runtime']);sha=lambda p:hashlib.sha256(p.read_bytes()).hexdigest()
def guard(p):
 m=p.lstat();assert not p.is_symlink() and not getattr(m,'st_file_attributes',0)&0x400;return m
assert sha(runtime/'airk-install.json')=='9d7b6406b2544db4c644fdacd171c4ddda3e77fbfa576fe842aa0fa3badb1bbe'
inventory=json.loads((runtime/'airk-install.json').read_text())['files'];actual=set()
for path in runtime.rglob('*'):
 guard(path)
 if path.is_file():actual.add(path.relative_to(runtime).as_posix())
assert actual==set(inventory)|{'airk-install.json'}
for name,digest in inventory.items():assert sha(runtime/name)==digest,name
for row in loc['scripts']:assert sha(task/'scripts'/row['name'])==row['sha256']
supplement=json.loads((a/'f10a-scope-corrected-copy.json').read_text());assert sha(Path(supplement['destination']))==supplement['sha256']
assets=json.loads((a/'cargo-clean-assets.json').read_text());asset_count=0
for name,entries in assets.items():
 folder=root/'target'/name;guard(folder);assert {p.relative_to(folder).as_posix() for p in folder.rglob('*') if p.is_file()}==set(entries)
 for relative,expected in entries.items():
  p=folder/relative;m=guard(p);assert m.st_size==expected['bytes'] and sha(p)==expected['sha256'];asset_count+=1
empty={name:len(list((task/name).iterdir())) for name in ['fixtures','target']};assert not any(empty.values())
spec=importlib.util.spec_from_file_location('f10a_final_builder',root/'scripts/build-service-runtime.py');b=importlib.util.module_from_spec(spec);spec.loader.exec_module(b);source=b.source_identity();assert source=={'sha256':'78ed038cbb33c609777fb09a7f5240dc7f92708bb44e1378f314240534483114','files':715}
r={'utc':datetime.datetime.now(datetime.timezone.utc).isoformat(),'native_source':source,'current_runtime_exact_files_verified':len(inventory),'verified_helper_copies':len(loc['scripts'])+1,'offline_assets_exact_files_sizes_hashes':asset_count,'owned_fixture_entries':empty,'scope':'current task/pair/reused assets only; previous pair proofs preserved, no prior pair reads or changes'}
(a/'f10a-final-file-audit.json').write_text(json.dumps(r,indent=2)+'\n');print(json.dumps(r,indent=2))
