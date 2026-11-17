# SPDX-License-Identifier: Apache-2.0
"""Fetch only frozen Mac R4 evidence/metadata; runtime binaries stay on Mac."""
from pathlib import Path,PurePosixPath
import base64,hashlib,io,json,subprocess,tarfile,time
ROOT=Path.cwd();OUT=ROOT/'artifacts/advanced-split/receipts/mac-recovery-r131';auth=json.loads((OUT/'artifact-authentication.json').read_bytes())
selected={}
for name,row in auth['receipt_files'].items():selected['receipts/'+name]={'root':auth['receipt_root'],'name':name,**row}
for name,row in auth['runtime_files'].items():
 if name.endswith('.json') or name in ['AIRK.app/Contents/Info.plist','AIRK.app/Contents/PkgInfo']:
  selected['runtime-metadata/'+name]={'root':auth['runtime_root'],'name':name,**row}
program=r'''set -eu
/Users/tylerjensen/Code/airk/artifacts/tools/neo-python-3.13.18/bin/python3.13 -I -B - <<'PY'
from pathlib import Path,PurePosixPath
import base64,hashlib,io,json,os,stat,sys,tarfile
assert os.geteuid()==501
selected=json.loads(base64.b64decode('__SELECTED__'))
with tarfile.open(fileobj=sys.stdout.buffer,mode='w|') as archive:
 for name,row in sorted(selected.items()):
  relative=PurePosixPath(row['name']);assert not relative.is_absolute() and '..' not in relative.parts
  root=Path(row['root']);path=root.joinpath(*relative.parts);assert path.resolve()==path and path.is_relative_to(root)
  info=path.stat();assert stat.S_ISREG(info.st_mode) and info.st_nlink==1 and not path.is_symlink()
  data=path.read_bytes();assert len(data)==row['bytes'] and hashlib.sha256(data).hexdigest()==row['sha256']
  member=tarfile.TarInfo(name);member.size=len(data);member.mode=0o600;archive.addfile(member,io.BytesIO(data))
PY
'''.replace('__SELECTED__',base64.b64encode(json.dumps(selected).encode()).decode())
(OUT/'retrieve-evidence.sh').write_bytes(program.encode());conn=json.loads((ROOT/'artifacts/v190/neo-connection.local.json').read_bytes());ssh=['ssh','-p',str(conn['port']),'-i',conn['identity_path'],*conn['ssh_options'],conn['user']+'@'+conn['host'],'sh','-s'];archive=OUT/'r4-evidence.tar';started=time.monotonic()
with archive.open('xb') as data:
 response=subprocess.run(ssh,input=program.encode(),stdout=data,stderr=subprocess.PIPE)
(OUT/'retrieve-evidence.stderr.log').write_bytes(response.stderr)
if response.returncode:raise SystemExit(response.returncode)
destination=OUT/'retrieved';destination.mkdir(exist_ok=False);verified={}
with tarfile.open(archive) as stream:
 members=stream.getmembers();assert len(members)==len(selected) and {m.name for m in members}==set(selected)
 for member in members:
  relative=PurePosixPath(member.name);assert member.isfile() and not member.issym() and not member.islnk() and not relative.is_absolute() and '..' not in relative.parts
  payload=stream.extractfile(member).read();row=selected[member.name];assert len(payload)==row['bytes'] and hashlib.sha256(payload).hexdigest()==row['sha256']
  output=destination.joinpath(*relative.parts);assert output.resolve().is_relative_to(destination.resolve());output.parent.mkdir(parents=True,exist_ok=True)
  with output.open('xb') as target:target.write(payload)
  verified[member.name]={'bytes':len(payload),'sha256':hashlib.sha256(payload).hexdigest()}
result={'command':'existing normal-owner SSH read-only tar stream -> exact local regular-file verification','exit_code':0,'elapsed_seconds':time.monotonic()-started,'program_sha256':hashlib.sha256(program.encode()).hexdigest(),'archive_sha256':hashlib.sha256(archive.read_bytes()).hexdigest(),'verified_files':verified,'runtime_binaries':'Remain retained on Mac; all831runtimefiles remotely hashed against actual install inventory and bothimages match arm64 MachO metadata','remote_writes':False,'no_cargo_build_nn_sign_or_install':True}
(OUT/'retrieval-result.json').write_bytes((json.dumps(result,indent=2)+'\n').encode());print(json.dumps({'retrieved_files':len(verified),'seconds':result['elapsed_seconds'],'archive_sha256':result['archive_sha256'],'evidence_directory':str(destination)}))
