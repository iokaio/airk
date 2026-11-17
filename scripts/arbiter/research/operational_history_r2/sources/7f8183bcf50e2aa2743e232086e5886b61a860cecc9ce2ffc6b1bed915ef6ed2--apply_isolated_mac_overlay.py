# SPDX-License-Identifier: Apache-2.0
"""Apply the Root-authorized two-file overlay only to isolated Mac R2, without Cargo."""
from pathlib import Path
import base64,hashlib,json,subprocess,time
ROOT=Path(__file__).resolve().parents[3]
HERE=Path(__file__).resolve().parent
sha=lambda raw:hashlib.sha256(raw).hexdigest()
manifest=json.loads((HERE/'preparation.json').read_bytes())
rows=[]
for row in manifest['files']:
    raw=(ROOT/row['proposed']).read_bytes();assert sha(raw)==row['proposed_sha256']
    rows.append({**row,'body':base64.b64encode(raw).decode()})
payload=base64.b64encode(json.dumps(rows).encode()).decode()
remote=r"""import base64,hashlib,importlib.util,json,os,platform,stat,subprocess,time
from pathlib import Path
SOURCE=Path('/Users/tylerjensen/airk-builds/advanced-split-local-delivery-r2/source')
BASE=SOURCE.parent
OUT=BASE/'receipts/mac-platform-typefix-r1'
ORIGINAL=Path('/Users/tylerjensen/Code/airk')
sha=lambda raw:hashlib.sha256(raw).hexdigest()
rows=json.loads(base64.b64decode('__PAYLOAD__'))
def plain(path,regular=False):
    assert path.is_absolute() and path.resolve(strict=True)==path
    for item in (path,*path.parents):
        info=item.lstat();assert not stat.S_ISLNK(info.st_mode)
    info=path.stat()
    if regular:assert stat.S_ISREG(info.st_mode) and info.st_nlink==1
    return info
def load(name,path):
    spec=importlib.util.spec_from_file_location(name,path);module=importlib.util.module_from_spec(spec);spec.loader.exec_module(module);return module
def original_state():
    return {'head':subprocess.check_output(['git','--no-optional-locks','-C',str(ORIGINAL),'rev-parse','HEAD'],text=True).strip(),
            'status_sha256':sha(subprocess.check_output(['git','--no-optional-locks','-C',str(ORIGINAL),'status','--porcelain=v1','-uno'])),
            'index_sha256':sha((ORIGINAL/'.git/index').read_bytes())}
assert os.geteuid()==501 and platform.system()=='Darwin' and platform.machine()=='arm64'
plain(SOURCE);plain(BASE/'receipts');plain(ORIGINAL)
assert SOURCE!=ORIGINAL and not SOURCE.is_relative_to(ORIGINAL) and not OUT.exists()
assert {row['target'] for row in rows}=={'crates/airk-retrieval/src/onnx_macos.rs','crates/airk-store/src/service/arbiter_assets/offline.rs'}
cleanup=json.loads((BASE/'receipts/native-platform-r3/result.json').read_bytes())
assert cleanup['cleanup']['exit_code']==0 and cleanup['cleanup']['compiler_descendants_absent']
os.chdir(SOURCE)
owner=load('mac_typefix_receipt_owner',SOURCE/'scripts/dev/receipt_runner.py')
runtime=load('mac_typefix_runtime_owner',SOURCE/'scripts/build-service-runtime.py')
before=owner.observe(['native','harness','fixtures','frontend'])
assert before['native']['sha256']=='eac7b9aac9a5a9943422a08406dd9602da55a35d2d7ed7cbceb24a2f4bdc29c5'
original_before=original_state();runtime_before=runtime.source_identity()
prepared=[]
for row in rows:
    path=SOURCE/row['target'];info=plain(path,True);assert info.st_uid==os.geteuid()
    raw=path.read_bytes();assert sha(raw)==row['base_sha256']
    body=base64.b64decode(row['body']);assert sha(body)==row['proposed_sha256']
    temporary=path.with_name(path.name+'.mac-typefix-r1.pending');assert not temporary.exists() and not temporary.is_symlink()
    prepared.append((row,path,raw,body,(info.st_dev,info.st_ino)))
OUT.mkdir(mode=0o700)
(OUT/'before.json').write_text(json.dumps({'groups':before,'runtime_source_identity':runtime_before,'original':original_before,'cleanup_retained':cleanup['cleanup']},indent=2)+'\n')
for row,path,raw,body,identity in prepared:
    backup=OUT/row['target'];backup.parent.mkdir(parents=True,exist_ok=True)
    with backup.open('xb') as stream:stream.write(raw);stream.flush();os.fsync(stream.fileno())
    assert sha(backup.read_bytes())==row['base_sha256']
for row,path,raw,body,identity in prepared:
    info=plain(path,True);assert (info.st_dev,info.st_ino)==identity and sha(path.read_bytes())==row['base_sha256']
    temporary=path.with_name(path.name+'.mac-typefix-r1.pending')
    with temporary.open('xb') as stream:stream.write(body);stream.flush();os.fsync(stream.fileno())
    plain(temporary,True)
    assert sha(path.read_bytes())==row['base_sha256']
    os.replace(temporary,path)
    plain(path,True);assert sha(path.read_bytes())==row['proposed_sha256']
after=owner.observe(['native','harness','fixtures','frontend']);runtime_after=runtime.source_identity()
changes={key for key,value in before['native']['files'].items() if after['native']['files'].get(key)!=value}
assert changes=={row['target'] for row in rows}
assert set(before['native']['files'])==set(after['native']['files'])
assert all(before[group]==after[group] for group in ('harness','fixtures','frontend'))
original_after=original_state();assert original_before==original_after
result={'schema':'airk.private-isolated-mac-typefix-overlay/1','status':'applied_exact_two_files','source':str(SOURCE),'backup_receipts':str(OUT),'native_before_sha256':before['native']['sha256'],'native_after_sha256':after['native']['sha256'],'runtime_source_identity_before':runtime_before,'runtime_source_identity_after':runtime_after,'files':[{k:v for k,v in row.items() if k!='body'} for row in rows],'other_observed_groups_unchanged':True,'original_checkout_unchanged':original_before==original_after,'prior_cleanup_retained':True,'Cargo_or_NN':False,'qualification_granted':False}
(OUT/'after.json').write_text(json.dumps({'groups':after,'runtime_source_identity':runtime_after,'original':original_after},indent=2)+'\n')
with (OUT/'result.json').open('x') as stream:json.dump(result,stream,indent=2);stream.write('\n')
print(json.dumps(result))
""".replace('__PAYLOAD__',payload)
connection=json.loads((ROOT/'artifacts/v190/neo-connection.local.json').read_bytes())
command=['ssh','-p',str(connection['port']),'-i',connection['identity_path'],*connection['ssh_options'],connection['user']+'@'+connection['host'],'sh','-s']
script="set -eu\numask 077\n/Users/tylerjensen/Code/airk/artifacts/tools/neo-python-3.13.18/bin/python3.13 -I -B - <<'AIRK_TYPEFIX_PY'\n"+remote+"\nAIRK_TYPEFIX_PY\n"
(HERE/'remote-overlay-exact.py').write_text(remote,encoding='utf-8',newline='\n')
started=time.monotonic()
result=subprocess.run(command,input=script.encode(),capture_output=True,timeout=120)
(HERE/'ssh.stdout.log').write_bytes(result.stdout);(HERE/'ssh.stderr.log').write_bytes(result.stderr)
record={'exit_code':result.returncode,'elapsed_seconds':time.monotonic()-started,'remote_program_sha256':sha(remote.encode()),'normal_owner_context_required':True,'Cargo_or_NN':False}
if result.returncode==0:
    observation=json.loads(result.stdout);(HERE/'overlay-result.json').write_text(json.dumps(observation,indent=2)+'\n',encoding='utf-8');record['observation']=observation
with (HERE/'ssh-result.json').open('x',encoding='utf-8') as stream:json.dump(record,stream,indent=2);stream.write('\n')
print(json.dumps(record))
if result.returncode:print(result.stderr.decode('utf-8',errors='replace'))
raise SystemExit(result.returncode)
