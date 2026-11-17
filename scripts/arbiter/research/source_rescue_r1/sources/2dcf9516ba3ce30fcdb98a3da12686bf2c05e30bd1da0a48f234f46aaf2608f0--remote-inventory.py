# SPDX-License-Identifier: Apache-2.0
import os,sys,stat,json,hashlib,subprocess,time
from pathlib import Path
assert os.getuid()==501==os.geteuid()==os.stat('/dev/console').st_uid and os.getuid()!=0
base=Path('/Users/tylerjensen/airk-builds/arbiter-final-20261116-r1');source=base/'source';home=Path('/Users/tylerjensen');old=home/'code/airk'
def sha(p):
 with p.open('rb') as f:return hashlib.file_digest(f,'sha256').hexdigest()
def cmd(argv,timeout=20,cwd=None):
 r=subprocess.run(argv,cwd=cwd,stdout=subprocess.PIPE,stderr=subprocess.PIPE,timeout=timeout)
 assert len(r.stdout)+len(r.stderr)<=1048576
 return {'argv':argv,'exit_code':r.returncode,'stdout':r.stdout.decode('utf-8','replace'),'stderr':r.stderr.decode('utf-8','replace')}
def git(*args,cwd=source):
 r=cmd(['/usr/bin/git','--no-optional-locks',*args],cwd=cwd);assert r['exit_code']==0;return r['stdout']
def snap():
 manifest=base/'transfer/bridge-059a2a89-r1/source-bridge-manifest.json';m=json.loads(manifest.read_bytes());assert m['commit']=='059a2a893069f89e8ea8aefec7f9bdb8bb6063b6'
 for row in m['files']:
  p=source/row['path'];s=p.lstat();assert stat.S_ISREG(s.st_mode) and s.st_uid==501 and s.st_nlink==1 and sha(p)==row['sha256']
 return {'head':git('rev-parse','HEAD').strip(),'source_clean':not git('status','--porcelain=v1','--untracked-files=all'),'manifest_sha256':sha(manifest),'source_files':len(m['files']),'raw_c7':m['raw_c7'],'old_dirty_sha256':hashlib.sha256(git('status','--porcelain=v1','-z','--untracked-files=normal',cwd=old).encode()).hexdigest(),'target_exists':(source/'target').exists()}
def meta(p):
 if not os.path.lexists(p):return {'path':str(p),'exists':False}
 s=p.lstat();return {'path':str(p),'exists':True,'uid':s.st_uid,'bytes':s.st_size,'mode':oct(stat.S_IMODE(s.st_mode)),'regular':stat.S_ISREG(s.st_mode),'directory':stat.S_ISDIR(s.st_mode),'symlink':stat.S_ISLNK(s.st_mode)}
before=snap();assert before['head']=='059a2a893069f89e8ea8aefec7f9bdb8bb6063b6' and before['source_clean'] and before['old_dirty_sha256']=='943936de2803601aa34eeb0d5c974bbb94261dc762496bd44196177755734b06' and not before['target_exists']
result={'actor':{'real_uid':os.getuid(),'effective_uid':os.geteuid(),'console_uid':os.stat('/dev/console').st_uid},'before':before,'os':cmd(['/usr/bin/sw_vers']),'uname':cmd(['/usr/bin/uname','-a']),'cpu':cmd(['/usr/sbin/sysctl','hw.model','hw.machine','hw.ncpu','hw.physicalcpu','hw.logicalcpu','hw.memsize','machdep.cpu.brand_string']),'translated_current':cmd(['/usr/sbin/sysctl','-n','sysctl.proc_translated']),'sdk_path':cmd(['/usr/bin/xcode-select','-p']),'disk':cmd(['/bin/df','-k',str(base),str(home)])}
packages=cmd(['/usr/sbin/pkgutil','--pkgs']);assert packages['exit_code']==0
rosetta=[x for x in packages['stdout'].splitlines() if 'rosetta' in x.lower()];result['rosetta_receipts']=[cmd(['/usr/sbin/pkgutil','--pkg-info',x]) for x in rosetta]
result['rosetta_runtime_metadata']=meta(Path('/Library/Apple/usr/libexec/oah/libRosettaRuntime'))
result['x86_64_system_execution']=cmd(['/usr/bin/arch','-x86_64','/usr/bin/uname','-m']) if rosetta else {'attempted':False,'reason':'No installed Rosetta package receipt; no install or emulation provisioning authorized'}
toolchains=home/'.rustup/toolchains';rust=[]
if toolchains.is_dir():
 for tc in sorted(toolchains.iterdir()):
  if tc.is_symlink() or not tc.is_dir() or tc.lstat().st_uid!=501:continue
  root=tc/'lib/rustlib';entry={'toolchain':str(tc),'targets':[]}
  if root.is_dir():
   for target in sorted(root.iterdir()):
    if target.name not in ('aarch64-apple-darwin','x86_64-apple-darwin'):continue
    lib=target/'lib';files=sorted(lib.glob('libstd-*')) if lib.is_dir() else []
    entry['targets'].append({'target':target.name,'metadata':meta(target),'std_artifacts':[meta(x) for x in files[:20]],'std_artifact_count':len(files)})
  rust.append(entry)
result['rust_toolchains_metadata']=rust
static=base/'evidence/static-environment-r1.json';s=static.lstat();assert stat.S_ISREG(s.st_mode) and s.st_uid==501 and s.st_size<=1048576
result['sealed_static_environment']={'metadata':meta(static),'sha256':sha(static),'value':json.loads(static.read_bytes())}
roots=[p for p in (home/'airk-builds').iterdir() if not p.is_symlink() and p.is_dir() and p.lstat().st_uid==501 and any(x in p.name.lower() for x in ('ort','onnx','static'))]
rows=[];count=0;cut=[];markers={'build-receipt.json','static-aggregate-receipt.json','static-consumer-link-receipt.json','libonnxruntime.a','onnxruntime-1.30.0-source.tar.gz','onnxruntime-1.22.1-source.tar.gz'}
for root in sorted(roots):
 for folder,dirs,files in os.walk(root,followlinks=False):
  p=Path(folder);count+=1;assert count<=5000,'bounded inventory directory limit'
  depth=len(p.relative_to(root).parts);dirs[:]=[n for n in dirs if depth<5 and n not in ('.git','node_modules','target','incremental','deps','.fingerprint','site-packages','__pycache__','third_party','source','dependency-source','build','b','mirror') and not (p/n).is_symlink()]
  for name in sorted(set(files)&markers):
   q=p/name;row=meta(q)
   if row['regular'] and row['uid']==501 and q.suffix=='.json' and row['bytes']<=1048576:
    d=json.loads(q.read_bytes());row['sha256']=sha(q);row['identity']={k:d[k] for k in ('schema','ort_version','target','result','upstream_commit','build_mode') if k in d}
   rows.append(row)
result['static_inventory']={'scope':'Only existing human-owned top-level airk-builds roots with ort/onnx/static names; depth<=5; omitted build/source/dependency trees, no writes or qualification','roots':[str(x) for x in roots],'walked_directories':count,'items':rows}
node=home/'airk-builds/timing-1.6.4.PutU1P/tools';result['existing_tool_directories']=[meta(x) for x in sorted(node.iterdir())] if node.is_dir() else []
result['after']=snap();assert result['after']==before
result['scope']='Read-only supported-platform availability inventory. No Cargo/rustup/build/install/provision/sign/cache/trust/remote evidence writes. Translation is not physical Intel qualification. Source full raw inventory and original dirty checkout unchanged. Filesystem read atime effects are possible.'
print(json.dumps(result,indent=2))
