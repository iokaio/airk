# SPDX-License-Identifier: Apache-2.0
import os,pwd,pathlib,json,subprocess,time,hashlib,tarfile,posixpath,stat
assert os.geteuid()==1000 and pwd.getpwuid(1000).pw_name=='tyler';os.umask(0o077)
B=pathlib.Path('/home/tyler/airk-builds/arbiter-final-linux-r1');assert B.resolve()==B and B.stat().st_uid==1000
rosterpath=B/'evidence/signed-package-roster-r1.json';assert hashlib.sha256(rosterpath.read_bytes()).hexdigest()=='52eba1bfa00d8cc3e4fe1f33507894154dc51e73c6c8e326339467f8a6b8273e'
rows=json.loads(rosterpath.read_text())['packages'];env={**os.environ,'APT_CONFIG':str(B/'apt/apt.conf'),'TMPDIR':str(B/'tmp')};D=B/'apt/downloads';S=B/'toolchain/sysroot';assert not list(D.iterdir()) and not list(S.iterdir())
record={'stage':'authenticated-user-package-download-extract','uid':os.getuid(),'passed':False,'roster_sha256':hashlib.sha256(rosterpath.read_bytes()).hexdigest(),'downloads':[],'preflighted':[],'extracted':[]};out=B/'evidence/package-extraction-r1.json';began=time.monotonic()
def save():out.write_text(json.dumps(record,indent=2)+'\n')
def digest(p):
 with p.open('rb') as f:return hashlib.file_digest(f,'sha256').hexdigest()
def rel(name):
 assert not name.startswith('/'),name
 name=posixpath.normpath(name)
 assert name!='..' and not name.startswith('../'),name
 return name
try:
 argv=['/usr/bin/apt-get','download',*[r['Package']+'='+r['Version'] for r in rows]]
 p=subprocess.run(argv,cwd=D,env=env,capture_output=True,text=True,timeout=180);record['download_command']={'argv':argv,'exit_code':p.returncode,'stdout':p.stdout,'stderr':p.stderr};save();assert p.returncode==0,'authenticated package download failed'
 files=list(D.glob('*.deb'));assert len(files)==len(rows)
 byhash={digest(p):p for p in files};assert len(byhash)==len(rows)
 for row in rows:
  p=byhash[row['SHA256']];assert p.stat().st_uid==1000 and p.stat().st_size==int(row['Size']);record['downloads'].append({'package':row['Package'],'path':str(p),'sha256':row['SHA256'],'bytes':p.stat().st_size})
 # Scan every archive before any extraction. A package may not create an escaping
 # link or write through a redirected parent; hardlinks are archive-root relative.
 for row in record['downloads']:
  p=pathlib.Path(row['path']);child=subprocess.Popen(['/usr/bin/dpkg-deb','--fsys-tarfile',str(p)],stdout=subprocess.PIPE,stderr=subprocess.PIPE)
  members=[]
  with tarfile.open(fileobj=child.stdout,mode='r|') as tar:
   for m in tar:
    name=rel(m.name)
    if name=='.':continue
    assert m.isdir() or m.isfile() or m.issym() or m.islnk(),(row['package'],name,'special')
    if m.issym():
     assert not m.linkname.startswith('/'),(row['package'],name,'absolute_symlink',m.linkname)
     rel(posixpath.join(posixpath.dirname(name),m.linkname))
    if m.islnk():rel(m.linkname)
    members.append({'path':name,'kind':'dir' if m.isdir() else 'symlink' if m.issym() else 'hardlink' if m.islnk() else 'file','link':m.linkname if m.issym() or m.islnk() else None})
  err=child.stderr.read();assert child.wait(timeout=10)==0,err
  for item in members:
   target=S/item['path'];assert target.resolve(strict=False).is_relative_to(S),str(target)
  record['preflighted'].append({'package':row['package'],'members':len(members),'inventory':members});save()
 for row in record['downloads']:
  # Recheck existing ancestors and all previously created links before next archive.
  assert S.resolve()==S and S.stat().st_uid==1000
  for p in S.rglob('*'):
   assert p.lstat().st_uid==1000 and p.resolve(strict=False).is_relative_to(S),str(p)
  p=subprocess.run(['/usr/bin/dpkg-deb','--extract',row['path'],str(S)],capture_output=True,text=True,timeout=45);assert p.returncode==0,p.stderr
  record['extracted'].append(row['package']);save()
 for p in S.rglob('*'):assert p.lstat().st_uid==1000 and p.resolve(strict=False).is_relative_to(S),str(p)
 record['passed']=True
except BaseException as error:record['failure']={'type':type(error).__name__,'message':str(error)};raise
finally:
 record['seconds']=time.monotonic()-began;save();print(json.dumps({k:v for k,v in record.items() if k not in ['preflighted','download_command']}))
