# SPDX-License-Identifier: Apache-2.0
import hashlib,json,os,posixpath,tarfile
from pathlib import Path,PurePosixPath
EXPECTED='7a2384641584ab9984221fcb0fc9265df465922af71be71410529e2a9c3f0540'
def inventory(archive):
 rows={};total=0
 for member in archive.getmembers():
  name=member.name.rstrip('/')
  if not name.startswith('python/') or any(p in ('','..','.') for p in name.split('/')) or '\\' in name or ':' in name:raise ValueError('Unsafe tool member')
  relative=name[len('python/'):]
  if relative in rows or not (member.isfile() or member.isdir() or member.issym()):raise ValueError('Duplicate/unsupported tool member')
  if member.mode & 0o7000:raise ValueError('Privileged tool mode')
  total+=member.size
  if total>256*1024*1024 or len(rows)>=10000:raise ValueError('Tool inventory bound')
  row={'kind':'file' if member.isfile() else 'directory' if member.isdir() else 'symlink','mode':member.mode&0o777}
  if member.isfile():
   content=archive.extractfile(member).read(64*1024*1024+1)
   if len(content)!=member.size:raise ValueError('Tool file size')
   row.update(bytes=len(content),sha256=hashlib.sha256(content).hexdigest())
  elif member.issym():
   link=member.linkname
   if not link or '\\' in link or PurePosixPath(link).is_absolute() or ':' in link:raise ValueError('Unsafe tool link')
   row['target']=link
  rows[relative]=row
 for name,row in rows.items():
  for parent in PurePosixPath(name).parents:
   if str(parent) in rows and rows[str(parent)]['kind']!='directory':raise ValueError('Tool parent is not directory')
  if row['kind']=='symlink':
   seen={name};current=posixpath.normpath(posixpath.join(posixpath.dirname(name),row['target']))
   for _ in range(32):
    if current=='..' or current.startswith('../') or current.startswith('/'):raise ValueError('Escaping tool link')
    if current in seen:raise ValueError('Cyclic tool link')
    seen.add(current)
    target=rows.get(current)
    if target is None:raise ValueError('Missing tool link target')
    if target['kind']!='symlink':break
    current=posixpath.normpath(posixpath.join(posixpath.dirname(current),target['target']))
   else:raise ValueError('Tool link depth')
 return rows

def provision(archive_path,destination):
 if archive_path.stat().st_size!=25479437 or hashlib.sha256(archive_path.read_bytes()).hexdigest()!=EXPECTED:raise ValueError('Official tool archive mismatch')
 if destination.exists():raise ValueError('Existing isolated tool retained; no overwrite')
 with tarfile.open(archive_path,'r:gz') as archive:
  rows=inventory(archive);destination.mkdir(parents=True)
  for name,row in rows.items():
   if row['kind']=='symlink':continue
   path=destination/name
   if row['kind']=='directory':path.mkdir(parents=True,exist_ok=True);continue
   path.parent.mkdir(parents=True,exist_ok=True)
   with path.open('xb') as output:output.write(archive.extractfile('python/'+name).read())
   path.chmod(row['mode'])
  for name,row in rows.items():
   if row['kind']=='symlink':
    path=destination/name;path.parent.mkdir(parents=True,exist_ok=True);path.symlink_to(row['target'])
 for name,row in rows.items():
  path=destination/name
  if row['kind']=='file' and (path.is_symlink() or path.stat().st_size!=row['bytes'] or hashlib.sha256(path.read_bytes()).hexdigest()!=row['sha256']):raise ValueError('Extracted tool bytes differ')
  if row['kind']=='symlink' and (os.readlink(path)!=row['target'] or not path.resolve().is_relative_to(destination.resolve())):raise ValueError('Extracted tool link differs')
 receipt={'schema':'airk.private-build-tool/1','role':'isolated-Mac-build-Python','version':'3.13.18','upstream':'astral-sh/python-build-standalone','release':'20261110','asset_id':733712223,'archive_sha256':EXPECTED,'files':rows,'licenses':[name for name in rows if any(token in name.lower() for token in ('license','copying','copyright','notice'))]}
 (destination/'provision-receipt.json').write_text(json.dumps(receipt,indent=2)+'\n',encoding='utf-8')
 print(json.dumps({'members':len(rows),'licenses':len(receipt['licenses']),'receipt_sha256':hashlib.sha256((destination/'provision-receipt.json').read_bytes()).hexdigest(),'executable':str(destination/'bin/python3.13')}))
if __name__=='__main__':
 import sys
 provision(Path(sys.argv[1]).resolve(),Path(sys.argv[2]).resolve())
