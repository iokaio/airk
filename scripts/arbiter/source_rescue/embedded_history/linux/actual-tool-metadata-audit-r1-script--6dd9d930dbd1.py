# SPDX-License-Identifier: Apache-2.0
import os,pwd,pathlib,json,hashlib,urllib.request,subprocess,tarfile,posixpath
assert os.geteuid()==1000 and pwd.getpwuid(1000).pw_name=='tyler';os.umask(0o077)
B=pathlib.Path('/home/tyler/airk-builds/arbiter-final-linux-r1');assert B.resolve()==B and B.stat().st_uid==1000
metadata=[]
for name,url in [('rustup-init.sha256','https://static.rust-lang.org/rustup/dist/x86_64-unknown-linux-gnu/rustup-init.sha256'),('node-SHASUMS256.txt','https://nodejs.org/dist/v24.15.0/SHASUMS256.txt'),('node-SHASUMS256.txt.asc','https://nodejs.org/dist/v24.15.0/SHASUMS256.txt.asc')]:
 try:
  with urllib.request.urlopen(url,timeout=30) as response:data=response.read(1048577);assert len(data)<=1048576;final=response.url
  p=B/'downloads'/name
  with p.open('xb') as f:f.write(data)
  metadata.append({'name':name,'url':url,'final_url':final,'sha256':hashlib.sha256(data).hexdigest(),'bytes':len(data),'content':data.decode('utf-8')})
 except Exception as e:metadata.append({'name':name,'url':url,'failure':{'type':type(e).__name__,'message':str(e)}})
escapes=[];counts=[]
for p in sorted((B/'apt/downloads').glob('*.deb')):
 child=subprocess.Popen(['/usr/bin/dpkg-deb','--fsys-tarfile',str(p)],stdout=subprocess.PIPE,stderr=subprocess.PIPE);n=0
 with tarfile.open(fileobj=child.stdout,mode='r|') as tar:
  for m in tar:
   n+=1
   if m.issym():
    target=posixpath.normpath(posixpath.join(posixpath.dirname(m.name),m.linkname))
    if m.linkname.startswith('/') or target=='..' or target.startswith('../'):escapes.append({'archive':p.name,'path':m.name,'target':m.linkname})
 assert child.wait(timeout=10)==0
 counts.append({'archive':p.name,'members':n})
r={'schema':'airk.private.user-tool-metadata-audit/1','uid':os.getuid(),'official_metadata':metadata,'all_package_member_census':counts,'escaping_links':escapes,'packages_extracted':0,'executables_run':False};p=B/'evidence/tool-metadata-and-package-links-r1.json';p.write_text(json.dumps(r,indent=2)+'\n');print(json.dumps(r));print('RECEIPT_SHA256',hashlib.sha256(p.read_bytes()).hexdigest())
