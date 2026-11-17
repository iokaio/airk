# SPDX-License-Identifier: Apache-2.0
set -eu
umask 077
/Users/tylerjensen/Code/airk/artifacts/tools/neo-python-3.13.18/bin/python3.13 -I -B - <<'PY'
import hashlib,importlib.util,json,os,stat,subprocess,tarfile,time
from pathlib import Path,PurePosixPath
original=Path('/Users/tylerjensen/Code/airk');transfer=Path('/Users/tylerjensen/airk-builds/advanced-split-local-delivery-source-transfer-r1');delivery=Path('/Users/tylerjensen/airk-builds/advanced-split-local-delivery-r1')
assert os.geteuid()==501 and not delivery.exists() and not delivery.is_symlink()
env=dict(os.environ,GIT_OPTIONAL_LOCKS='0')
def run(*args,cwd=None):return subprocess.check_output(list(args),cwd=cwd,env=env,text=True).strip()
def sha(path):
 with path.open('rb') as stream:return hashlib.file_digest(stream,'sha256').hexdigest()
raw=(transfer/'source.json').read_bytes();assert hashlib.sha256(raw).hexdigest()=='04f933fe0651c4a668da2f48b814c6e7db22cf86aa9a1d3b2f96b9d2a46c4fe1'
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
 'head':packet['head'],'manifest_sha256':'04f933fe0651c4a668da2f48b814c6e7db22cf86aa9a1d3b2f96b9d2a46c4fe1','files':len(packet['files']),
 'source_root':str(root),'original_head_index_status_unchanged':True,'original_status_sha256':hashlib.sha256(before['status'].encode()).hexdigest(),
 'original_index_sha256':before['index'],'no_cargo_build_sign_install_or_service_change':True}
(transfer/'source-copy-result.json').write_text(json.dumps(receipt,indent=2)+'\n')
print(json.dumps(receipt))
PY
