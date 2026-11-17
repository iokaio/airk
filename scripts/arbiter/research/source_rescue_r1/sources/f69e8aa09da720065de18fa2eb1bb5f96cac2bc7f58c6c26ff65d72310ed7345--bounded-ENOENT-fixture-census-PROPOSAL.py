# SPDX-License-Identifier: Apache-2.0
# Bounded, read-only evidence fetch. Caller owns the single Root-authorized SSH run.
from pathlib import Path
import os,stat,json,hashlib,base64,fnmatch
assert os.getuid()==os.geteuid()==501==os.stat('/dev/console').st_uid and os.getuid()!=0
base=Path('/Users/tylerjensen/airk-builds/arbiter-final-20261116-r1')
assert base.resolve(strict=True)==base
fixture=base/'evidence/platform-process-observer-repair-r1/fixtures/exact-cli-replacement'
owner=fixture/'.tmpw0UT5F';source=fixture/'.tmppJhhgu'
def ancestry(path):
 assert path.is_relative_to(base)
 for q in [base,*[base.joinpath(*path.relative_to(base).parts[:i]) for i in range(1,len(path.relative_to(base).parts)+1)]]:
  p=q
  s=p.lstat();assert stat.S_ISDIR(s.st_mode) and s.st_uid==501 and not p.is_symlink()
for p in (owner,source):ancestry(p);s=p.lstat();assert stat.S_ISDIR(s.st_mode) and s.st_uid==501 and not p.is_symlink()
def metadata(p):
 try:s=p.lstat()
 except FileNotFoundError:return {'path':str(p),'exists':False}
 return {'path':str(p),'exists':True,'type':'regular' if stat.S_ISREG(s.st_mode) else 'directory' if stat.S_ISDIR(s.st_mode) else 'symlink' if stat.S_ISLNK(s.st_mode) else 'other','uid':s.st_uid,'mode':s.st_mode,'nlink':s.st_nlink,'device':s.st_dev,'inode':s.st_ino,'bytes':s.st_size,'mtime_ns':s.st_mtime_ns,'ctime_ns':s.st_ctime_ns,'atime_ns':s.st_atime_ns}
top=sorted(owner.iterdir(),key=lambda p:p.name);census=[metadata(p) for p in top]
print(json.dumps({'phase':'metadata-first','uid':os.getuid(),'effective_uid':os.geteuid(),'console_uid':os.stat('/dev/console').st_uid,'owner':str(owner),'source':str(source),'top_level':census}),flush=True)
commands=[p for p in top if fnmatch.fnmatchcase(p.name,'command-*.stdout') or fnmatch.fnmatchcase(p.name,'command-*.stderr')]
small=[owner/'.install.replacement-1/receipt.json',owner/'install/selected.json',owner/'install/runtime-layout.json',owner/'shutdown-complete.json',owner/'state/control/airk-control-1.4.owner.json']
selected=[]
for p,cap in [(p,1<<20) for p in commands]+[(p,64<<10) for p in small]:
 m=metadata(p)
 if not m['exists']:selected.append((p,cap,m));continue
 ancestry(p.parent);assert m['type']=='regular' and m['uid']==501 and m['nlink']==1 and m['bytes']<=cap,'nonregular/foreign/aliased/oversize: '+str(p)
 selected.append((p,cap,m))
original=source/'original.txt';source_m=metadata(original)
if source_m['exists']:
 assert source_m['type']=='regular' and source_m['uid']==501 and source_m['nlink']==1 and source_m['bytes']<=1<<20
assert sum(m.get('bytes',0) for p,cap,m in selected)+source_m.get('bytes',0)<=16<<20,'total16MiBcap exceeded; no content read'
def read_exact(p,cap,before):
 descriptor=os.open(p,os.O_RDONLY|os.O_NOFOLLOW)
 try:
  f=os.fstat(descriptor);assert stat.S_ISREG(f.st_mode) and f.st_uid==501 and f.st_nlink==1 and (f.st_dev,f.st_ino,f.st_size)==(before['device'],before['inode'],before['bytes'])
  data=bytearray()
  while True:
   block=os.read(descriptor,min(65536,cap+1-len(data)))
   if not block:break
   data.extend(block)
   assert len(data)<=cap,'cap exceeded; never truncate'
  after=metadata(p);assert (after['device'],after['inode'],after['bytes'],after['mtime_ns'],after['ctime_ns'],after['uid'],after['mode'],after['nlink'])==(before['device'],before['inode'],before['bytes'],before['mtime_ns'],before['ctime_ns'],before['uid'],before['mode'],before['nlink'])
  assert len(data)==before['bytes'];return bytes(data),after
 finally:os.close(descriptor)
streams=[]
for p,cap,m in selected:
 if not m['exists']:streams.append({'metadata':m});continue
 raw,after=read_exact(p,cap,m);streams.append({'metadata':m,'after':after,'sha256':hashlib.sha256(raw).hexdigest(),'base64_complete_bytes':base64.b64encode(raw).decode(),'atime_unchanged':m['atime_ns']==after['atime_ns']})
m=source_m;sentinel={'metadata':m}
if m['exists']:
 assert m['type']=='regular' and m['uid']==501 and m['nlink']==1 and m['bytes']<=1<<20
 raw,after=read_exact(original,1<<20,m);sentinel.update(sha256=hashlib.sha256(raw).hexdigest(),after=after)
print(json.dumps({'phase':'complete-evidence','identity':{'real_uid':os.getuid(),'effective_uid':os.geteuid(),'console_uid':os.stat('/dev/console').st_uid},'expected_source_commit_context':'f5f9dceceb91ce3d8bace0061522dbf889612c09','expected_source_C7_context':{'sha256':'00b03d498f4afc6f7cf5694d4f4d7e1ee8e6c2ac0833a40da90095962e70e245','files':1942},'source_context_limit':'Frozen f5 source/receipt identities verified by Root/transport reviewer; this fixture-only probe does not rederive C7.','top_level_order_by_name':census,'streams_order_by_name_then_fixed_metadata':streams,'source_original_sha_only':sentinel,'caps':{'command_each':1<<20,'small_json_each':64<<10,'total':16<<20},'preservation':'No writes/utime/DB/corpus/process operations. Content/mtime/ctime/owner/mode/inode checked unchanged; actual atime observation disclosed without restoration.'}),flush=True)
