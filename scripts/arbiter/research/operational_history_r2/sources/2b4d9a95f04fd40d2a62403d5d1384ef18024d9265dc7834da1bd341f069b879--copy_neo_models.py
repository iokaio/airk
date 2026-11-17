# SPDX-License-Identifier: Apache-2.0
"""Copy exactly the root-authorized original eight MiniLMv2 files; no NN/build."""
import base64
import hashlib
import io
import json
from pathlib import Path
import shlex
import stat
import subprocess
import tarfile
import time

ROOT = Path(__file__).resolve().parents[3]
HERE = Path(__file__).resolve().parent
candidate = '/Users/tylerjensen/airk-builds/advanced-split-local-delivery-prerequisites-r1'
connection = json.loads((ROOT/'artifacts/v190/neo-connection.local.json').read_bytes())
ssh = ['ssh', '-p', str(connection['port']), '-i', str(connection['identity_path']),
       *connection['ssh_options'], connection['user']+'@'+connection['host']]
approved = json.loads((HERE/'windows-input-identities-current-r5.json').read_bytes())
source = ROOT/'artifacts/advanced-split/bundled-model-v2'
catalog = ROOT/'crates/airk-retrieval/src/model_catalog.json'
assert hashlib.sha256(catalog.read_bytes()).hexdigest() == approved['identities']['crates/airk-retrieval/src/model_catalog.json']['sha256']
expected = {profile['id']+'/'+name: {'bytes': row['bytes'], 'sha256': row['sha256']}
            for profile in approved['bundled_profiles'] for name, row in profile['files'].items()}
assert set(expected) == {profile+'/'+name for profile in ('minilm-l6-v2-u8-v2','minilm-l6-v2-s8-v2')
                         for name in ('LICENSE','model.json','model.onnx','tokenizer.json')}
assert {path.relative_to(source).as_posix() for path in source.rglob('*') if path.is_file()} == set(expected)
archive = HERE/'original-minilm-v2-eight-files.tar'
assert not archive.exists()
with tarfile.open(archive,'w') as stream:
    for name, row in sorted(expected.items()):
        path = source/name
        assert path.is_file() and not path.is_symlink() and path.stat().st_nlink == 1
        assert all(not parent.is_symlink() for parent in path.parents)
        data = path.read_bytes()
        assert len(data) == row['bytes'] and hashlib.sha256(data).hexdigest() == row['sha256']
        member = tarfile.TarInfo(name); member.size = len(data); member.mode = 0o600
        stream.addfile(member,io.BytesIO(data))
with archive.open('rb') as stream:
    archive_sha = hashlib.file_digest(stream,'sha256').hexdigest()
packet = {'files':expected,'archive_sha256':archive_sha,'archive_bytes':archive.stat().st_size,
          'catalog_sha256':hashlib.sha256(catalog.read_bytes()).hexdigest(),
          'source':'root-authorized existing bundled-model-v2; not parent-bag export'}
(HERE/'neo-model-transfer-inputs.json').write_text(json.dumps(packet,indent=2)+'\n',encoding='utf-8')
transfer_command = ('set -eu; umask 077; test -d '+shlex.quote(candidate)+'; test ! -L '+shlex.quote(candidate)+
                    '; test ! -e '+shlex.quote(candidate+'/model-transfer')+'; mkdir '+shlex.quote(candidate+'/model-transfer')+
                    '; cat > '+shlex.quote(candidate+'/model-transfer/original-minilm-v2-eight-files.tar'))
started = time.monotonic()
with archive.open('rb') as stream:
    transferred = subprocess.run([*ssh,'sh -c '+shlex.quote(transfer_command)],stdin=stream,capture_output=True,timeout=180)
(HERE/'neo-model-transfer.stdout.log').write_bytes(transferred.stdout)
(HERE/'neo-model-transfer.stderr.log').write_bytes(transferred.stderr)
transfer_receipt = {'schema':'airk.private-neo-model-transfer/1','command':'existing SSH -> POSIX sh -c guarded fresh candidate cat',
                    'remote_command':transfer_command,'exit':transferred.returncode,
                    'elapsed_seconds':time.monotonic()-started,**packet,'credentials_printed':False}
(HERE/'neo-model-transfer.json').write_text(json.dumps(transfer_receipt,indent=2)+'\n',encoding='utf-8')
if transferred.returncode:
    raise SystemExit(transferred.returncode)
remote = r'''#!/bin/sh
set -eu
umask 077
[ "$(/usr/bin/id -u)" = 501 ]
/Users/tylerjensen/Code/airk/artifacts/tools/neo-python-3.13.18/bin/python3.13 -I -B - <<'PY'
import base64,hashlib,json,stat,tarfile,time
from pathlib import Path
root=Path('/Users/tylerjensen/airk-builds/advanced-split-local-delivery-prerequisites-r1')
assert root.is_dir() and not root.is_symlink()
packet=json.loads(base64.b64decode('__PACKET__')); expected=packet['files']
started=time.monotonic(); index=Path('/Users/tylerjensen/Code/airk/.git/index')
before=hashlib.sha256(index.read_bytes()).hexdigest()
node_receipt=json.loads((root/'receipts/node-result.json').read_bytes())
assert node_receipt['result']=='verified_prerequisite'
archive=root/'model-transfer/original-minilm-v2-eight-files.tar'
assert archive.is_file() and not archive.is_symlink() and archive.stat().st_size==packet['archive_bytes']
with archive.open('rb') as stream:assert hashlib.file_digest(stream,'sha256').hexdigest()==packet['archive_sha256']
destination=root/'inputs/bundled-model-v2'
assert not destination.exists() and not destination.is_symlink()
payload={}
with tarfile.open(archive,'r:') as stream:
 members=stream.getmembers()
 assert len(members)==8 and len({member.name for member in members})==8 and {member.name for member in members}==set(expected)
 for member in members:
  assert member.isfile() and not member.issym() and not member.islnk()
  data=stream.extractfile(member).read(); row=expected[member.name]
  assert len(data)==row['bytes'] and hashlib.sha256(data).hexdigest()==row['sha256']
  payload[member.name]=data
destination.mkdir(parents=True)
for name,data in payload.items():
 path=destination/name; assert path.is_relative_to(destination)
 path.parent.mkdir(exist_ok=True)
 with path.open('xb') as stream:stream.write(data)
actual={}
for path in destination.rglob('*'):
 if path.is_file():
  assert not path.is_symlink() and stat.S_ISREG(path.stat().st_mode) and path.stat().st_nlink==1
  actual[path.relative_to(destination).as_posix()]={'bytes':path.stat().st_size,'sha256':hashlib.sha256(path.read_bytes()).hexdigest()}
assert actual==expected
after=hashlib.sha256(index.read_bytes()).hexdigest(); assert before==after
receipt={'schema':'airk.private-neo-model-prerequisite/1','result':'exact_original_eight_files_verified','elapsed_seconds':time.monotonic()-started,'directory':str(destination),'files':actual,'archive_sha256':packet['archive_sha256'],'source_catalog_sha256':packet['catalog_sha256'],'source':packet['source'],'original_git_index_before':before,'original_git_index_after':after,'original_git_index_unchanged':True,'no_cargo_nn_source_install_service_mutations':True,'model_quality_or_new_model_promotion_claimed':False}
(root/'receipts/models-result.json').write_text(json.dumps(receipt,indent=2)+'\n')
print(json.dumps(receipt))
PY
'''.replace('__PACKET__',base64.b64encode(json.dumps(packet).encode()).decode())
(HERE/'verify-neo-models.sh').write_text(remote,encoding='utf-8')
started = time.monotonic()
verified = subprocess.run([*ssh,'sh','-s'],input=remote.encode(),capture_output=True,timeout=60)
(HERE/'neo-model-verification.stdout.log').write_bytes(verified.stdout)
(HERE/'neo-model-verification.stderr.log').write_bytes(verified.stderr)
result = {'schema':'airk.private-neo-model-verification-command/1','command':'existing SSH -> POSIX sh -s',
          'stdin_file':'verify-neo-models.sh','stdin_sha256':hashlib.sha256(remote.encode()).hexdigest(),
          'exit':verified.returncode,'elapsed_seconds':time.monotonic()-started,'credentials_printed':False}
if verified.stdout.strip():
    try:result['remote_result']=json.loads(verified.stdout)
    except json.JSONDecodeError:result['remote_result_parse']='not_json; inspect retained stdout'
for name,row in expected.items():
    path=source/name
    assert path.stat().st_size==row['bytes'] and hashlib.sha256(path.read_bytes()).hexdigest()==row['sha256']
assert hashlib.sha256(catalog.read_bytes()).hexdigest()==packet['catalog_sha256']
result['local_original_source_still_matches']=True
(HERE/'neo-model-verification.json').write_text(json.dumps(result,indent=2)+'\n',encoding='utf-8')
print(json.dumps({'transfer_exit':transferred.returncode,'transfer_seconds':transfer_receipt['elapsed_seconds'],
                  'verification_exit':verified.returncode,'verification_seconds':result['elapsed_seconds'],
                  'remote_result':result.get('remote_result')}))
raise SystemExit(verified.returncode)
