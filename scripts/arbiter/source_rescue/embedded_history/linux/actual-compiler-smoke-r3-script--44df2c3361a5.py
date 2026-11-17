# SPDX-License-Identifier: Apache-2.0
import os,pwd,pathlib,json,subprocess,time,hashlib,re
assert os.geteuid()==1000 and pwd.getpwuid(1000).pw_name=='tyler';os.umask(0o077)
B=pathlib.Path('/home/tyler/airk-builds/arbiter-final-linux-r1');S=B/'toolchain/sysroot';assert B.resolve()==B and B.stat().st_uid==1000
record={'schema':'airk.private.rootless-compiler-usrmerge-smoke/1','passed':False,'uid':os.getuid(),'previous_failure_preserved':'compiler-subset-smoke-r2.json','layout_aliases':[],'linker_script_roots':[],'commands':[]};began=time.monotonic();out=B/'evidence/compiler-smoke-r3.json'
def sha(p):
 with p.open('rb') as f:return hashlib.file_digest(f,'sha256').hexdigest()
def save():out.write_text(json.dumps(record,indent=2)+'\n')
try:
 for name,target in [('lib','usr/lib'),('lib64','usr/lib64')]:
  original=pathlib.Path('/')/name;assert original.is_symlink() and os.readlink(original)==target
  p=S/name;assert not p.exists() and not p.is_symlink() and (S/target).is_dir() and (S/target).resolve().is_relative_to(S)
  p.symlink_to(target);assert p.resolve().is_relative_to(S);record['layout_aliases'].append({'path':str(p),'target':target,'basis':'actual host Ubuntu usr-merged filesystem required by original libc linker scripts','uid':p.lstat().st_uid})
 for p in (S/'usr/lib/x86_64-linux-gnu').glob('*.so'):
  if p.is_symlink() or p.stat().st_size>16384:continue
  data=p.read_bytes()
  if b'GROUP' in data or b'INPUT' in data:
   for target in re.findall(rb'(?<!\S)(/[^\s()]+)',data):
    target=target.decode();selected=S/target.lstrip('/');assert selected.exists() and selected.resolve().is_relative_to(S),(p,target)
    record['linker_script_roots'].append({'script':str(p.relative_to(S)),'source_sha256':sha(p),'absolute_target':target,'within_sysroot':str(selected.resolve())})
 smoke=B/'toolchain/smoke-r3';smoke.mkdir(mode=0o700)
 for n in ['smoke.c','smoke.cpp']:(smoke/n).write_bytes((B/'toolchain/smoke-r2'/n).read_bytes())
 def run(argv):
  t=time.monotonic();p=subprocess.run(list(map(str,argv)),cwd=smoke,capture_output=True,text=True,timeout=30)
  record['commands'].append({'argv':list(map(str,argv)),'exit_code':p.returncode,'seconds':time.monotonic()-t,'stdout':p.stdout,'stderr':p.stderr});save();assert p.returncode==0,record['commands'][-1];return p.stdout
 run([B/'toolchain/bin/cc','smoke.c','-o','smoke-c']);run([B/'toolchain/bin/cxx','-std=c++20','-pthread','smoke.cpp','-o','smoke-cpp'])
 assert run([smoke/'smoke-c'])=='C:42\n';assert run([smoke/'smoke-cpp'])=='CPP:6\n'
 for n in ['smoke-c','smoke-cpp']:run(['/usr/bin/readelf','-l','-d',smoke/n])
 inventory=[]
 for p in sorted(S.rglob('*')):
  assert p.lstat().st_uid==1000 and p.resolve(strict=False).is_relative_to(S)
  inventory.append({'path':str(p.relative_to(S)),'kind':'link' if p.is_symlink() else 'dir' if p.is_dir() else 'file','target':os.readlink(p) if p.is_symlink() else None,'sha256':sha(p) if p.is_file() and not p.is_symlink() else None})
 inv=B/'evidence/compiler-sysroot-inventory-r3.json';inv.write_text(json.dumps(inventory,indent=2)+'\n');record['inventory']={'path':str(inv),'sha256':sha(inv),'entries':len(inventory)}
 record['tool_pins']=[{'path':str(p),'sha256':sha(p)} for p in [B/'toolchain/bin/cc',B/'toolchain/bin/cxx',S/'usr/bin/x86_64-linux-gnu-gcc-13',S/'usr/bin/x86_64-linux-gnu-g++-13',S/'usr/lib/gcc/x86_64-linux-gnu/13/cc1',S/'usr/lib/gcc/x86_64-linux-gnu/13/cc1plus',pathlib.Path('/usr/bin/as'),pathlib.Path('/usr/bin/ld'),pathlib.Path('/usr/bin/ar')]]
 record['passed']=True
except BaseException as e:record['failure']={'type':type(e).__name__,'message':str(e)};raise
finally:
 record['seconds']=time.monotonic()-began;save();print(json.dumps(record));print('RECEIPT_SHA256',sha(out))
