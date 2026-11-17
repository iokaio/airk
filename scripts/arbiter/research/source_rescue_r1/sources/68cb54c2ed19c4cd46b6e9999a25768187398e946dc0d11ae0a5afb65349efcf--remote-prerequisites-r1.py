# SPDX-License-Identifier: Apache-2.0

from pathlib import Path
import json,os,subprocess,time,hashlib,stat,sys
from datetime import datetime,timezone
uid=os.getuid();assert uid==501==os.stat('/dev/console').st_uid and uid!=0
base=Path.home()/'airk-builds/arbiter-final-20261116-r1';source=base/'source';evidence=base/'evidence';inputs=base/'inputs'
for q in (base,source,evidence,inputs):assert q.lstat().st_uid==uid and not q.is_symlink() and os.access(q,os.W_OK|os.X_OK)
assert not (evidence/'prerequisite-start-r1.json').exists()
tools={'python': '/Users/tylerjensen/airk-builds/timing-1.6.4.PutU1P/python/cpython-3.13.18-macos-aarch64-none/bin/python3.13', 'node': '/Users/tylerjensen/airk-builds/timing-1.6.4.PutU1P/tools/node-v24.15.0-darwin-arm64/bin/node', 'npm_cli': '/Users/tylerjensen/airk-builds/timing-1.6.4.PutU1P/tools/node-v24.15.0-darwin-arm64/lib/node_modules/npm/bin/npm-cli.js', 'cmake': '/Users/tylerjensen/airk-builds/one-airk-static-ort-1814-1794087059/tools/cmake-3.31.6-macos-universal/CMake.app/Contents/bin/cmake', 'ninja': '/Users/tylerjensen/airk-builds/one-airk-static-ort-1814-1794087059/ninja-build/ninja'};python=tools['python'];cmake=tools['cmake'];ninja=tools['ninja']
for name,path in tools.items():assert Path(path).stat().st_uid==uid
cache=Path.home()/'airk-builds/advanced-split-local-delivery-prerequisites-r1/inputs/bundled-model-v3'
profiles=['minilm-l6-v2-u8-v3','minilm-l6-v2-s8-v3'];model=inputs/'bundled-model-v3';ort=inputs/'static-ort130-aarch64';environment=evidence/'static-environment-r1.json'
assert not model.exists() and not ort.exists()
def sha(q):return hashlib.file_digest(q.open('rb'),'sha256').hexdigest()
cache_before=[]
for profile in profiles:
 for name in ('model.onnx','tokenizer.json','LICENSE'):
  q=cache/profile/name;m=q.lstat();assert stat.S_ISREG(m.st_mode) and m.st_uid==uid and m.st_nlink==1 and not q.is_symlink()
  cache_before.append({'path':str(q),'bytes':m.st_size,'sha256':sha(q),'uid':m.st_uid,'inode':m.st_ino})
commands=[[python,'-I','-B',str(source/'scripts/provision-local-model.py'),'--destination',str(model),'--profile',profile,'--source-directory',str(cache/profile)] for profile in profiles]
commands.append([python,'-I','-B',str(source/'scripts/provision-local-model.py'),'--destination',str(model)])
commands.append([python,'-I','-B',str(source/'scripts/provision-static-onnx-runtime.py'),'--target','aarch64-apple-darwin','--destination',str(ort),'--download','--build','--parallel','2','--cmake',cmake,'--ninja',ninja,'--environment-file',str(environment)])
commands.append([python,'-I','-B',str(source/'scripts/provision-static-onnx-runtime.py'),'--target','aarch64-apple-darwin','--destination',str(ort),'--verify'])
start={'schema':'airk.neo-prerequisite-attempt/1','status':'started','passed':False,'utc_started':datetime.now(timezone.utc).isoformat(),'uid':uid,'console_uid':os.stat('/dev/console').st_uid,'source':str(source),'commit':subprocess.check_output(['git','rev-parse','HEAD'],cwd=source,text=True).strip(),'source_custody_sha256':sha(evidence/'source-custody-r1.json'),'commands':commands,'source_provisioner_sha256':sha(source/'scripts/provision-static-onnx-runtime.py'),'model_provisioner_sha256':sha(source/'scripts/provision-local-model.py'),'cache_before':cache_before,'tool_paths':tools,'not_cargo':True}
with (evidence/'prerequisite-start-r1.json').open('x') as f:json.dump(start,f,indent=2);f.write('\n');f.flush();os.fsync(f.fileno())
env=dict(os.environ,PYTHONDONTWRITEBYTECODE='1',PYTHONNOUSERSITE='1');env['PATH']=str(Path(python).parent)+':'+str(Path(tools['node']).parent)+':'+str(Path.home()/'.cargo/bin')+':/usr/bin:/bin:/usr/sbin:/sbin'
records=[];began=time.monotonic();passed=False
with (evidence/'prerequisite-events-r1.jsonl').open('x',buffering=1) as events:
 try:
  for index,argv in enumerate(commands):
   assert os.getuid()==os.stat('/dev/console').st_uid==uid
   log=evidence/('prerequisite-command-%02d-r1.log'%index);t=time.monotonic()
   with log.open('xb') as out:
    child=subprocess.Popen(argv,cwd=source,env=env,stdin=subprocess.DEVNULL,stdout=out,stderr=subprocess.STDOUT)
    events.write(json.dumps({'event':'command_started','index':index,'pid':child.pid,'argv':argv,'utc':datetime.now(timezone.utc).isoformat(),'log':str(log)})+'\n');events.flush();os.fsync(events.fileno())
    code=child.wait()
   record={'index':index,'pid':child.pid,'argv':argv,'exit_code':code,'elapsed_seconds':time.monotonic()-t,'log':str(log),'log_sha256':sha(log)};records.append(record);events.write(json.dumps({'event':'command_completed',**record})+'\n');events.flush();os.fsync(events.fileno())
   print('completed command %d exit=%d seconds=%.3f log=%s'%(index,code,record['elapsed_seconds'],log),flush=True)
   if code:raise RuntimeError('prerequisite command %d failed; no retry'%index)
  for row in cache_before:
   q=Path(row['path']);s=q.stat();assert sha(q)==row['sha256'] and s.st_uid==row['uid'] and s.st_ino==row['inode']
  assert not subprocess.check_output(['git','--no-optional-locks','status','--porcelain=v1','--untracked-files=normal'],cwd=source)
  passed=True
 finally:
  result={**start,'status':'completed' if passed else 'failed','passed':passed,'utc_completed':datetime.now(timezone.utc).isoformat(),'elapsed_seconds':time.monotonic()-began,'observed_commands':records,'cache_originals_unchanged':passed,'qualification':'Static inputs/model-byte provisioning only; no Cargo, final native link, model numerical or installed-journey qualification.'}
  with (evidence/'prerequisite-result-r1.json').open('x') as f:json.dump(result,f,indent=2);f.write('\n');f.flush();os.fsync(f.fileno())
print(json.dumps({'passed':passed,'commands':len(records),'elapsed_seconds':result['elapsed_seconds'],'source':str(source),'models':str(model),'static_ort':str(ort),'environment_file':str(environment)},indent=2),flush=True)
