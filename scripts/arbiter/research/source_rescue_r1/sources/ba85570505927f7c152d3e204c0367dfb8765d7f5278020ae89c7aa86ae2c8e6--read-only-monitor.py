# SPDX-License-Identifier: Apache-2.0
from pathlib import Path
import os,json,stat,base64
assert os.getuid()==os.geteuid()==501==os.stat('/dev/console').st_uid and os.getuid()!=0
b=Path.home()/'airk-builds/arbiter-final-20261116-r1';out=b/'evidence/platform-current-diagnostics-r1'
result={'uid':os.getuid(),'effective_uid':os.geteuid(),'console_uid':os.stat('/dev/console').st_uid,'out_exists':out.exists(),'target_exists':(b/'source/target').exists(),'files':{},'log_tails':{}}
if out.exists():
 assert out.is_dir() and out.lstat().st_uid==501 and not out.is_symlink()
 names=['batch-start.json','batch-result.json','cargo-clean-result.json']
 for phase in ('scoped-clippy','exact-cli-replacement','exact-reference-refresh'):
  names.extend([phase+'-outer-start.json',phase+'-outer-result.json',phase+'-independent-continuation.json','receipts/neo-current-diagnostics-'+phase+'-r1.json'])
 for name in names:
  p=out/name
  if not p.exists():continue
  st=p.lstat();assert stat.S_ISREG(st.st_mode) and st.st_uid==501 and st.st_nlink==1 and st.st_size<=8*1024*1024
  result['files'][name]=base64.b64encode(p.read_bytes()).decode()
 for phase in ('scoped-clippy','exact-cli-replacement','exact-reference-refresh','cargo-clean'):
  p=out/'logs'/(phase+'.log')
  if p.exists():
   st=p.lstat();assert stat.S_ISREG(st.st_mode) and st.st_uid==501 and st.st_nlink==1
   with p.open('rb') as f:f.seek(max(0,st.st_size-12000));data=f.read(12000)
   result['log_tails'][phase]={'bytes_total':st.st_size,'tail':data.decode(errors='replace')}
# The one bounded fixture read happens only after actual diagnostic results.
import re,hashlib
assert (out/'batch-result.json').is_file(), 'batch final outcome not yet observed; do not fetch retained fixture early'
result['refresh_fixture']={'scope':'one bounded original retained exporter snapshot only; no DB/corpus/process operations','found':False}
refresh_log=out/'receipts/neo-current-diagnostics-exact-reference-refresh-r1.log'
if refresh_log.exists():
 log=refresh_log.read_text(encoding='utf-8',errors='strict')
 matches=re.findall(r'^failed refresh fixture retained: (.+?)(?:; secondary diagnostic capture failed:.*)?$',log,re.MULTILINE)
 assert len(matches)<=1
 if matches:
  fixture=Path(matches[0]);allowed=out/'fixtures/exact-reference-refresh'
  assert fixture.parent==allowed and re.fullmatch(r'[A-Za-z0-9_.-]{1,128}',fixture.name) and fixture.name not in ('.','..')
  def ancestry(path):
   assert path.is_relative_to(b)
   for index in range(len(path.relative_to(b).parts)+1):
    q=b.joinpath(*path.relative_to(b).parts[:index]);s=q.lstat();assert stat.S_ISDIR(s.st_mode) and s.st_uid==501 and not q.is_symlink()
  ancestry(fixture);evidence=fixture/'reference-refresh-failure';before=[]
  for name,limit in [('observations.json',256<<10),('raw-export.json',64<<10),('raw-errors.jsonl',16<<20)]:
   q=evidence/name
   if not os.path.lexists(q):before.append((q,limit,None));continue
   ancestry(q.parent);s=q.lstat();assert stat.S_ISREG(s.st_mode) and s.st_uid==501 and s.st_nlink==1 and s.st_size<=limit and not q.is_symlink()
   before.append((q,limit,s))
  existing=sum(s.st_size for q,limit,s in before if s is not None)
  basebytes=sum(len(base64.b64decode(value)) for value in result['files'].values())
  assert existing+basebytes<=32<<20, '32MiB aggregate cap; no truncation/content replay'
  entries=[]
  for q,limit,s in before:
   if s is None:entries.append({'name':q.name,'exists':False});continue
   fd=os.open(q,os.O_RDONLY|os.O_NOFOLLOW)
   try:
    st=os.fstat(fd);assert (st.st_dev,st.st_ino,st.st_uid,st.st_nlink,st.st_size)==(s.st_dev,s.st_ino,501,1,s.st_size)
    raw=bytearray()
    while True:
     block=os.read(fd,min(65536,limit+1-len(raw)))
     if not block:break
     raw.extend(block);assert len(raw)<=limit
    after=q.lstat();assert len(raw)==s.st_size and (after.st_dev,after.st_ino,after.st_uid,after.st_nlink,after.st_size,after.st_mode,after.st_mtime_ns,after.st_ctime_ns)==(s.st_dev,s.st_ino,501,1,s.st_size,s.st_mode,s.st_mtime_ns,s.st_ctime_ns)
    entries.append({'name':q.name,'exists':True,'bytes':len(raw),'sha256':hashlib.sha256(raw).hexdigest(),'base64_complete':base64.b64encode(raw).decode(),'uid':501,'inode':s.st_ino,'mtime_ns':s.st_mtime_ns,'ctime_ns':s.st_ctime_ns,'metadata_unchanged':True,'atime_unchanged':s.st_atime_ns==after.st_atime_ns})
   finally:os.close(fd)
  result['refresh_fixture'].update(found=True,path=str(fixture),entries=entries,aggregate_fixture_bytes=existing,scope='Original one-live-snapshot exporter bytes; not writer drain/graceful-stop/currentness beyond captured snapshot')
print(json.dumps(result))
