# SPDX-License-Identifier: Apache-2.0
# Prepared only. Root must review exact packet before executing; this preparation never runs Cargo.
from pathlib import Path
import os,sys,json,time,hashlib,stat,subprocess,importlib.util,resource,shutil,re
from datetime import datetime,timezone
C=None

def validate_config(value):
 assert isinstance(value,dict) and value.get('status')=='frozen_declared_overlay'
 for field,size in (('commit',40),('tree',40),('manifest_sha256',64),('static_sha256',64)):
  item=value.get(field);assert isinstance(item,str) and len(item)==size and all(c in '0123456789abcdef' for c in item)
 assert value['commit']=='059a2a893069f89e8ea8aefec7f9bdb8bb6063b6' and value['external_delta_commit']=='e306258a2b6913e6ba91a20fe0b50dbe6ae8361f'
 assert value['tree']=='92f38579cbd9a4650466cc839e34bde1b8574973'
 assert value['expected_dirty_paths'] and len(value['expected_dirty_paths'])==len(set(value['expected_dirty_paths']))
 assert re.fullmatch('[0-9a-f]{64}',value['expected_dirty_status_sha256'])
 seal=value.get('raw_c7');assert isinstance(seal,dict) and isinstance(seal.get('files'),int) and seal['files']>0
 digest=seal.get('sha256');assert isinstance(digest,str) and len(digest)==64 and all(c in '0123456789abcdef' for c in digest)
 assert [x['name'] for x in value.get('commands',[])]==['scoped-clippy', 'mac-system-control-anchor', 'affected-control-regressions', 'private-log-controls', 'exact-reference-refresh']
 return value

def read_config():
 actor();assert len(sys.argv)==2, 'one frozen Root config path required'
 path=Path(sys.argv[1]);metadata=path.lstat()
 assert path.is_absolute() and not path.is_symlink() and stat.S_ISREG(metadata.st_mode) and metadata.st_uid==501 and metadata.st_nlink==1 and metadata.st_size<=65536
 with path.open('rb') as source:raw=source.read(65537)
 assert len(raw)<=65536
 return validate_config(json.loads(raw))

def actor():
 assert os.getuid()==501==os.geteuid()==os.stat('/dev/console').st_uid and os.getuid()!=0
 return {'uid':os.getuid(),'console_uid':os.stat('/dev/console').st_uid}
def sha(q):
 with Path(q).open('rb') as f:return hashlib.file_digest(f,'sha256').hexdigest()
def load(name,path):
 s=importlib.util.spec_from_file_location(name,path);m=importlib.util.module_from_spec(s);s.loader.exec_module(m);return m
def fresh(q,value):
 actor()
 with q.open('x') as f:json.dump(value,f,indent=2);f.write('\n');f.flush();os.fsync(f.fileno())
def git(*args):return subprocess.check_output(['git','--no-optional-locks',*args],cwd=C['source'])
def require_exact_overlay_status():
 raw=git('status','--porcelain=v1','-z','--untracked-files=all')
 entries=raw.split(b'\0');assert entries.pop()==b''
 paths=[]
 for entry in entries:
  assert len(entry)>=4 and entry[2:3]==b' ' and entry[:2] in (b' M',b'??'), 'unexpected index/rename/deletion state'
  paths.append(os.fsdecode(entry[3:]))
 assert sorted(paths)==sorted(C['expected_dirty_paths']), 'undeclared or missing dirty source path'
 assert hashlib.sha256(raw).hexdigest()==C['expected_dirty_status_sha256'], 'exact overlay census bytes changed'
 return True

def preflight():
 actor();source=Path(C['source']);base=Path(C['base'])
 for q in (base,source,base/'evidence'):
  assert q.lstat().st_uid==501 and q.resolve()==q and q.is_dir()
 assert git('rev-parse','HEAD').decode().strip()==C['commit']
 assert git('rev-parse','HEAD^{tree}').decode().strip()==C['tree']
 require_exact_overlay_status()
 assert sha(C['manifest'])==C['manifest_sha256']
 manifest=json.loads(Path(C['manifest']).read_text())
 for row in manifest['files']:
  name=row['path'];assert not Path(name).is_absolute() and '..' not in Path(name).parts and '\\' not in name
  q=source/name;s=q.lstat();assert stat.S_ISREG(s.st_mode) and s.st_uid==501 and s.st_nlink==1 and q.resolve()==q and sha(q)==row['sha256']
 assert load('actual_native_identity',source/'scripts/build-service-runtime.py').source_identity()==C['raw_c7']
 assert sha(C['static_environment'])==C['static_sha256']
 return source,base

def capture_final_refresh(out):
 actor();base=Path(C['base']);log=out/'receipts'/'neo-native-name-exact-reference-refresh-r1.log'
 metadata=log.lstat();assert stat.S_ISREG(metadata.st_mode) and metadata.st_uid==501 and metadata.st_nlink==1 and metadata.st_size<=8*1024*1024 and not log.is_symlink()
 names=re.findall(r'^failed refresh fixture retained: (.+?)(?:; secondary diagnostic capture failed:.*)?$',log.read_text(encoding='utf-8'),re.MULTILINE)
 assert len(names)<=1
 if not names:
  fresh(out/'refresh-final-capture.json',{'found':False,'scope':'No retained fixture path emitted; no raw cause or cleanup claim fabricated.'});return
 fixture=Path(names[0]);allowed=out/'fixtures/exact-reference-refresh'
 assert fixture.parent==allowed and re.fullmatch(r'airk-combined-[A-Za-z0-9_.-]{1,110}',fixture.name)
 def ancestry(path):
  assert path.is_relative_to(base)
  for index in range(len(path.relative_to(base).parts)+1):
   q=base.joinpath(*path.relative_to(base).parts[:index]);st=q.lstat();assert stat.S_ISDIR(st.st_mode) and st.st_uid==501 and q.resolve()==q and not q.is_symlink()
 ancestry(fixture);paths=[];metadata_only=[]
 for name,cap in [('reference-refresh-failure/observations.json',256<<10),('reference-refresh-failure/raw-export.json',64<<10),('reference-refresh-failure/raw-errors.jsonl',16<<20),('service.log',1<<20),('resources.json',64<<10),('control/job-errors-1.owner.json',4096)]:paths.append((fixture/name,cap))
 ring=fixture/'control/job-errors-1'
 if os.path.lexists(ring):
  ancestry(ring)
  for q in sorted(ring.iterdir()):
   st=q.lstat();assert stat.S_ISREG(st.st_mode) and st.st_uid==501 and st.st_nlink==1 and not q.is_symlink()
   if q.name=='writer.lock':
    assert st.st_size==0;metadata_only.append({'path':str(q),'bytes':0,'uid':501,'inode':st.st_ino,'role':'known writer lock; metadata only'});continue
   assert q.name=='current.jsonl' or re.fullmatch(r'previous-(?:[1-9]|1[0-9]|20)\.jsonl',q.name), 'unknown raw ring entry'
   paths.append((q,10_000_000))
 selected=[]
 for q,cap in paths:
  if not os.path.lexists(q):selected.append((q,cap,None));continue
  ancestry(q.parent);st=q.lstat();assert stat.S_ISREG(st.st_mode) and st.st_uid==501 and st.st_nlink==1 and st.st_size<=cap and not q.is_symlink();selected.append((q,cap,st))
 assert sum(st.st_size for q,cap,st in selected if st is not None)<=32<<20, 'complete physical capture exceeds32MiB; no truncation'
 destination=out/'retained-reference-refresh-final';assert not destination.exists();destination.mkdir(mode=0o700);entries=[]
 for q,cap,before in selected:
  relative=q.relative_to(fixture)
  if before is None:entries.append({'path':str(relative),'exists':False});continue
  fd=os.open(q,os.O_RDONLY|os.O_NOFOLLOW)
  try:
   opened=os.fstat(fd);assert (opened.st_dev,opened.st_ino,opened.st_uid,opened.st_nlink,opened.st_size)==(before.st_dev,before.st_ino,501,1,before.st_size)
   raw=bytearray()
   while True:
    block=os.read(fd,min(65536,cap+1-len(raw)))
    if not block:break
    raw.extend(block);assert len(raw)<=cap
   after=q.lstat();assert len(raw)==before.st_size and (after.st_dev,after.st_ino,after.st_uid,after.st_nlink,after.st_size,after.st_mode,after.st_mtime_ns,after.st_ctime_ns)==(before.st_dev,before.st_ino,501,1,before.st_size,before.st_mode,before.st_mtime_ns,before.st_ctime_ns)
  finally:os.close(fd)
  target=destination/relative;target.parent.mkdir(mode=0o700,parents=True,exist_ok=True)
  with target.open('xb') as stream:stream.write(raw);stream.flush();os.fsync(stream.fileno())
  assert sha(target)==hashlib.sha256(raw).hexdigest() and target.lstat().st_uid==501
  entries.append({'path':str(relative),'exists':True,'bytes':len(raw),'sha256':sha(target),'uid':501,'inode':before.st_ino,'metadata_unchanged':True,'atime_unchanged':before.st_atime_ns==after.st_atime_ns})
 fresh(out/'refresh-final-capture.json',{'found':True,'fixture':str(fixture),'destination':str(destination),'entries':entries,'metadata_only':metadata_only,'scope':'Complete original live snapshot plus final physical private ring after actual owned descendants settled. No source/DB/corpus reads, writer flush/control/retry/signal, gracefulStop or service HANDLE claim.'})


def main():
 global C
 C=read_config()
 source,base=preflight();out=Path(C['out']);target=Path(C['target'])
 assert not out.exists() and not target.exists();os.umask(0o077);out.mkdir(mode=0o700);(out/'logs').mkdir();(out/'receipts').mkdir();(out/'fixtures').mkdir()
 soft,hard=resource.getrlimit(resource.RLIMIT_NOFILE);assert hard==resource.RLIM_INFINITY or hard>=4096
 resource.setrlimit(resource.RLIMIT_NOFILE,(max(soft,4096),hard))
 owner=load('actual_darwin_owner',source/'scripts/provision-static-onnx-runtime.py')
 env=dict(os.environ,PYTHONDONTWRITEBYTECODE='1',PYTHONNOUSERSITE='1',CARGO_BUILD_JOBS='2',CARGO_INCREMENTAL='0',CARGO_TARGET_DIR=str(target),AIRK_BUNDLED_MODEL_DIR='/Users/tylerjensen/airk-builds/arbiter-final-20261116-r1/inputs/bundled-model-v3',AIRK_TEST_MODEL_DIR='/Users/tylerjensen/airk-builds/arbiter-final-20261116-r1/inputs/bundled-model-v3/minilm-l6-v2-s8-v3',RUST_TEST_THREADS='2')
 env['PATH']=str(Path(C['python']).parent)+':'+str(Path(C['node']).parent)+':'+str(Path.home()/'.cargo/bin')+':/usr/bin:/bin:/usr/sbin:/sbin'
 start=time.monotonic();records=[];cleanup=None;all_settled=True;error=None;passed=False
 initial={'schema':'airk.neo-integrated-platform-start/1','utc':datetime.now(timezone.utc).isoformat(),'actor':actor(),'config':C,'status':'started','passed':False,'descriptor_before':[soft,hard],'descriptor_after':list(resource.getrlimit(resource.RLIMIT_NOFILE)),'owner_script_sha256':sha(source/'scripts/provision-static-onnx-runtime.py'),'receipt_runner_sha256':sha(source/'scripts/dev/receipt_runner.py'),'original_dirty_status_sha256':hashlib.sha256(subprocess.check_output(['git','--no-optional-locks','status','--porcelain=v1','-z','--untracked-files=normal'],cwd=Path.home()/'code/airk')).hexdigest()}
 fresh(out/'batch-start.json',initial)
 try:
  for command in C['commands']:
   actor();name=command['name'];fixture=out/'fixtures'/name;fixture.mkdir(mode=0o700)
   fresh(out/(name+'-outer-start.json'),{'utc':datetime.now(timezone.utc).isoformat(),'actor':actor(),'argv':command['argv'],'deadline_seconds':10800,'status':'started','passed':False})
   all_settled=False  # Unknown until THIS launch returns its actual ownership witness.
   result=owner.run_owned(command['argv'],source,out/'logs'/(name+'.log'),10800,env)
   all_settled=result.get('owned_descendants_settled') is True
   natural=result.get('owned_descendants_settled') is True and not result.get('owned_group_signaled',False)
   ok=owner.successful(result) and natural
   record={'phase':name,'result':result,'natural_unix_settlement':natural,'passed':ok};records.append(record);fresh(out/(name+'-outer-result.json'),record)
   print(json.dumps({'phase':name,'passed':ok,'result':result}),flush=True)
   if not ok:
    if name=='exact-reference-refresh' and natural:
     try:capture_final_refresh(out)
     except BaseException as failure:
      fresh(out/'refresh-final-capture-error.json',{'type':type(failure).__name__,'message':str(failure),'primary_phase_failed':name,'scope':'Secondary evidence failure; original native test remains failed and retained.'})
    raise RuntimeError('failed '+name+'; remaining phases stopped, no replay')
  passed=True
 except BaseException as failure:
  error={'type':type(failure).__name__,'message':str(failure)};passed=False
 finally:
  actor();evidence_errors=[];cleanup_blocker=None
  # Preserve evidence failures without replacing the original phase outcome.
  # The exact compiler witness controls cleanup, independently of copy success.
  timing=target/'cargo-timings'
  if all_settled:
   try:
    if timing.is_dir():shutil.copytree(timing,out/'retained-cargo-timings',symlinks=False)
   except BaseException as failure:
    evidence_errors.append({'stage':'timing_retention','type':type(failure).__name__,'message':str(failure)});passed=False
   try:
    assert target==source/'target' and target.resolve()==target and target.parent.resolve()==source.resolve()
    if target.exists():assert target.is_dir() and not target.is_symlink() and target.lstat().st_uid==501
    cleanup=owner.run_owned(C['cleanup'],source,out/'logs/cargo-clean.log',600,env)
    clean_ok=owner.successful(cleanup) and cleanup.get('owned_descendants_settled') is True and not cleanup.get('owned_group_signaled',False)
    fresh(out/'cargo-clean-result.json',{'result':cleanup,'passed':clean_ok,'target':str(target),'target_exists_after':target.exists()})
    passed=passed and clean_ok
   except BaseException as failure:
    evidence_errors.append({'stage':'target_cleanup','type':type(failure).__name__,'message':str(failure)});passed=False
  else:
   cleanup_blocker='Actual owned descendant settlement unproven; target/evidence retained without cleanup claim.';passed=False
  current=None;dirty=None;unchanged=False
  try:
   preflight()  # Full final source-map, baseline HEAD/tree and exact dirty census reopen.
   current=load('after_native_identity',source/'scripts/build-service-runtime.py').source_identity()
   dirty=hashlib.sha256(subprocess.check_output(['git','--no-optional-locks','status','--porcelain=v1','-z','--untracked-files=normal'],cwd=Path.home()/'code/airk')).hexdigest()
   unchanged=current==C['raw_c7'] and require_exact_overlay_status() and dirty==initial['original_dirty_status_sha256']
  except BaseException as failure:
   evidence_errors.append({'stage':'final_input_observation','type':type(failure).__name__,'message':str(failure)});passed=False
  passed=passed and unchanged and not evidence_errors
  fresh(out/'batch-result.json',{'schema':'airk.neo-integrated-platform-result/1','utc':datetime.now(timezone.utc).isoformat(),'actor':actor(),'passed':passed,'elapsed_seconds':time.monotonic()-start,'phases':records,'cleanup':cleanup,'cleanup_blocker':cleanup_blocker,'error':error,'evidence_errors':evidence_errors,'inputs_unchanged':bool(unchanged),'raw_c7_after':current,'original_dirty_status_sha256':dirty,'scope':'Actual Darwin compiled component gates only. No larger-model fit, signing, installed journey or Windows observer qualification.'})
 if not passed:raise SystemExit(1)

if __name__=='__main__':main()
