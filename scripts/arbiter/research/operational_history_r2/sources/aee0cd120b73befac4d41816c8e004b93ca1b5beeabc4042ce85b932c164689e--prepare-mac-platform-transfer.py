# SPDX-License-Identifier: Apache-2.0
"""Prepare frozen public source archive; --apply requires exact root-reviewed seal.
Never invokes Cargo or transfers models/artifact/private state. Keeps remote recovery bytes.
"""
import argparse,gzip,hashlib,io,json,subprocess,tarfile
from pathlib import Path
p=argparse.ArgumentParser();p.add_argument('--plan',type=Path,default=Path('artifacts/advanced-split/mac-platform-source-plan.json'));p.add_argument('--expected-seal',required=True);p.add_argument('--apply',action='store_true');args=p.parse_args()
plan=json.loads(args.plan.read_text());rows=plan['source_files']
actual=hashlib.sha256(json.dumps(rows,sort_keys=True,separators=(',',':')).encode()).hexdigest()
if args.expected_seal!=actual or actual!=plan['source_set_sha256']:raise SystemExit('Reviewed source seal differs')
payload=io.BytesIO()
with tarfile.open(fileobj=payload,mode='w') as archive:
 metadata=json.dumps({'seal':actual,'files':rows,'preserved_extra_source':plan.get('preserved_extra_source',{})},sort_keys=True).encode();entry=tarfile.TarInfo('.sync-manifest.json');entry.size=len(metadata);archive.addfile(entry,io.BytesIO(metadata))
 for name,row in rows.items():
  path=Path(name)
  if path.is_symlink() or not path.is_file() or path.stat().st_size!=row['bytes'] or hashlib.sha256(path.read_bytes()).hexdigest()!=row['sha256']:raise SystemExit('Source drift before transfer: '+name)
  if path.is_absolute() or any(part in ('..','artifacts','temp','.airk','.git','node_modules') for part in path.parts):raise SystemExit('Unsafe public source path')
  content=path.read_bytes();entry=tarfile.TarInfo(name);entry.size=len(content);entry.mode=0o755 if path.stat().st_mode&0o111 else 0o644;archive.addfile(entry,io.BytesIO(content))
payload=io.BytesIO(gzip.compress(payload.getvalue(),mtime=0))
out=Path('artifacts/advanced-split/receipts')/('mac-platform-source-'+actual[:16]+'.tar.gz')
if out.exists():
 if out.read_bytes()!=payload.getvalue():raise SystemExit('Existing source archive evidence differs')
else:out.write_bytes(payload.getvalue())
print(json.dumps({'seal':actual,'files':len(rows),'archive':str(out),'archive_sha256':hashlib.sha256(payload.getvalue()).hexdigest(),'apply':args.apply}))
if not args.apply:raise SystemExit(0)
connection=json.loads(Path('artifacts/v190/neo-connection.local.json').read_text());ssh=['ssh',*connection['ssh_options'],'-i',connection['identity_path'],'-p',str(connection['port']),connection['user']+'@'+connection['host']]
# This private staging directory and its entire original-byte recovery stay out of source/package manifests.
stage='artifacts/advanced-split/receipts/mac-platform-sync-'+actual[:16]
command="'cd \"$HOME/code/airk\" && test ! -e "+stage+" && mkdir "+stage+" && cat > "+stage+"/source.tar.gz'"
r=subprocess.run(ssh+['sh','-c',command],input=payload.getvalue(),capture_output=True,timeout=90)
if r.returncode:raise SystemExit(r.stderr.decode())
remote=r'''set -eu
cd "$HOME/code/airk"
/usr/bin/python3 - __STAGE__ __SHA__ <<'PY'
import hashlib,io,json,os,stat,subprocess,sys,tarfile,tempfile
from pathlib import Path,PurePosixPath
root=Path.cwd().resolve();stage=root/sys.argv[1];archive=stage/'source.tar.gz'
if hashlib.sha256(archive.read_bytes()).hexdigest()!=sys.argv[2]:raise ValueError('Transferred archive differs')
with tarfile.open(archive,'r:gz') as stream:
 metadata=json.loads(stream.extractfile('.sync-manifest.json').read());files=metadata['files'];members=stream.getmembers()
 if len(members)!=len(files)+1 or {m.name for m in members}!={'.sync-manifest.json',*files}:raise ValueError('Unexpected sync archive roster')
 remote_names=subprocess.check_output(['git','-c','safe.directory='+str(root),'ls-files','--cached','--others','--exclude-standard','--','crates'],text=True).splitlines()
 extras=[name for name in remote_names if name not in files and (root/name).exists() and name.endswith(('.rs','Cargo.toml')) and not name.startswith('crates/airk-cli/admin-assets/')]
 preserved=metadata['preserved_extra_source']
 if any(name not in preserved or hashlib.sha256((root/name).read_bytes()).hexdigest()!=preserved[name] for name in extras):raise ValueError('Unreviewed extra remote native source must be preserved/reviewed before sync: '+repr(extras))
 content={}
 for name,row in files.items():
  relative=PurePosixPath(name)
  if relative.is_absolute() or any(p in ('..','artifacts','temp','.airk','.git','node_modules') for p in relative.parts):raise ValueError('Unsafe source path')
  target=root/name
  for part in (target,*target.parents):
   if part.is_symlink():raise ValueError('Remote source alias: '+str(part))
   if part==root:break
  member=stream.getmember(name)
  if not member.isfile():raise ValueError('Nonregular sync file')
  value=stream.extractfile(member).read()
  if len(value)!=row['bytes'] or hashlib.sha256(value).hexdigest()!=row['sha256']:raise ValueError('Source payload differs')
  content[name]=(value,member.mode)
 # Retain every displaced original before changing any public source.
 recovery=[]
 for name,(value,mode) in content.items():
  target=root/name
  if target.exists():
   info=target.stat()
   if not stat.S_ISREG(info.st_mode) or info.st_nlink!=1:raise ValueError('Unaliased regular source required')
   original=target.read_bytes()
   if original==value:continue
   if name.startswith(('scripts/provision-static-','scripts/static-onnx-','scripts/static-cmake-','scripts/darwin-waitid')):raise ValueError('Divergent remote tool producer preserved; explicit separate review required: '+name)
   backup=stage/'original'/name;backup.parent.mkdir(parents=True,exist_ok=True)
   with backup.open('xb') as output:output.write(original)
   recovery.append({'path':name,'previous_sha256':hashlib.sha256(original).hexdigest(),'previous_mode':stat.S_IMODE(info.st_mode),'backup':str(backup.relative_to(stage))})
  else:recovery.append({'path':name,'previous_sha256':None,'backup':None})
 with (stage/'recovery.json').open('x') as output:json.dump({'seal':metadata['seal'],'displaced':recovery},output,indent=2)
 for row in recovery:
  name=row['path'];target=root/name;value,mode=content[name];target.parent.mkdir(parents=True,exist_ok=True)
  if row['previous_sha256'] is not None and hashlib.sha256(target.read_bytes()).hexdigest()!=row['previous_sha256']:raise ValueError('Concurrent remote source change')
  if row['previous_sha256'] is None and target.exists():raise ValueError('Concurrent remote source creation')
  with tempfile.NamedTemporaryFile(dir=target.parent,prefix='.airk-source-sync-',delete=False) as output:output.write(value);temporary=Path(output.name)
  temporary.chmod(mode);os.replace(temporary,target)
 for name,row in files.items():
  if hashlib.sha256((root/name).read_bytes()).hexdigest()!=row['sha256']:raise ValueError('Final source seal differs')
 result={'seal':metadata['seal'],'verified_files':len(files),'displaced':len(recovery),'originals_retained':str(stage/'recovery.json'),'scope':'public source only; no Cargo/model/private input/checkout reset'}
 with (stage/'complete.json').open('x') as output:json.dump(result,output,indent=2)
 print(json.dumps(result))
PY
'''.replace('__STAGE__',stage).replace('__SHA__',hashlib.sha256(payload.getvalue()).hexdigest())
r=subprocess.run(ssh+['sh','-s'],input=remote.encode(),capture_output=True,timeout=90)
Path('artifacts/advanced-split/receipts/mac-platform-sync-'+actual[:16]+'.log').write_bytes(r.stdout+r.stderr)
print(r.stdout.decode());print(r.stderr.decode());raise SystemExit(r.returncode)
