# SPDX-License-Identifier: Apache-2.0
"""Build unchanged entry-revision Store handlers with one declared test driver."""
import hashlib,json,os,subprocess,tarfile,time
from pathlib import Path
root=Path(__file__).resolve().parents[2]
base=root/'artifacts/v190'
run=base/'p9-pre-policy-probe-20261105-r1'
run.mkdir(exist_ok=False)
origin='8c790a53b6e9ff19c5d1599617af4c7c321bb10a'
archive=run/'source.tar'
subprocess.run(['git','-c',f'safe.directory={root.as_posix()}','archive','--format=tar','--output',str(archive),origin],cwd=root,check=True)
source=run/'source';source.mkdir()
with tarfile.open(archive) as tar:
    members=tar.getmembers()
    for member in members:
        assert member.name.split('/')[0] not in {'.git','.codex','.agents','.aws','temp','artifacts','target','.mcp.json'}
        assert not member.issym() and not member.islnk()
        assert (source/member.name).resolve().is_relative_to(source)
    tar.extractall(source,filter='data')
def digest(path):return hashlib.sha256(path.read_bytes()).hexdigest()
original={str(path.relative_to(source)).replace('\\','/'):digest(path) for path in source.rglob('*') if path.is_file()}
assert all('required_source_policy' not in (source/name).read_text(encoding='utf-8') for name in ['crates/airk-store/src/service_collection.rs','crates/airk-store/src/service_collection_purge.rs'])
driver=source/'crates/airk-store/src/p9_pre_policy_probe.rs'
driver.write_bytes((base/'p9-pre-policy-owner-probe.rs').read_bytes())
lib=source/'crates/airk-store/src/lib.rs'
lib.write_bytes(lib.read_bytes()+b'\n#[cfg(test)]\nmod p9_pre_policy_probe;\n')
environment=dict(os.environ,CARGO_TARGET_DIR=str(run/'target'),CARGO_PROFILE_TEST_DEBUG='0',CARGO_PROFILE_DEV_DEBUG='0',CARGO_INCREMENTAL='0')
command=['cargo','test','-p','airk-store','--lib','--no-run','--locked','--offline','--message-format=json']
started=time.monotonic()
with (run/'build.jsonl').open('x',encoding='utf-8') as out,(run/'build.stderr').open('x',encoding='utf-8') as err:
    result=subprocess.run(command,cwd=source,env=environment,stdout=out,stderr=err)
changed={name for name,expected in original.items() if digest(source/name)!=expected}
assert changed=={'crates/airk-store/src/lib.rs'}
assert lib.read_bytes().endswith(b'\n#[cfg(test)]\nmod p9_pre_policy_probe;\n')
executable=None
for line in (run/'build.jsonl').read_text(encoding='utf-8').splitlines():
    row=json.loads(line)
    if row.get('reason')=='compiler-artifact' and row.get('executable') and row['target']['name']=='airk_store':executable=row['executable']
receipt={'origin_commit':origin,'archive_sha256':digest(archive),'original_files':original,'declared_changes':{'crates/airk-store/src/lib.rs':'Appended cfg(test) module only','crates/airk-store/src/p9_pre_policy_probe.rs':digest(driver)},'production_handler_files_unchanged':True,'command':command,'exit_code':result.returncode,'seconds':time.monotonic()-started,'executable':executable,'executable_sha256':digest(Path(executable)) if executable else None,'scope':'Original entry-revision native Store constructor, current collection metadata read and purge handlers; test helper only, no approved service-runtime claim.'}
(run/'receipt.json').write_text(json.dumps(receipt,indent=2)+'\n')
print(json.dumps({key:receipt[key] for key in ['origin_commit','exit_code','seconds','executable','executable_sha256','production_handler_files_unchanged']}))
assert result.returncode==0 and executable
