# SPDX-License-Identifier: Apache-2.0
import os,stat,json,hashlib,base64
from pathlib import Path
assert os.getuid()==os.geteuid()==os.stat('/dev/console').st_uid==501 and os.getuid()!=0
base=Path('/Users/tylerjensen/airk-builds/arbiter-final-control-alias-e306258a-r1');out=base/'evidence/manual-exact-target-cleanup-r1';assert out.resolve(strict=True)==out and out.lstat().st_uid==501
names=['result.json','start.json','before-tag.json','before-storage.json','after-inventory.json','hardlink-closure.json','removed-entries.json','before-inventory.json','before-processes.json','predelete-processes.json','after-processes.json'];rows=[];total=0
for name in names:
 p=out/name;assert p.resolve(strict=True)==p;s=p.lstat();assert stat.S_ISREG(s.st_mode) and s.st_uid==501 and s.st_nlink==1 and s.st_size<=8*1024*1024;total+=s.st_size;rows.append({'name':name,'uid':s.st_uid,'inode':s.st_ino,'bytes':s.st_size,'mtime_ns':s.st_mtime_ns,'ctime_ns':s.st_ctime_ns})
assert total<=16*1024*1024
for row in rows:
 p=out/row['name'];fd=os.open(p,os.O_RDONLY|os.O_NOFOLLOW)
 try:
  s=os.fstat(fd);assert (s.st_uid,s.st_ino,s.st_size,s.st_mtime_ns,s.st_ctime_ns)==(501,row['inode'],row['bytes'],row['mtime_ns'],row['ctime_ns']);data=b''
  while len(data)<=s.st_size:
   chunk=os.read(fd,min(65536,s.st_size+1-len(data)))
   if not chunk:break
   data+=chunk
  assert len(data)==s.st_size;a=os.fstat(fd);assert (a.st_ino,a.st_size,a.st_mtime_ns,a.st_ctime_ns)==(s.st_ino,s.st_size,s.st_mtime_ns,s.st_ctime_ns)
 finally:os.close(fd)
 row.update({'sha256':hashlib.sha256(data).hexdigest(),'base64':base64.b64encode(data).decode()})
print(json.dumps({'identity':{'real_uid':os.getuid(),'effective_uid':os.geteuid(),'console_uid':os.stat('/dev/console').st_uid},'target_exists_now':os.path.lexists(base/'source/target'),'files':rows,'total_bytes':total,'scope':'Single read-only cleanup receipt harvest; no cleanup/Cargo/tests or source changes.'}))
