# SPDX-License-Identifier: Apache-2.0
import os,stat,json,hashlib,base64
from pathlib import Path
assert os.getuid()==os.geteuid()==os.stat('/dev/console').st_uid==501 and os.getuid()!=0
base=Path('/Users/tylerjensen/airk-builds/arbiter-final-control-alias-e306258a-r1');out=base/'evidence/platform-system-control-alias-nextest-continuation-r1';assert out.resolve(strict=True)==out and out.lstat().st_uid==501 and stat.S_ISDIR(out.lstat().st_mode)
selected=['batch-result.json','cargo-clean-result.json']
for phase in ['affected-control-regressions','private-log-controls','exact-reference-refresh']:
 selected += [phase+'-outer-result.json','logs/'+phase+'.log','receipts/neo-system-control-nextest-continuation-'+phase+'-r1.json','receipts/neo-system-control-nextest-continuation-'+phase+'-r1.log']
selected += ['logs/cargo-clean.log'];assert len(selected)<=32
rows=[];total=0
for rel in selected:
 p=out/rel
 if not p.exists():rows.append({'relative_path':rel,'exists':False});continue
 assert p.resolve(strict=True)==p and p.parent.resolve(strict=True)==p.parent
 s=p.lstat();assert stat.S_ISREG(s.st_mode) and s.st_uid==501 and s.st_nlink==1 and s.st_size<=4*1024*1024,(rel,'file guard')
 total+=s.st_size;rows.append({'relative_path':rel,'exists':True,'metadata':{'uid':s.st_uid,'gid':s.st_gid,'mode':oct(stat.S_IMODE(s.st_mode)),'nlink':s.st_nlink,'inode':s.st_ino,'bytes':s.st_size,'mtime_ns':s.st_mtime_ns,'ctime_ns':s.st_ctime_ns}})
assert total<=16*1024*1024
for row in rows:
 if not row['exists']:continue
 p=out/row['relative_path'];fd=os.open(p,os.O_RDONLY|os.O_NOFOLLOW)
 try:
  s=os.fstat(fd);assert (s.st_uid,s.st_nlink,s.st_ino,s.st_size,s.st_mtime_ns,s.st_ctime_ns)==(501,1,row['metadata']['inode'],row['metadata']['bytes'],row['metadata']['mtime_ns'],row['metadata']['ctime_ns']);data=b''
  while len(data)<=s.st_size:
   part=os.read(fd,min(65536,s.st_size+1-len(data)))
   if not part:break
   data+=part
  assert len(data)==s.st_size
  after=os.fstat(fd);assert (after.st_size,after.st_mtime_ns,after.st_ctime_ns)==(s.st_size,s.st_mtime_ns,s.st_ctime_ns)
 finally:os.close(fd)
 row['sha256']=hashlib.sha256(data).hexdigest();row['base64']=base64.b64encode(data).decode()
target=base/'source/target';target_meta={'exists':os.path.lexists(target)}
if target_meta['exists']:
 s=target.lstat();target_meta.update({'uid':s.st_uid,'directory':stat.S_ISDIR(s.st_mode),'symlink':stat.S_ISLNK(s.st_mode),'inode':s.st_ino})
print(json.dumps({'identity':{'real_uid':os.getuid(),'effective_uid':os.geteuid(),'console_uid':os.stat('/dev/console').st_uid},'scope':'Single bounded read-only final continuation receipt/log observation; no Cargo/control/cleanup/source writes','out':str(out),'files':rows,'total_bytes':total,'target_metadata_only':target_meta}))
