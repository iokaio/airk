# SPDX-License-Identifier: Apache-2.0
from pathlib import Path,PurePosixPath
import base64,hashlib,importlib.util,io,json,os,stat,subprocess,sys,tarfile,time
assert os.getuid()==os.geteuid()==501==os.stat('/dev/console').st_uid and os.getuid()!=0
os.umask(0o077)
OLD=Path('/Users/tylerjensen/airk-builds/arbiter-final-20261116-r1');BASE=Path('/Users/tylerjensen/airk-builds/arbiter-final-control-alias-e306258a-r1');SOURCE=BASE/'source';EXPECTED={'base-source-manifest.json': {'bytes': 925557, 'sha256': 'efe5dfe46fdfae80c8ea02b31a6a5b1ac7c131b1850928d7ce1dc1543c60ad09'}, 'source-overlay-manifest.json': {'bytes': 732897, 'sha256': '7d32786855f41e63d99a48d30dddc6d75d9859e9b9f71cc1f1dbe10a8ea94ea0'}, 'source-delta.tar': {'bytes': 532480, 'sha256': '1f6a01304c719739bd869ac551454b028fa20d8c22f23d4d5a5dd6544fd98c79'}, 'consumed-groups.json': {'bytes': 288457, 'sha256': '36adfc322a9b69d5fa76563909d2edfa6421e2fb42e1d669ba51066990d24781'}}
def sha(p):
 with Path(p).open('rb') as f:return hashlib.file_digest(f,'sha256').hexdigest()
def actor():assert os.getuid()==os.geteuid()==501==os.stat('/dev/console').st_uid

def directory(p):
 st=p.lstat();assert stat.S_ISDIR(st.st_mode) and st.st_uid==501 and p.resolve()==p and not st.st_mode&0o022

def regular(p):
 st=p.lstat();assert stat.S_ISREG(st.st_mode) and st.st_uid==501 and st.st_nlink==1 and p.resolve()==p
 return st

def git(root,*a):return subprocess.check_output(['git','--no-optional-locks','-C',str(root),*a],stderr=subprocess.PIPE)
def load(name,p):
 spec=importlib.util.spec_from_file_location(name,p);m=importlib.util.module_from_spec(spec);spec.loader.exec_module(m);return m

def fresh(p,v):
 with p.open('x') as f:json.dump(v,f,indent=2);f.write('\n');f.flush();os.fsync(f.fileno())
raw=sys.stdin.buffer.read(8*1024*1024+1);assert len(raw)<=8*1024*1024
wire=json.loads(raw);assert set(wire)==set(EXPECTED)
data={k:base64.b64decode(v,validate=True) for k,v in wire.items()}
for k,v in data.items():assert len(v)==EXPECTED[k]['bytes'] and hashlib.sha256(v).hexdigest()==EXPECTED[k]['sha256']
baseline=json.loads(data['base-source-manifest.json']);final=json.loads(data['source-overlay-manifest.json']);groups=json.loads(data['consumed-groups.json'])
assert baseline['commit']==final['baseline_commit']=='059a2a893069f89e8ea8aefec7f9bdb8bb6063b6'
assert final['external_delta_commit']=='e306258a2b6913e6ba91a20fe0b50dbe6ae8361f'
assert len(baseline['files'])==3511 and len(final['files'])==3516 and len(final['overlays'])==20
assert git(OLD/'source','rev-parse','HEAD').decode().strip()==baseline['commit']
assert git(OLD/'source','rev-parse','HEAD^{tree}').decode().strip()==baseline['normalized_tree']
assert not git(OLD/'source','status','--porcelain=v1','--untracked-files=all')
for p in [OLD,OLD/'source',BASE.parent]:directory(p)
assert not os.path.lexists(BASE)
original_dirty=hashlib.sha256(git(Path.home()/'code/airk','status','--porcelain=v1','-z','--untracked-files=normal')).hexdigest()
for row in baseline['files']:
 name=row['path'];assert not name.startswith('docs/arbiter/analysis/') and not Path(name).is_absolute() and '..' not in Path(name).parts and '\\' not in name
 p=OLD/'source'/name;st=regular(p);assert st.st_size==row['bytes'] and sha(p)==row['sha256']
actor();BASE.mkdir(mode=0o700);directory(BASE);(BASE/'evidence').mkdir(mode=0o700);(BASE/'transfer').mkdir(mode=0o700)
for k,v in data.items():
 p=BASE/'transfer'/k
 with p.open('xb') as f:f.write(v);f.flush();os.fsync(f.fileno())
 regular(p);assert sha(p)==EXPECTED[k]['sha256']
started=time.monotonic();clone=['git','--no-optional-locks','clone','--no-hardlinks','--no-checkout',str(OLD/'source'),str(SOURCE)]
r=subprocess.run(clone,capture_output=True,timeout=300);fresh(BASE/'evidence/source-clone.json',{'argv':clone,'code':r.returncode,'stdout':r.stdout.decode(errors='replace'),'stderr':r.stderr.decode(errors='replace'),'elapsed':time.monotonic()-started});assert r.returncode==0
# Explicit genuine revision selection handles absent/stale advertised default branches.
checkout=['git','--no-optional-locks','-C',str(SOURCE),'checkout','--detach',baseline['commit']]
r=subprocess.run(checkout,capture_output=True,timeout=300);fresh(BASE/'evidence/source-checkout.json',{'argv':checkout,'code':r.returncode,'stdout':r.stdout.decode(errors='replace'),'stderr':r.stderr.decode(errors='replace')});assert r.returncode==0
for row in baseline['files']:
 old=OLD/'source'/row['path'];p=SOURCE/row['path'];regular(old);regular(p);v=old.read_bytes();assert hashlib.sha256(v).hexdigest()==row['sha256']
 with p.open('wb') as f:f.write(v)
 os.chmod(p,int(row['mode'],8)&0o777)
git(SOURCE,'add','--renormalize','--all');assert git(SOURCE,'write-tree').decode().strip()==baseline['normalized_tree'];assert not git(SOURCE,'status','--porcelain=v1','--untracked-files=all')
assert load('base_builder',SOURCE/'scripts/build-service-runtime.py').source_identity()==baseline['raw_c7']
rows={r['path']:r for r in final['overlays']}
with tarfile.open(fileobj=io.BytesIO(data['source-delta.tar']),mode='r:') as t:
 members=t.getmembers();assert len(members)==20 and {m.name for m in members}==set(rows)
 contents={}
 for m in members:
  name=m.name;assert m.isfile() and not PurePosixPath(name).is_absolute() and '..' not in PurePosixPath(name).parts and '\\' not in name and not name.startswith('docs/arbiter/analysis/')
  v=t.extractfile(m).read();row=rows[name];assert len(v)==row['bytes'] and hashlib.sha256(v).hexdigest()==row['after_sha256'];p=SOURCE/name
  if row['before_sha256'] is None:assert not os.path.lexists(p)
  else:regular(p);assert sha(p)==row['before_sha256']
  contents[name]=v
 # Complete member+before-SHA preflight precedes every overlay write.
 for name,v in contents.items():
  row=rows[name];p=SOURCE/name;p.parent.mkdir(parents=True,mode=0o700,exist_ok=True)
  with p.open('xb' if row['before_sha256'] is None else 'wb') as f:f.write(v);f.flush();os.fsync(f.fileno())
  os.chmod(p,int(row['mode'],8)&0o777);regular(p);assert sha(p)==row['after_sha256']
for row in final['files']:
 p=SOURCE/row['path'];st=regular(p);assert st.st_size==row['bytes'] and sha(p)==row['sha256']
raw_status=git(SOURCE,'status','--porcelain=v1','-z','--untracked-files=all');assert hashlib.sha256(raw_status).hexdigest()==final['dirty_status_sha256'] and raw_status.hex()==final['dirty_status_hex']
assert git(SOURCE,'rev-parse','HEAD').decode().strip()==baseline['commit'] and git(SOURCE,'rev-parse','HEAD^{tree}').decode().strip()==baseline['normalized_tree']
actual_c7=load('overlay_builder',SOURCE/'scripts/build-service-runtime.py').source_identity();assert actual_c7==final['raw_c7']
actual_groups=load('actual_receipts',SOURCE/'scripts/dev/receipt_runner.py').observe(['native','harness','fixtures'],SOURCE);assert actual_groups==groups
assert not git(OLD/'source','status','--porcelain=v1','--untracked-files=all')
for row in baseline['files']:assert sha(OLD/'source'/row['path'])==row['sha256']
assert hashlib.sha256(git(Path.home()/'code/airk','status','--porcelain=v1','-z','--untracked-files=normal')).hexdigest()==original_dirty
actor();result={'passed':True,'actor':{'uid':os.getuid(),'effective':os.geteuid(),'console':os.stat('/dev/console').st_uid},'baseline_commit':baseline['commit'],'external_delta_commit':final['external_delta_commit'],'exact_overlay_count':20,'source_files':3516,'source_manifest_sha256':EXPECTED['source-overlay-manifest.json']['sha256'],'raw_c7':actual_c7,'groups':{k:{'sha256':v['sha256'],'files':len(v['files'])} for k,v in actual_groups.items()},'original_dirty_status_sha256':original_dirty,'old_source_and_dirty_checkout_unchanged':True,'Cargo_or_native_or_model_run':False}
fresh(BASE/'evidence/source-transfer-proof.json',result);print(json.dumps(result),flush=True)
