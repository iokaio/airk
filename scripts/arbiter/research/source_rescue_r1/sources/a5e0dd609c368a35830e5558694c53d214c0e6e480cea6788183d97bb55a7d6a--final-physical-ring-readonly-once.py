# SPDX-License-Identifier: Apache-2.0
from pathlib import Path
import os,stat,json,base64,hashlib
assert os.getuid()==os.geteuid()==501==os.stat('/dev/console').st_uid and os.getuid()!=0
base=Path('/Users/tylerjensen/airk-builds/arbiter-final-20261116-r1');assert base.resolve(strict=True)==base
owner=base/'evidence/platform-current-diagnostics-r1/fixtures/exact-reference-refresh/airk-combined-l90wRc';control=owner/'control';directory=control/'job-errors-1'
def ancestors(p):
 assert p.is_relative_to(base)
 for n in range(len(p.relative_to(base).parts)+1):
  q=base.joinpath(*p.relative_to(base).parts[:n]);s=q.lstat();assert stat.S_ISDIR(s.st_mode) and s.st_uid==501 and not q.is_symlink()
def meta(p):
 try:s=p.lstat()
 except FileNotFoundError:return {'path':str(p),'exists':False}
 return {'path':str(p),'exists':True,'mode':s.st_mode,'uid':s.st_uid,'gid':s.st_gid,'nlink':s.st_nlink,'inode':s.st_ino,'device':s.st_dev,'bytes':s.st_size,'mtime_ns':s.st_mtime_ns,'ctime_ns':s.st_ctime_ns,'atime_ns':s.st_atime_ns,'type':'regular' if stat.S_ISREG(s.st_mode) else 'directory' if stat.S_ISDIR(s.st_mode) else 'symlink' if stat.S_ISLNK(s.st_mode) else 'other'}
ancestors(owner);ancestors(control)
names=['current.jsonl']+['previous-'+str(n)+'.jsonl' for n in range(1,21)]
metadata=[meta(owner),meta(control),meta(control/'job-errors-1.owner.json'),meta(directory),meta(owner/'service.log')];listing=[];refusals=[];paths=[owner/'service.log']
if os.path.lexists(directory):
 ancestors(directory)
 with os.scandir(directory) as iterator:
  for index,item in enumerate(iterator):
   if index>=33:refusals.append('raw namespace listing exceeds bounded33entries');break
   p=directory/item.name;listing.append(meta(p))
   if item.name not in names:refusals.append('unknown rawnamespace entry: '+item.name)
   else:paths.append(p)
selected=[meta(p) for p in paths];existing=[m for m in selected if m['exists']]
if len(existing)>32:refusals.append('more than32ordinary files')
for m in existing:
 if m['type']!='regular' or m['uid']!=501 or m['nlink']!=1:refusals.append('unsafe/foreign/aliased logfile: '+m['path'])
if sum(m['bytes'] for m in existing)>8<<20:refusals.append('aggregate8MiBcap exceeded')
result={'schema':'airk.retained-final-physical-ring-observation/1','phase':'distinct final physical-ring observation; firstliveexport remainsoriginal0','identity':{'uid':os.getuid(),'effective_uid':os.geteuid(),'console_uid':os.stat('/dev/console').st_uid},'fixture':str(owner),'control':str(control),'metadata':metadata,'raw_namespace_listing':listing,'selected':selected,'caps':{'bytes':8<<20,'files':32,'listed_entries':33},'refusals':refusals,'contents':[],'status':'refused_metadata_only' if refusals else 'observed'}
if not refusals:
 for m in existing:
  p=Path(m['path']);ancestors(p.parent);fd=os.open(p,os.O_RDONLY|os.O_NOFOLLOW)
  try:
   before=os.fstat(fd);assert stat.S_ISREG(before.st_mode) and (before.st_dev,before.st_ino,before.st_uid,before.st_nlink,before.st_size)==(m['device'],m['inode'],501,1,m['bytes'])
   raw=bytearray()
   while True:
    block=os.read(fd,min(65536,m['bytes']+1-len(raw)))
    if not block:break
    raw.extend(block);assert len(raw)<=m['bytes']
   after=meta(p);assert len(raw)==m['bytes'] and tuple(after[k] for k in ('device','inode','uid','gid','nlink','bytes','mode','mtime_ns','ctime_ns'))==tuple(m[k] for k in ('device','inode','uid','gid','nlink','bytes','mode','mtime_ns','ctime_ns'))
   result['contents'].append({'path':str(p),'bytes':len(raw),'sha256':hashlib.sha256(raw).hexdigest(),'base64_complete':base64.b64encode(raw).decode(),'metadata_unchanged':True,'atime_before':m['atime_ns'],'atime_after':after['atime_ns']})
  finally:os.close(fd)
print(json.dumps(result))
