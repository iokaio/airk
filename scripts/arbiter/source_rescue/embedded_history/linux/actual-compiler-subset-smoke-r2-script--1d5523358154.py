# SPDX-License-Identifier: Apache-2.0
import os,pwd,pathlib,json,subprocess,time,hashlib,tarfile,posixpath,stat
assert os.geteuid()==1000 and pwd.getpwuid(1000).pw_name=='tyler';os.umask(0o077)
B=pathlib.Path('/home/tyler/airk-builds/arbiter-final-linux-r1');assert B.resolve()==B and B.stat().st_uid==1000
S=B/'toolchain/sysroot';assert not list(S.iterdir())
old=json.loads((B/'evidence/package-extraction-r1.json').read_text());assert old['passed'] is False and old['extracted']==[]
rows=[r for r in old['downloads'] if r['package']!='locales'];assert len(rows)==38
out=B/'evidence/compiler-subset-smoke-r2.json';record={'schema':'airk.private.rootless-compiler-subset-smoke/1','uid':os.getuid(),'passed':False,'excluded_packages':[{'name':'locales','reason':'noncompiler locale data with absolute /etc/locale.alias; preserved original .deb, omitted whole payload, no system installation claim'}],'preserved_original_failure':'package-extraction-r1.json','extracted':[],'commands':[]};began=time.monotonic()
def save():out.write_text(json.dumps(record,indent=2)+'\n')
def digest(p):
 with p.open('rb') as f:return hashlib.file_digest(f,'sha256').hexdigest()
def rel(s):
 assert not s.startswith('/');s=posixpath.normpath(s);assert s!='..' and not s.startswith('../');return s
try:
 for row in rows:
  p=pathlib.Path(row['path']);assert p.stat().st_uid==1000 and p.stat().st_size==row['bytes'] and digest(p)==row['sha256']
  child=subprocess.Popen(['/usr/bin/dpkg-deb','--fsys-tarfile',str(p)],stdout=subprocess.PIPE,stderr=subprocess.PIPE)
  with tarfile.open(fileobj=child.stdout,mode='r|') as tar:
   for m in tar:
    name=rel(m.name);assert m.isdir() or m.isfile() or m.issym() or m.islnk()
    assert (S/name).resolve(strict=False).is_relative_to(S)
    if m.issym():assert not m.linkname.startswith('/');rel(posixpath.join(posixpath.dirname(name),m.linkname))
    if m.islnk():rel(m.linkname)
  assert child.wait(timeout=15)==0
 # Full subset preflight completes before the first extraction.
 for row in rows:
  for p in S.rglob('*'):assert p.lstat().st_uid==1000 and p.resolve(strict=False).is_relative_to(S)
  p=subprocess.run(['/usr/bin/dpkg-deb','--extract',row['path'],str(S)],capture_output=True,text=True,timeout=45);assert p.returncode==0,p.stderr
  record['extracted'].append(row['package']);save()
 inventory=[]
 for p in sorted(S.rglob('*')):
  st=p.lstat();assert st.st_uid==1000 and p.resolve(strict=False).is_relative_to(S)
  inventory.append({'path':str(p.relative_to(S)),'kind':'link' if p.is_symlink() else 'dir' if p.is_dir() else 'file','uid':st.st_uid,'mode':oct(stat.S_IMODE(st.st_mode)),'target':os.readlink(p) if p.is_symlink() else None,'sha256':digest(p) if p.is_file() and not p.is_symlink() else None})
 inv=B/'evidence/compiler-sysroot-inventory-r2.json';inv.write_text(json.dumps(inventory,indent=2)+'\n');record['sysroot_inventory']={'path':str(inv),'sha256':digest(inv),'entries':len(inventory)}
 # Isolate compiler support libraries from libc so host executables keep a
 # coherent host loader/libc. Target link headers/libraries use the full sysroot.
 support=B/'toolchain/compiler-runtime';support.mkdir(mode=0o700)
 for stem in ['libisl.so','libmpc.so','libmpfr.so','libgmp.so','libzstd.so','libz.so','libstdc++.so','libgcc_s.so']:
  for src in (S/'usr/lib/x86_64-linux-gnu').glob(stem+'*'):
   dst=support/src.name
   if not dst.exists():dst.symlink_to(os.path.relpath(src,support))
 for name,binary in [('cc','x86_64-linux-gnu-gcc-13'),('cxx','x86_64-linux-gnu-g++-13')]:
  p=B/'toolchain/bin'/name
  p.write_text('#!/bin/sh\nexport LD_LIBRARY_PATH='+str(support)+'\nexec '+str(S/'usr/bin'/binary)+' --sysroot='+str(S)+' -B'+str(S/'usr/lib/gcc/x86_64-linux-gnu/13')+'/ -B'+str(S/'usr/lib/x86_64-linux-gnu')+'/ "$@"\n');p.chmod(0o700)
 p=B/'toolchain/bin/pkg-config';p.write_text('#!/bin/sh\nexport LD_LIBRARY_PATH='+str(support)+':'+str(S/'usr/lib/x86_64-linux-gnu')+'\nexport PKG_CONFIG_SYSROOT_DIR='+str(S)+'\nexport PKG_CONFIG_LIBDIR='+str(S/'usr/lib/x86_64-linux-gnu/pkgconfig')+':'+str(S/'usr/share/pkgconfig')+'\nexec '+str(S/'usr/bin/pkgconf')+' "$@"\n');p.chmod(0o700)
 smoke=B/'toolchain/smoke-r2';smoke.mkdir(mode=0o700)
 (smoke/'smoke.c').write_text('#include <stdio.h>\n#include <stdint.h>\nint main(void){uint32_t x=42;printf("C:%u\\n",x);return x==42?0:1;}\n')
 (smoke/'smoke.cpp').write_text('#include <iostream>\n#include <vector>\n#include <numeric>\n#include <thread>\nint main(){std::vector<int> v{1,2,3};int n=0;std::thread t([&]{n=std::accumulate(v.begin(),v.end(),0);});t.join();std::cout<<"CPP:"<<n<<"\\n";return n==6?0:1;}\n')
 def run(args):
  started=time.monotonic();p=subprocess.run(list(map(str,args)),cwd=smoke,capture_output=True,text=True,timeout=30)
  record['commands'].append({'argv':list(map(str,args)),'exit_code':p.returncode,'seconds':time.monotonic()-started,'stdout':p.stdout,'stderr':p.stderr});save();assert p.returncode==0,record['commands'][-1];return p.stdout
 run([B/'toolchain/bin/cc','--version']);run([B/'toolchain/bin/cxx','--version'])
 run([B/'toolchain/bin/cc','smoke.c','-o','smoke-c']);run([B/'toolchain/bin/cxx','-std=c++20','-pthread','smoke.cpp','-o','smoke-cpp'])
 assert run([smoke/'smoke-c'])=='C:42\n';assert run([smoke/'smoke-cpp'])=='CPP:6\n'
 for name in ['smoke-c','smoke-cpp']:run(['/usr/bin/readelf','-l','-d',smoke/name])
 record['tool_pins']=[{'path':str(p),'sha256':digest(p)} for p in [B/'toolchain/bin/cc',B/'toolchain/bin/cxx',S/'usr/bin/x86_64-linux-gnu-gcc-13',S/'usr/bin/x86_64-linux-gnu-g++-13',S/'usr/lib/gcc/x86_64-linux-gnu/13/cc1',S/'usr/lib/gcc/x86_64-linux-gnu/13/cc1plus',pathlib.Path('/usr/bin/as'),pathlib.Path('/usr/bin/ld'),pathlib.Path('/usr/bin/ar')]]
 record['passed']=True
except BaseException as e:record['failure']={'type':type(e).__name__,'message':str(e)};raise
finally:
 record['seconds']=time.monotonic()-began;save();print(json.dumps({k:v for k,v in record.items() if k!='commands'}));print('COMMANDS',json.dumps(record['commands'][-3:]))
