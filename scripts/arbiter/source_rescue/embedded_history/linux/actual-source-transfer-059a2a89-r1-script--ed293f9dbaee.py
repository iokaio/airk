# SPDX-License-Identifier: Apache-2.0
import os,pwd,pathlib,json,hashlib,subprocess,tarfile,posixpath,stat,importlib.util,time
assert os.geteuid()==1000 and pwd.getpwuid(1000).pw_name=='tyler';os.umask(0o077)
B=pathlib.Path('/home/tyler/airk-builds/arbiter-final-linux-r1');assert B.resolve()==B and B.stat().st_uid==1000
W=pathlib.Path('/mnt/d/code/Github/airk/artifacts/v190/final-plan/exec-20261116T052942Z/linux-source-059a2a89-r1');T=B/'source-transfer';S=B/'source';assert not S.exists() and not list(T.iterdir())
def sha(p):
 with p.open('rb') as f:return hashlib.file_digest(f,'sha256').hexdigest()
expected={'source.bundle':'c35027797f4e8199af39e0e9334940e87e59dbc770dbfd2262c3da5b785a44fd','raw-source.tar':'addf8a30528b4444a0522af4fd3b8278d7aaecf9b52d2a222c58835552ff1a1e','source-manifest.json':'efe5dfe46fdfae80c8ea02b31a6a5b1ac7c131b1850928d7ce1dc1543c60ad09'}
r={'schema':'airk.private.linux-source-transfer/1','passed':False,'uid':os.getuid(),'source_root':str(S),'copied':[],'commands':[]};out=B/'evidence/source-transfer-059a2a89-r1.json';began=time.monotonic()
def save():out.write_text(json.dumps(r,indent=2)+'\n')
def run(argv):
 p=subprocess.run(argv,cwd=B,capture_output=True,text=True,timeout=90);r['commands'].append({'argv':argv,'exit_code':p.returncode,'stdout':p.stdout,'stderr':p.stderr});save();assert p.returncode==0,r['commands'][-1];return p.stdout.strip()
try:
 for name,digest in expected.items():
  original=W/name;target=T/name;h=hashlib.sha256()
  with original.open('rb') as src,target.open('xb') as dst:
   while True:
    data=src.read(1024*1024)
    if not data:break
    dst.write(data);h.update(data)
  assert h.hexdigest()==digest and sha(target)==digest
  r['copied'].append({'path':str(target),'sha256':digest,'bytes':target.stat().st_size})
 manifest=json.loads((T/'source-manifest.json').read_text());assert manifest['commit']=='059a2a893069f89e8ea8aefec7f9bdb8bb6063b6';files={x['path']:x for x in manifest['files']};assert len(files)==3511
 run(['/usr/bin/git','bundle','verify',str(T/'source.bundle')])
 run(['/usr/bin/git','clone','--no-hardlinks','--branch','feat/arbiter-final',str(T/'source.bundle'),str(S)])
 with tarfile.open(T/'raw-source.tar') as tar:
  members=tar.getmembers();assert len(members)==len(files) and {m.name for m in members}==set(files)
  for m in members:
   assert m.isfile() and not m.issym() and not m.islnk() and not m.name.startswith('/') and '..' not in pathlib.PurePosixPath(m.name).parts
   target=S/m.name;assert target.resolve(strict=False).is_relative_to(S)
   if target.exists():assert target.is_file() and not target.is_symlink() and target.lstat().st_uid==1000
   for parent in target.parents:
    if parent==S:break
    assert not parent.is_symlink() and parent.resolve(strict=False).is_relative_to(S)
  tar.extractall(S,filter='data')
 for name,row in files.items():
  p=S/name;assert p.lstat().st_uid==1000 and p.stat().st_size==row['bytes'] and sha(p)==row['sha256'],name
  assert bool(p.stat().st_mode&0o111)==(row['mode']=='100755'),name
 head=run(['/usr/bin/git','-C',str(S),'rev-parse','HEAD']);tree=run(['/usr/bin/git','-C',str(S),'rev-parse','HEAD^{tree}']);status=run(['/usr/bin/git','-C',str(S),'status','--porcelain','--untracked-files=all']);assert head==manifest['commit'] and tree==manifest['normalized_tree'] and not status
 spec=importlib.util.spec_from_file_location('linux_frozen_runtime_builder',S/'scripts/build-service-runtime.py');m=importlib.util.module_from_spec(spec);spec.loader.exec_module(m);c7=m.source_identity();assert c7==manifest['raw_c7']=={'sha256':'06c740ee904ba9ccc3aec6a8359bff2b6e1f76bd061aa26e41e3c55a2bfd910e','files':1944}
 r.update(head=head,tree=tree,raw_c7=c7,public_file_count=len(files),git_clean=True,all_source_raw_hashes_and_modes_match=True,passed=True)
except BaseException as e:r['failure']={'type':type(e).__name__,'message':str(e)};raise
finally:r['seconds']=time.monotonic()-began;save();print(json.dumps(r));print('RECEIPT_SHA256',sha(out))
