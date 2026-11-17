# SPDX-License-Identifier: Apache-2.0
from pathlib import Path
import os,stat,json,hashlib,base64
assert os.getuid()==os.geteuid()==501==os.stat('/dev/console').st_uid and os.getuid()!=0
base=Path('/Users/tylerjensen/airk-builds/arbiter-final-20261116-r1');assert base.resolve(strict=True)==base
control=base/'evidence/platform-current-diagnostics-r1/fixtures/exact-reference-refresh/airk-combined-l90wRc/control'
expected=[(control/'job-errors-1/current.jsonl',732,10542869,1794866228325345069),(control/'job-errors-1.owner.json',86,10542865,1794866228320199890)]
selected=[]
for p,size,inode,mtime in expected:
 for n in range(len(p.parent.relative_to(base).parts)+1):
  q=base.joinpath(*p.parent.relative_to(base).parts[:n]);s=q.lstat();assert stat.S_ISDIR(s.st_mode) and s.st_uid==501 and not q.is_symlink()
 s=p.lstat();assert stat.S_ISREG(s.st_mode) and s.st_uid==501 and s.st_nlink==1 and s.st_size==size and s.st_ino==inode and s.st_mtime_ns==mtime
 selected.append((p,s))
assert sum(s.st_size for p,s in selected)<=8<<20 and len(selected)<=32
files=[]
for p,s in selected:
 fd=os.open(p,os.O_RDONLY|os.O_NOFOLLOW)
 try:
  f=os.fstat(fd);assert (f.st_dev,f.st_ino,f.st_uid,f.st_nlink,f.st_size)==(s.st_dev,s.st_ino,501,1,s.st_size)
  raw=os.read(fd,s.st_size+1);assert len(raw)==s.st_size and not os.read(fd,1)
  after=p.lstat();assert (after.st_dev,after.st_ino,after.st_uid,after.st_gid,after.st_nlink,after.st_size,after.st_mode,after.st_mtime_ns,after.st_ctime_ns)==(s.st_dev,s.st_ino,s.st_uid,s.st_gid,s.st_nlink,s.st_size,s.st_mode,s.st_mtime_ns,s.st_ctime_ns)
  files.append({'path':str(p),'bytes':len(raw),'sha256':hashlib.sha256(raw).hexdigest(),'base64_complete':base64.b64encode(raw).decode(),'metadata_unchanged':True,'inode':s.st_ino,'mtime_ns':s.st_mtime_ns,'ctime_ns':s.st_ctime_ns,'atime_before':s.st_atime_ns,'atime_after':after.st_atime_ns})
 finally:os.close(fd)
print(json.dumps({'phase':'corrected finalphysicalring exact732Brecord+86Bowner','uid':os.getuid(),'effective_uid':os.geteuid(),'console_uid':os.stat('/dev/console').st_uid,'files':files,'scope':'Explicit in-scope correctedreadonlyguard; normalwriter.lock notread/mutated; firstlive0export andmetadatarefusalattempt unchanged; no writerdrain orotherstate/process/native operation'}))
