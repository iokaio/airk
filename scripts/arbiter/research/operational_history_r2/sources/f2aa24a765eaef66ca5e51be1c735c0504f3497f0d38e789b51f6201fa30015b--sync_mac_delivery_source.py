# SPDX-License-Identifier: Apache-2.0
"""Reviewed frozen public-source packet -> fresh independent Neo copy; no Cargo."""
import argparse
import base64
import hashlib
import json
from pathlib import Path
import shlex
import subprocess
import sys
import time

ROOT=Path(__file__).resolve().parents[3]
HERE=Path(__file__).resolve().parent
REMOTE_TRANSFER='/Users/tylerjensen/airk-builds/advanced-split-local-delivery-source-transfer-r1'
REMOTE_DEST='/Users/tylerjensen/airk-builds/advanced-split-local-delivery-r1'
def digest(path):
    with path.open('rb') as stream:return hashlib.file_digest(stream,'sha256').hexdigest()
def main():
    parser=argparse.ArgumentParser(description=__doc__)
    parser.add_argument('--packet',type=Path,required=True)
    parser.add_argument('--expected-manifest-sha256',required=True)
    parser.add_argument('--execute-reviewed',action='store_true')
    args=parser.parse_args();packet=args.packet.resolve(strict=True);manifest=packet/'source.json'
    assert digest(manifest)==args.expected_manifest_sha256
    source=json.loads(manifest.read_bytes())
    for name,key in [('public-source.tar.gz','public_archive_sha256'),('source.bundle','git_bundle_sha256')]:
        assert digest(packet/name)==source[key]
    if not args.execute_reviewed:
        print(json.dumps({'manifest_sha256':args.expected_manifest_sha256,'head':source['head'],
            'native_source_identity':source['source_identity'],'files':len(source['files']),
            'remote_fresh_transfer':REMOTE_TRANSFER,'remote_fresh_copy':REMOTE_DEST,'executed':False},indent=2));return 0
    connection=json.loads((ROOT/'artifacts/v190/neo-connection.local.json').read_bytes())
    ssh=['ssh','-p',str(connection['port']),'-i',str(connection['identity_path']),
         *connection['ssh_options'],connection['user']+'@'+connection['host']]
    evidence=packet/'sync-receipts';evidence.mkdir(exist_ok=False)
    records=[]
    for index,name in enumerate(['source.json','public-source.tar.gz','source.bundle']):
        setup=('test ! -e '+shlex.quote(REMOTE_TRANSFER)+'; test ! -L '+shlex.quote(REMOTE_TRANSFER)+'; mkdir '+shlex.quote(REMOTE_TRANSFER)+'; ') if index==0 else ''
        command='set -eu; umask 077; '+setup+'test ! -e '+shlex.quote(REMOTE_TRANSFER+'/'+name)+'; cat > '+shlex.quote(REMOTE_TRANSFER+'/'+name)
        start=time.monotonic()
        with (packet/name).open('rb') as data:result=subprocess.run([*ssh,'sh -c '+shlex.quote(command)],stdin=data,capture_output=True,timeout=180)
        (evidence/(name+'.stderr.log')).write_bytes(result.stderr)
        records.append({'file':name,'sha256':digest(packet/name),'command':command,'exit_code':result.returncode,'elapsed_seconds':time.monotonic()-start})
        (evidence/'transfers.json').write_text(json.dumps(records,indent=2)+'\n',encoding='utf-8')
        if result.returncode:return result.returncode
    program=r'''set -eu
umask 077
/Users/tylerjensen/Code/airk/artifacts/tools/neo-python-3.13.18/bin/python3.13 -I -B - <<'PY'
import hashlib,importlib.util,json,os,stat,subprocess,tarfile,time
from pathlib import Path,PurePosixPath
original=Path('/Users/tylerjensen/Code/airk');transfer=Path('__TRANSFER__');delivery=Path('__DEST__')
assert os.geteuid()==501 and not delivery.exists() and not delivery.is_symlink()
env=dict(os.environ,GIT_OPTIONAL_LOCKS='0')
def run(*args,cwd=None):return subprocess.check_output(list(args),cwd=cwd,env=env,text=True).strip()
def sha(path):
 with path.open('rb') as stream:return hashlib.file_digest(stream,'sha256').hexdigest()
raw=(transfer/'source.json').read_bytes();assert hashlib.sha256(raw).hexdigest()=='__MANIFEST__'
packet=json.loads(raw)
for name,key in [('public-source.tar.gz','public_archive_sha256'),('source.bundle','git_bundle_sha256')]:assert sha(transfer/name)==packet[key]
before={'head':run('/usr/bin/git','rev-parse','HEAD',cwd=original),'index':sha(original/'.git/index'),
 'status':run('/usr/bin/git','status','--porcelain=v1','--untracked-files=no',cwd=original)}
assert before['head']==packet['remote_base']
delivery.mkdir(mode=0o700);root=delivery/'source'
subprocess.run(['/usr/bin/git','clone','--no-hardlinks','--no-checkout',str(original),str(root)],env=env,check=True)
subprocess.run(['/usr/bin/git','-C',str(root),'config','core.autocrlf','false'],env=env,check=True)
subprocess.run(['/usr/bin/git','-C',str(root),'fetch',str(transfer/'source.bundle'),'HEAD'],env=env,check=True)
subprocess.run(['/usr/bin/git','-C',str(root),'checkout','--detach','FETCH_HEAD'],env=env,check=True)
assert run('/usr/bin/git','rev-parse','HEAD',cwd=root)==packet['head']
def safe(name):
 relative=PurePosixPath(name);assert not relative.is_absolute()
 assert not set(relative.parts)&{'..','.git','artifacts','temp','.airk','node_modules','target'}
 target=root.joinpath(*relative.parts)
 for parent in (target,*target.parents):
  assert not parent.is_symlink()
  if parent==root:break
 return target
with tarfile.open(transfer/'public-source.tar.gz','r:gz') as archive:
 expected={name:row for name,row in packet['files'].items() if row['sha256']!='missing'}
 members=archive.getmembers();assert len(members)==len(expected) and {member.name for member in members}==set(expected)
 for member in members:
  assert member.isfile() and not member.issym() and not member.islnk()
  safe(member.name)
 for name,row in packet['files'].items():
  target=safe(name)
  if target.exists():assert target.is_file() and stat.S_ISREG(target.stat().st_mode) and target.stat().st_nlink==1
  if row['sha256']=='missing':
   if target.exists():target.unlink()
  else:
   data=archive.extractfile(name).read();assert len(data)==row['bytes'] and hashlib.sha256(data).hexdigest()==row['sha256']
   target.parent.mkdir(parents=True,exist_ok=True)
   with target.open('wb') as stream:stream.write(data)
   target.chmod(row['mode'])
for name,row in packet['files'].items():
 target=safe(name)
 assert not target.exists() if row['sha256']=='missing' else sha(target)==row['sha256']
os.chdir(root)
spec=importlib.util.spec_from_file_location('independent_runtime_source_owner',root/'scripts/build-service-runtime.py')
owner=importlib.util.module_from_spec(spec);spec.loader.exec_module(owner)
assert owner.source_identity()==packet['source_identity']
after={'head':run('/usr/bin/git','rev-parse','HEAD',cwd=original),'index':sha(original/'.git/index'),
 'status':run('/usr/bin/git','status','--porcelain=v1','--untracked-files=no',cwd=original)}
assert before==after
receipt={'schema':'airk.private-independent-mac-source-copy/1','result':'exact_source_copy_verified','source_identity':packet['source_identity'],
 'head':packet['head'],'manifest_sha256':'__MANIFEST__','files':len(packet['files']),
 'source_root':str(root),'original_head_index_status_unchanged':True,'original_status_sha256':hashlib.sha256(before['status'].encode()).hexdigest(),
 'original_index_sha256':before['index'],'no_cargo_build_sign_install_or_service_change':True}
(transfer/'source-copy-result.json').write_text(json.dumps(receipt,indent=2)+'\n')
print(json.dumps(receipt))
PY
'''.replace('__TRANSFER__',REMOTE_TRANSFER).replace('__DEST__',REMOTE_DEST).replace('__MANIFEST__',args.expected_manifest_sha256)
    (evidence/'apply-independent-source.sh').write_text(program,encoding='utf-8')
    start=time.monotonic();result=subprocess.run([*ssh,'sh','-s'],input=program.encode(),capture_output=True,timeout=180)
    (evidence/'apply.stdout.log').write_bytes(result.stdout);(evidence/'apply.stderr.log').write_bytes(result.stderr)
    record={'command':'existing SSH -> POSIXsh -s','stdin_sha256':hashlib.sha256(program.encode()).hexdigest(),
        'exit_code':result.returncode,'elapsed_seconds':time.monotonic()-start,'credentials_printed':False}
    if result.returncode==0:record['result']=json.loads(result.stdout)
    (evidence/'result.json').write_text(json.dumps(record,indent=2)+'\n',encoding='utf-8')
    print(json.dumps(record));return result.returncode
if __name__=='__main__':raise SystemExit(main())
