# SPDX-License-Identifier: Apache-2.0
import os,pwd,pathlib,json,hashlib,re,urllib.request,tarfile,posixpath,time
assert os.geteuid()==1000 and pwd.getpwuid(1000).pw_name=='tyler';os.umask(0o077)
B=pathlib.Path('/home/tyler/airk-builds/arbiter-final-linux-r1');assert B.resolve()==B and B.stat().st_uid==1000
r2=json.loads((B/'evidence/tool-integrity-pins-r2.json').read_text());assert r2['signature_exit_code']==0 and '[GNUPG:] VALIDSIG ' in r2['signature_status']
verified=B/'downloads/node-verified-SHASUMS256-r2.txt';unsigned=B/'downloads/node-SHASUMS256.txt';a,b=verified.read_bytes(),unsigned.read_bytes()
# Use authentic verified cleartext itself as checksum authority; preserve both raw
# files. No byte-equality claim or rewrite of the original unsigned mirror.
checksums={}
for line in a.decode().splitlines():
 if not line:continue
 m=re.fullmatch(r'([0-9a-f]{64})  ([A-Za-z0-9._/-]+)',line);assert m,line
 assert m.group(2) not in checksums;checksums[m.group(2)]=m.group(1)
assert checksums['node-v24.15.0-win-x64.zip']=='cc5149eabd53779ce1e7bdc5401643622d0c7e6800ade18928a767e940bb0e62'
assert checksums['node-v24.15.0-linux-x64.tar.xz']=='472655581fb851559730c48763e0c9d3bc25975c59d518003fc0849d3e4ba0f6'
record={'schema':'airk.private.linux-executable-acquisition/1','uid':os.getuid(),'passed':False,'node_checksum_authority':'Actual successfully PGP-verified cleartext body; unsigned raw catalog kept independently','verified_body_sha256':hashlib.sha256(a).hexdigest(),'unsigned_body_sha256':hashlib.sha256(b).hexdigest(),'catalog_byte_equal':a==b,'catalog_line_equal':a.splitlines()==b.splitlines(),'verified_bytes':len(a),'unsigned_bytes':len(b),'signer':r2['signature_status'],'downloads':[],'commands':[]};out=B/'evidence/user-executable-acquisition-r1.json';began=time.monotonic()
def sha(p):
 with p.open('rb') as f:return hashlib.file_digest(f,'sha256').hexdigest()
def save():out.write_text(json.dumps(record,indent=2)+'\n')
rows=[('rustup-init','https://static.rust-lang.org/rustup/dist/x86_64-unknown-linux-gnu/rustup-init','b1c8cb14e30889e7e2999e6b69e62d9e9b9ffc0f4df8180564d306d9680e6c55'),('node-v24.15.0-linux-x64.tar.xz','https://nodejs.org/dist/v24.15.0/node-v24.15.0-linux-x64.tar.xz',checksums['node-v24.15.0-linux-x64.tar.xz']),('cargo-nextest-0.9.148-x86_64-unknown-linux-gnu.tar.gz','https://github.com/nextest-rs/nextest/releases/download/cargo-nextest-0.9.148/cargo-nextest-0.9.148-x86_64-unknown-linux-gnu.tar.gz','223ba936714cc861a9cdedad5a8eb664dfdd69d6fbdf6cea6aa8184f523290dc')]
try:
 for name,url,expected in rows:
  target=B/'downloads'/name;assert not target.exists();h=hashlib.sha256();size=0
  with urllib.request.urlopen(url,timeout=60) as response,target.open('xb') as f:
   while True:
    data=response.read(1024*1024)
    if not data:break
    size+=len(data);assert size<=128*1024*1024;h.update(data);f.write(data)
  assert h.hexdigest()==expected,(name,h.hexdigest(),expected)
  record['downloads'].append({'name':name,'url':url,'bytes':size,'sha256':expected,'path':str(target)});save()
 for archive,root in [(B/'downloads/node-v24.15.0-linux-x64.tar.xz',B/'node'),(B/'downloads/cargo-nextest-0.9.148-x86_64-unknown-linux-gnu.tar.gz',B/'nextest')]:
  assert not list(root.iterdir())
  with tarfile.open(archive) as tar:
   for m in tar.getmembers():
    n=posixpath.normpath(m.name);assert not n.startswith('/') and n!='..' and not n.startswith('../')
    assert m.isdir() or m.isfile() or m.issym() or m.islnk()
    if m.issym():assert not m.linkname.startswith('/');resolved=posixpath.normpath(posixpath.join(posixpath.dirname(n),m.linkname));assert resolved!='..' and not resolved.startswith('../')
   tar.extractall(root,filter='data')
  assert all(p.lstat().st_uid==1000 and p.resolve(strict=False).is_relative_to(root) for p in root.rglob('*'))
 installer=B/'downloads/rustup-init';installer.chmod(0o700)
 record['passed']=True
except BaseException as e:record['failure']={'type':type(e).__name__,'message':str(e)};raise
finally:record['seconds']=time.monotonic()-began;save();print(json.dumps(record));print('RECEIPT_SHA256',sha(out))
