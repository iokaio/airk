# SPDX-License-Identifier: Apache-2.0
# Root-only exact-target manual cleanup; no Cargo/tests or cache-tag repair.
import argparse,base64,collections,datetime,hashlib,json,os,re,stat,subprocess,sys,time
from pathlib import Path
BASE=Path('/Users/tylerjensen/airk-builds/arbiter-final-control-alias-e306258a-r1')
SOURCE=BASE/'source';TARGET=SOURCE/'target';EVIDENCE=BASE/'evidence';OLD=EVIDENCE/'platform-system-control-alias-nextest-continuation-r1';OUT=EVIDENCE/'manual-exact-target-cleanup-r1'
UID=501;MAX_ENTRIES=500000;MAX_DEPTH=128
COMPILERS={'cargo','cargo-nextest','rustc','rustdoc','clippy-driver','build-script-build','build-script-main','cc','c++','clang','clang++','gcc','g++','ld','ld64','ar','libtool','cmake','ninja','make','gmake','dsymutil','cc1','cc1plus','lld','ld.lld','llvm-ar','llvm-link','llvm-ranlib','ranlib','as','nvcc','ptxas','nvlink','cicc','cudafe++'}
def actor():
 a={'real_uid':os.getuid(),'effective_uid':os.geteuid(),'console_uid':os.stat('/dev/console').st_uid};assert all(v==UID for v in a.values()) and UID!=0,a;return a
def census(text):
 rows=[];blocked=[]
 for line in text.splitlines():
  p=line.split(None,3)
  if len(p)!=4:raise RuntimeError('unparseable ps row')
  pid,ppid,uid=map(int,p[:3]);comm=p[3];row={'pid':pid,'ppid':ppid,'uid':uid,'comm':comm};rows.append(row)
  name=comm.rsplit('/',1)[-1]
  if uid==UID and (name in COMPILERS or re.fullmatch(r'(?:clang|gcc|g\+\+|clang\+\+)-[0-9.]+',name) or comm==str(TARGET) or comm.startswith(str(TARGET)+'/')):blocked.append(row)
 return {'rows':rows,'blockers':blocked}
def processes():return census(subprocess.check_output(['/bin/ps','-ww','-axo','pid=,ppid=,uid=,comm='],text=True,timeout=20))
def meta(s):return {'dev':s.st_dev,'inode':s.st_ino,'mode':s.st_mode,'uid':s.st_uid,'gid':s.st_gid,'nlink':s.st_nlink,'bytes':s.st_size,'blocks512':s.st_blocks,'mtime_ns':s.st_mtime_ns,'ctime_ns':s.st_ctime_ns}
def admit(m,rootdev):
 if m['uid']!=UID:raise RuntimeError('foreign owner')
 if m['dev']!=rootdev:raise RuntimeError('nested mount/device')
 if not (stat.S_ISREG(m['mode']) or stat.S_ISDIR(m['mode'])):raise RuntimeError('symlink/special entry')
def same(m,n,directory=False):
 keys=['dev','inode','mode','uid','gid'] if directory else ['dev','inode','mode','uid','gid','bytes','mtime_ns']
 assert all(m[k]==n[k] for k in keys),'changed entry preimage'
 if not directory:assert 1<=n['nlink']<=m['nlink'],'link count increased'
def fresh(name,value):
 p=OUT/name
 with p.open('x',encoding='utf-8') as f:json.dump(value,f,indent=2);f.write('\n');f.flush();os.fsync(f.fileno())
def directory(path):
 assert path.resolve(strict=True)==path and not path.is_symlink(),str(path)
 s=path.lstat();assert stat.S_ISDIR(s.st_mode) and s.st_uid==UID,str(path);return s
FLAGS=os.O_RDONLY|getattr(os,'O_DIRECTORY',0)|getattr(os,'O_NOFOLLOW',0)
def listing(fd,relative,rows,rootdev,depth=0):
 assert depth<=MAX_DEPTH
 for name in sorted(os.listdir(fd)):
  assert name not in ['.','..'] and '/'not in name
  rel=relative+'/'+name if relative else name;s=os.stat(name,dir_fd=fd,follow_symlinks=False);m=meta(s);rows.append({'path':rel,**m});assert len(rows)<=MAX_ENTRIES
  try:admit(m,rootdev)
  except RuntimeError as failure:raise RuntimeError(rel+': '+str(failure)) from failure
  if stat.S_ISDIR(s.st_mode):
   child=os.open(name,FLAGS,dir_fd=fd)
   try:same(m,meta(os.fstat(child)),True);listing(child,rel,rows,rootdev,depth+1)
   finally:os.close(child)
def tree(fd,rootdev):
 rows=[];listing(fd,'',rows,rootdev);return rows
def snapshot():
 s=os.statvfs(BASE);return {'available_bytes':s.f_bavail*s.f_frsize,'free_bytes':s.f_bfree*s.f_frsize,'block_size':s.f_frsize}
def tag(fd):
 try:s=os.stat('CACHEDIR.TAG',dir_fd=fd,follow_symlinks=False)
 except FileNotFoundError:return {'exists':False,'repair_attempted':False}
 m=meta(s);result={'exists':True,'metadata':m,'repair_attempted':False}
 if not stat.S_ISREG(s.st_mode) or s.st_uid!=UID or s.st_size>4096:return {**result,'content_not_read':'not ordinary sameUID <=4096B'}
 f=os.open('CACHEDIR.TAG',os.O_RDONLY|os.O_NOFOLLOW,dir_fd=fd)
 try:
  same(m,meta(os.fstat(f)));data=os.read(f,4097);assert len(data)==s.st_size;same(m,meta(os.fstat(f)))
 finally:os.close(f)
 return {**result,'base64':base64.b64encode(data).decode(),'sha256':hashlib.sha256(data).hexdigest(),'valid_signature':data.startswith(b'Signature: 8a477f597d28d172789f06886806bc55')}
def source_status():
 return {str(p):hashlib.sha256(subprocess.check_output(['/usr/bin/git','--no-optional-locks','status','--porcelain=v1','-z','--untracked-files=normal'],cwd=p,timeout=30)).hexdigest() for p in [SOURCE,Path('/Users/tylerjensen/code/airk')]}
def clean():
 start=time.monotonic();actor();assert sys.platform=='darwin';assert os.open in os.supports_dir_fd and os.unlink in os.supports_dir_fd and os.rmdir in os.supports_dir_fd
 for p in [BASE,SOURCE,EVIDENCE]:directory(p)
 assert TARGET==BASE/'source/target' and TARGET.resolve(strict=True)==TARGET and TARGET.parent==SOURCE
 targetmeta=meta(directory(TARGET));assert not OUT.exists();OUT.mkdir(mode=0o700)
 removed=[];sourcefd=None;targetfd=None;success=False;error=None;before_space=snapshot();before_status=None
 try:
  before_status=source_status();oldpins={}
  for name,digest in [('batch-result.json','761b096014eff5c818106ede451f73524210d0acae9288d5e1f49f02a3799772'),('cargo-clean-result.json','2fe3ea8e4d7bc5b8fed91f6fda7224a4bf5612ec0815bd8afd886ab6dfee4f96')]:
   p=OLD/name;directory(p.parent);s=p.lstat();assert stat.S_ISREG(s.st_mode) and s.st_uid==UID and s.st_size<1024*1024;data=p.read_bytes();assert hashlib.sha256(data).hexdigest()==digest;oldpins[name]=digest;v=json.loads(data)
   if name=='batch-result.json':assert v['inputs_unchanged'] and v['error'] is None and all(r['passed'] and r['natural_unix_settlement'] for r in v['phases'])
   else:assert not v['passed'] and v['result']['owned_descendants_settled'] and not v['result']['owned_group_signaled'] and v['result']['exit_code']==101
  observed=processes();fresh('before-processes.json',observed);assert not observed['blockers'],'Cargo/compiler/target image still active'
  sourcefd=os.open(str(SOURCE),FLAGS);targetfd=os.open('target',FLAGS,dir_fd=sourcefd);same(targetmeta,meta(os.fstat(targetfd)),True)
  fresh('before-tag.json',tag(targetfd));fresh('start.json',{'actor':actor(),'target':str(TARGET),'target_preimage':targetmeta,'space':before_space,'source_status':before_status,'preserved_failed_receipts':oldpins,'scope':'Separate authorized manual exact-target cleanup. Original Cargo cleanup and batch remain failed.'})
  rows=[]
  try:listing(targetfd,'',rows,targetmeta['dev'])
  finally:fresh('before-inventory.json',rows)
  fresh('before-storage.json',{'entries':len(rows),'logical_file_bytes':sum(r['bytes'] for r in rows if stat.S_ISREG(r['mode'])),'allocated_blocks512_sum':sum(r['blocks512'] for r in rows),'hardlink_entries':[r['path'] for r in rows if stat.S_ISREG(r['mode']) and r['nlink']>1],'hardlink_policy':'Unlink directory entries only; no file data/metadata edits. External hard links remain intact.','space':before_space})
  actor();again=processes();fresh('predelete-processes.json',again);assert not again['blockers'];assert tree(targetfd,targetmeta['dev'])==rows,'tree changed after inventory'
  bypath={r['path']:r for r in rows};children=collections.defaultdict(list)
  for row in rows:
   parent,name=row['path'].rsplit('/',1) if '/'in row['path'] else ('',row['path']);children[parent].append(name)
  def root_anchor():
   same(targetmeta,meta(os.stat('target',dir_fd=sourcefd,follow_symlinks=False)),True);assert TARGET.resolve(strict=True)==TARGET
  def remove(fd,rel):
   expected=sorted(children[rel])
   assert sorted(os.listdir(fd))==expected,'directory entries changed'
   for name in expected:
    root_anchor();key=rel+'/'+name if rel else name;m=bypath[key];s=os.stat(name,dir_fd=fd,follow_symlinks=False);admit(meta(s),targetmeta['dev']);isdir=stat.S_ISDIR(s.st_mode);same(m,meta(s),isdir)
    if isdir:
     child=os.open(name,FLAGS,dir_fd=fd)
     try:same(m,meta(os.fstat(child)),True);remove(child,key)
     finally:os.close(child)
     root_anchor();same(m,meta(os.stat(name,dir_fd=fd,follow_symlinks=False)),True);os.rmdir(name,dir_fd=fd)
    else:os.unlink(name,dir_fd=fd)
    removed.append(key)
  remove(targetfd,'');assert not os.listdir(targetfd);root_anchor();os.close(targetfd);targetfd=None;os.rmdir('target',dir_fd=sourcefd)
  after_status=source_status();assert after_status==before_status,'outside source status changed';assert not os.path.lexists(TARGET);post_processes=processes();fresh('after-processes.json',post_processes);assert not post_processes['blockers'];fresh('after-inventory.json',{'target_exists':False,'entries':[],'source_status':after_status,'space':snapshot()});success=True
 except BaseException as failure:error={'type':type(failure).__name__,'message':str(failure)}
 finally:
  if targetfd is not None:os.close(targetfd)
  if sourcefd is not None:os.close(sourcefd)
  fresh('removed-entries.json',removed);fresh('result.json',{'passed':success,'actor':actor(),'error':error,'removed_entry_count':len(removed),'target_exists_after':os.path.lexists(TARGET),'elapsed_seconds':time.monotonic()-start,'space_before':before_space,'space_after':snapshot(),'old_batch_reclassified':False,'Cargo_or_tests_executed':False,'no_tag_repair':True});print(json.dumps({'manual_cleanup_passed':success,'evidence':str(OUT),'error':error,'removed_entries':len(removed),'target_exists_after':os.path.lexists(TARGET)}),flush=True)
 if not success:raise SystemExit(1)
def controls():
 tests=[]
 for image,expected in [('/Users/tylerjensen/airk-builds/cache/python3.13',False),('/Users/tylerjensen/airk-builds/source folder/cmake-helper',False),('/opt/homebrew/bin/node',False),('/x/cargo',True),('/x/clang-20',True),(str(TARGET)+'/debug/test image',True)]:
  got=census('12 1 501 '+image)['blockers'];assert bool(got)==expected;tests.append({'comm':image,'passed':True})
 assert not census('12 1 502 /x/rustc')['blockers'];tests.append({'other_uid_preserved':True,'passed':True})
 valid={'uid':501,'dev':7,'mode':stat.S_IFREG|0o600};admit(valid,7)
 for change in [{'uid':0},{'dev':8},{'mode':stat.S_IFLNK|0o777},{'mode':stat.S_IFIFO|0o600}]:
  try:admit({**valid,**change},7)
  except RuntimeError:tests.append({'refusal':change,'passed':True})
  else:raise AssertionError(change)
 print(json.dumps({'selected':len(tests),'passed':len(tests),'controls':tests,'scope':'Pure admission/process-name controls only; no deletion or Mac runtime proof.'}))
if __name__=='__main__':
 p=argparse.ArgumentParser();g=p.add_mutually_exclusive_group(required=True);g.add_argument('--self-test',action='store_true');g.add_argument('--execute-root-reviewed-exact-target',action='store_true');a=p.parse_args();controls() if a.self_test else clean()
