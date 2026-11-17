# SPDX-License-Identifier: Apache-2.0
from pathlib import Path
import datetime,hashlib,importlib.util,json
root=Path.cwd();a=root/'artifacts/optimizations';location=json.loads((a/'f09-runtime-location.json').read_text());task=Path(location['task'])
def sha(path):return hashlib.sha256(path.read_bytes()).hexdigest()
def guard(path):
    metadata=path.lstat()
    assert not path.is_symlink() and not getattr(metadata,'st_file_attributes',0)&0x400
    return metadata
def verify_pair(directory,expected):
    assert guard(directory) and directory.is_dir()
    manifest=directory/'airk-install.json';assert sha(manifest)==expected
    inventory=json.loads(manifest.read_text());actual=set()
    for path in directory.rglob('*'):
        guard(path)
        if path.is_file():actual.add(path.relative_to(directory).as_posix())
    assert actual==set(inventory['files'])|{'airk-install.json'}
    for name,digest in inventory['files'].items():assert sha(directory/name)==digest,name
    return len(inventory['files'])
current_files=verify_pair(Path(location['runtime']),'e1b417d27012f37fd41300bd2e0f943f34e2e9abd924ea181cbae07bdac37a00')
base=task.parent
old={
    'airk-optimizations-f01c-r19x7kro':'356a1974171e9c01718aaf5bd17eea09719793769572d3eba32713636d85b78b',
    'airk-optimizations-f03-e934ix1m':'0df2101c5cb2399ef3b9f093eece22ad909f2f1f69bb39651775ce1ae4982091',
    'airk-optimizations-f04-zdpqet0e':'ca91b8ff16ca216a2de5580cf5b86cb7025ac434435d3cd6ae1500c282ef7fda',
    'airk-optimizations-f05-a6hqtiys':'f9ba343e976490bb16c1b4bcc4feeee7cd09165021b24dafb21777719c1057b8',
    'airk-optimizations-f06-_urtf6kk':'7c6fdcba0bebf1f26f09959d9288db532a8ec8244383249ca4c1537b6d52ed8c',
    'airk-optimizations-f06-authority-fnw0nvde':'c4c6bae94f096f7e1531fea00dc502b878a356121b353708e758eb2c44bee6a9',
}
old_counts={name:verify_pair(base/name/'runtime',digest) for name,digest in old.items()}
assets=json.loads((a/'cargo-clean-assets.json').read_text());asset_count=0
for folder,entries in assets.items():
    directory=root/'target'/folder;guard(directory)
    actual={path.relative_to(directory).as_posix() for path in directory.rglob('*') if path.is_file()}
    assert actual==set(entries)
    for name,expected in entries.items():
        path=directory/name;metadata=guard(path)
        assert metadata.st_size==expected['bytes'] and sha(path)==expected['sha256'];asset_count+=1
for item in location['scripts']:assert sha(task/'scripts'/item['name'])==item['sha256']
for receipt in ['f09-corrected-driver-copy.json','f09-console-client-corrected-copy.json']:
    record=json.loads((a/receipt).read_text());assert sha(Path(record['path']))==record['sha256']
assert sha(root/'scripts/check-service-console-client.cjs')==json.loads((a/'f09-console-client-corrected-copy.json').read_text())['sha256']
compiled=json.loads((a/'f09-console-client-out-copy.json').read_text())
for folder in [Path(compiled['source']),Path(compiled['destination'])]:
    guard(folder);actual=set()
    for path in folder.rglob('*'):
        metadata=guard(path)
        if path.is_file():
            name=path.relative_to(folder).as_posix();actual.add(name)
            assert sha(path)==compiled['files_inventory'][name]['sha256'] and metadata.st_size==compiled['files_inventory'][name]['bytes']
    assert actual==set(compiled['files_inventory'])
empty={name:len(list((task/name).iterdir())) for name in ['target','fixtures']};assert not any(empty.values())
spec=importlib.util.spec_from_file_location('f09_final_source',root/'scripts/build-service-runtime.py');builder=importlib.util.module_from_spec(spec);spec.loader.exec_module(builder)
source=builder.source_identity();assert source=={'sha256':'568283bade24bee26a5f6fa24bc025e9e75a57e636f32192f6660c9b2e9af64f','files':708}
result={'utc':datetime.datetime.now(datetime.timezone.utc).isoformat(),'native_source':source,'current_runtime_all_files_exact_verified':current_files,
        'preserved_prior_runtimes_all_file_counts':old_counts,'offline_assets_exact_files_sizes_hashes':asset_count,'verified_helper_copies':len(location['scripts'])+2,
        'compiled_source_and_destination_files_each':compiled['files'],'owned_fixture_entries':empty}
(a/'f09-final-file-audit.json').write_text(json.dumps(result,indent=2)+'\n');print(json.dumps(result,indent=2))
