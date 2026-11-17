# SPDX-License-Identifier: Apache-2.0
import os,pwd,pathlib,json,hashlib,subprocess,tarfile,time,stat
assert os.getuid()==os.geteuid()==os.getgid()==os.getegid()==1000 and pwd.getpwuid(1000).pw_name=='tyler';os.umask(0o077)
P=pathlib.Path('/mnt/d/code/Github/airk/artifacts/v190/final-plan/exec-20261116T052942Z/linux-source-driver-fix-proposal-r2.json');proposal=json.loads(P.read_text())
B=pathlib.Path('/home/tyler/airk-builds/arbiter-final-linux-r1');S=B/'source-driver-fix-r1';T=B/'source-transfer-driver-fix-r1';W=pathlib.Path('/mnt/d/code/Github/airk/artifacts/v190/final-plan/exec-20261116T052942Z/linux-source-driver-fix-r1')
deadline=time.monotonic()+600;record={'schema':'airk.private.linux-source-transfer/1','passed':False,'git_clean':False,'uid':1000,'source_root':str(S),'commands':[]}
def check():
 if time.monotonic()>=deadline:raise TimeoutError('original source-transfer deadline')
def owned(p,directory=False):
 s=p.lstat();assert s.st_uid==s.st_gid==1000 and not p.is_symlink() and p.resolve()==p
 if directory:assert p.is_dir() and stat.S_IMODE(s.st_mode)==0o700
def sha(p):
 h=hashlib.sha256()
 with p.open('rb') as f:
  while data:=f.read(1<<20):check();h.update(data)
 return h.hexdigest()
def run(argv,allowed=(0,)):
 check();p=subprocess.run(argv,cwd=B,capture_output=True,text=True,timeout=min(90,deadline-time.monotonic()))
 record['commands'].append({'argv':argv,'exit_code':p.returncode,'stdout':p.stdout,'stderr':p.stderr});assert p.returncode in allowed,record['commands'][-1];check();return p.stdout.strip()
for p in (B.parent,B,B/'evidence'):owned(p,True)
assert not S.exists() and not T.exists()
receipt_path=B/'evidence/source-transfer-driver-fix-r1.json';assert not receipt_path.exists()
custody=json.loads((W/'custody.json').read_text());assert custody['source_unchanged'] is True and custody['git_clean'] is False
expected=custody['artifacts'];assert set(expected)=={'source.bundle','raw-source.tar','source-manifest.json','tracked-overlay.patch'} and sum(x['bytes'] for x in expected.values())<=1<<30
T.mkdir(mode=0o700);owned(T,True)
try:
 for name,row in expected.items():
  original=W/name;target=T/name;assert original.is_file() and not original.is_symlink()
  count=0;h=hashlib.sha256()
  with original.open('rb') as src,target.open('xb') as dst:
   while data:=src.read(1<<20):check();count+=len(data);assert count<=row['bytes'];h.update(data);dst.write(data)
  owned(target);assert count==row['bytes'] and h.hexdigest()==row['sha256']==sha(target)
 manifest=json.loads((T/'source-manifest.json').read_text());assert manifest['proposal_sha256']==sha(P) and manifest['base_commit']==proposal['base_commit'] and manifest['native_group']==proposal['native_group'] and manifest['files']==proposal['files']
 files={x['path']:x for x in manifest['files']};assert len(files)==len(manifest['files'])<=5000
 run(['/usr/bin/git','clone','--no-hardlinks','--branch','feat/arbiter-final',str(T/'source.bundle'),str(S)])
 owned(S,True)
 for p in S.rglob('*'):owned(p,p.is_dir())
 run(['/usr/bin/git','-C',str(S),'bundle','verify',str(T/'source.bundle')])
 assert run(['/usr/bin/git','-C',str(S),'rev-parse','HEAD'])==manifest['base_commit']
 assert run(['/usr/bin/git','-C',str(S),'rev-parse','HEAD^{tree}'])==manifest['base_tree']
 with tarfile.open(T/'raw-source.tar') as archive:
  members=archive.getmembers();assert len(members)==len(files) and {m.name for m in members}==set(files)
  for member in members:
   check();assert member.isfile() and not member.issym() and not member.islnk() and not member.name.startswith('/') and '..' not in pathlib.PurePosixPath(member.name).parts and member.size==files[member.name]['bytes']
   target=S/member.name;assert target.resolve(strict=False).is_relative_to(S)
   missing=[];parent=target.parent
   while not parent.exists():missing.append(parent);parent=parent.parent
   owned(parent,True)
   for parent in reversed(missing):parent.mkdir(mode=0o700);owned(parent,True)
   if target.exists():owned(target);assert target.is_file() and target.stat().st_nlink==1
   with archive.extractfile(member) as src,target.open('wb' if target.exists() else 'xb') as dst:
    while data:=src.read(1<<20):check();dst.write(data)
   target.chmod(0o700 if files[member.name]['mode']=='100755' else 0o600);owned(target)
 for name,row in files.items():
  p=S/name;owned(p);assert p.stat().st_size==row['bytes'] and sha(p)==row['sha256'] and bool(p.stat().st_mode&0o111)==(row['mode']=='100755'),name
 for p in S.rglob('*'):owned(p,p.is_dir())
 run(['/usr/bin/git','-C',str(S),'add','--renormalize','--all'])
 changed=set(run(['/usr/bin/git','-C',str(S),'diff','--cached','--name-only','HEAD']).splitlines())
 untracked=set(run(['/usr/bin/git','-C',str(S),'ls-files','--others','--exclude-standard']).splitlines())
 assert changed=={x['path'] for x in manifest['overlay'] if x['tracked']}
 assert untracked=={x['path'] for x in manifest['overlay'] if not x['tracked']}
 status=run(['/usr/bin/git','-C',str(S),'status','--porcelain','--untracked-files=all']);assert status
 overlay_tree=run(['/usr/bin/git','-C',str(S),'write-tree']);assert overlay_tree!=manifest['base_tree']
 for name,digest in proposal['native_files'].items():assert sha(S/name)==digest,name
 for p in S.rglob('*'):owned(p,p.is_dir())
 for name,row in files.items():
  p=S/name;owned(p);assert p.stat().st_size==row['bytes'] and sha(p)==row['sha256'],name
 check()
 record.update(passed=True,base_commit=manifest['base_commit'],base_tree=manifest['base_tree'],normalized_tracked_overlay_tree=overlay_tree,raw_file_census_sha256=manifest['raw_file_census_sha256'],native_group=manifest['native_group'],actual_git_status=status,files_verified=len(files),original_snapshot_untouched=True)
except BaseException as error:
 record['failure']={'type':type(error).__name__,'message':str(error)};raise
finally:
 record['elapsed_seconds']=600-(deadline-time.monotonic())
 if record['elapsed_seconds']>=600:record['passed']=False
 record['timing_scope']='Includes all final input and directory proofs; excludes final receipt serialization/write and owner audit'
 with receipt_path.open('x',encoding='utf-8') as f:json.dump(record,f,indent=2);f.write('\n')
 owned(receipt_path)
print(json.dumps(record))
