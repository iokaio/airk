# SPDX-License-Identifier: Apache-2.0
import hashlib,json,os,shutil,subprocess,tarfile,time
from pathlib import Path
run=Path(__file__).resolve().parent
assert run.parent==Path.home()/'airk-builds' and run.name.startswith('p9-native-')
record=json.loads((run/'pre-policy-inputs.json').read_text())
def digest(path):return hashlib.sha256(path.read_bytes()).hexdigest()
archive=run/'pre-policy-source.tar'
assert digest(archive)==record['archive_sha256']
source=run/'pre-policy-source';source.mkdir()
with tarfile.open(archive) as tar:
    for member in tar.getmembers():
        assert member.name.split('/')[0] not in {'.git','.codex','.agents','.aws','temp','artifacts','target','.mcp.json'}
        assert not member.issym() and not member.islnk() and (source/member.name).resolve().is_relative_to(source)
    tar.extractall(source,filter='data')
for name,expected in record['original_files'].items():assert digest(source/name)==expected,name
driver=source/'crates/airk-store/src/p9_pre_policy_probe.rs'
driver.write_bytes((run/'pre-policy-driver.rs').read_bytes())
assert digest(driver)==record['declared_changes']['crates/airk-store/src/p9_pre_policy_probe.rs']
lib=source/'crates/airk-store/src/lib.rs'
lib.write_bytes(lib.read_bytes()+b'\n#[cfg(test)]\nmod p9_pre_policy_probe;\n')
environment=dict(os.environ,CARGO_TARGET_DIR=str(run/'target'),CARGO_PROFILE_TEST_DEBUG='0',CARGO_PROFILE_DEV_DEBUG='0',CARGO_INCREMENTAL='0')
command=['cargo','test','-p','airk-store','--lib','--no-run','--locked','--offline','--message-format=json']
started=time.monotonic()
with (run/'pre-policy-build.jsonl').open('x') as out,(run/'pre-policy-build.stderr').open('x') as err:
    result=subprocess.run(command,cwd=source,env=environment,stdout=out,stderr=err)
changed={name for name,expected in record['original_files'].items() if digest(source/name)!=expected}
assert changed=={'crates/airk-store/src/lib.rs'}
executable=None
for line in (run/'pre-policy-build.jsonl').read_text().splitlines():
    row=json.loads(line)
    if row.get('reason')=='compiler-artifact' and row.get('executable') and row['target']['name']=='airk_store':executable=row['executable']
receipt={key:record[key] for key in ['origin_commit','archive_sha256','declared_changes']}
if executable:
    frozen=run/'target/p9-pre-policy-native-helper'
    assert not frozen.exists()
    shutil.copy2(executable,frozen)
    executable=str(frozen)
receipt.update(production_handler_files_unchanged=True,command=command,exit_code=result.returncode,seconds=time.monotonic()-started,executable=executable,executable_sha256=digest(Path(executable)) if executable else None,target=str(run/'target'),cleanup_owner='owner.py finally cargo clean on this exact target; helper executable is not retained afterward')
(run/'pre-policy-probe.json').write_text(json.dumps(receipt,indent=2)+'\n')
print(json.dumps({key:receipt[key] for key in ['origin_commit','exit_code','seconds','executable_sha256','production_handler_files_unchanged']}),flush=True)
assert result.returncode==0 and executable
