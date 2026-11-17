# SPDX-License-Identifier: Apache-2.0
import base64,hashlib,io,json,shlex,subprocess,tarfile,time
from pathlib import Path
repo=Path.cwd(); config=json.loads((repo/'artifacts/v190/neo-connection.local.json').read_text())
assert config['host']=='192.168.68.61'
files=['scripts/provision-static-onnx-runtime.py','scripts/adopt-neo-static-onnx-runtime.py','scripts/static-onnx-link-closure.py','scripts/static-onnx-runtime-lock.json']
stamp=str(int(time.time())); local=repo/'artifacts/local/one-airk-1814/neo-static-ort'/('supplement-'+stamp);local.mkdir()
root='/Users/tylerjensen/airk-builds/one-airk-static-ort-1814-1794087059';area=root+'/supplement-tools-'+stamp
python='/Users/tylerjensen/airk-builds/timing-1.6.4.PutU1P/python/cpython-3.13.18-macos-aarch64-none/bin/python3.13'
ssh=['ssh','-i',config['identity_path'],*config['ssh_options'],'-p',str(config['port']),config['user']+'@'+config['host']]
hashes={name:hashlib.sha256((repo/name).read_bytes()).hexdigest() for name in files}
(local/'tool-sync.json').write_text(json.dumps({'host':config['host'],'root':root,'tool_area':area,'hashes':hashes},indent=2)+'\n')
def remote(code,input_bytes=None,timeout=60):
 command=shlex.quote(python)+' -c '+shlex.quote('import base64;exec(base64.b64decode('+repr(base64.b64encode(code.encode()).decode())+'))')
 return subprocess.run(ssh+[command],input=input_bytes,capture_output=True,timeout=timeout)
payload=io.BytesIO()
with tarfile.open(fileobj=payload,mode='w') as out:
 for name in files:out.add(repo/name,arcname=name,recursive=False)
code=f'''
import hashlib,io,json,os,platform,shutil,socket,sys,tarfile
from pathlib import Path
assert os.getuid()==501 and platform.machine()=='arm64' and socket.gethostname()=='Tylers-MacBook-Neo.local'
root=Path({root!r});assert root.resolve(strict=True)==root
area=Path({area!r});area.mkdir(mode=0o700);(area/'scripts').mkdir()
expected={hashes!r}
with tarfile.open(fileobj=io.BytesIO(sys.stdin.buffer.read()),mode='r:') as archive:
 members=archive.getmembers();assert set(m.name for m in members)==set(expected)
 for member in members:
  assert member.isfile() and 0<member.size<1024*1024
  value=archive.extractfile(member).read();assert hashlib.sha256(value).hexdigest()==expected[member.name]
  (area/member.name).write_bytes(value)
print(json.dumps({{'hostname':socket.gethostname(),'free_bytes':shutil.disk_usage(root).free,'tool_area':str(area),'hashes':expected}}))
'''
p=remote(code,payload.getvalue());(local/'sync.stdout').write_bytes(p.stdout);(local/'sync.stderr').write_bytes(p.stderr)
assert p.returncode==0, p.stderr.decode(errors='replace')
print('SYNC '+str(local),flush=True)
code=f'''
import hashlib,json,os,resource,subprocess,sys,time
from pathlib import Path
root=Path({root!r});area=Path({area!r});receipt_paths=['build-receipt.json','evidence2/receipt.json','postbuild-evidence/archive-inspection.json']
digest=lambda p:hashlib.sha256(p.read_bytes()).hexdigest()
before={{name:digest(root/name) for name in receipt_paths}}
assert before['build-receipt.json']=='1973d3a7252c60f17a8ac898e709cb5b61775134ea34a8376064b627108b023d'
soft,hard=resource.getrlimit(resource.RLIMIT_NOFILE);assert hard>=4096;resource.setrlimit(resource.RLIMIT_NOFILE,(4096,hard))
result={{'schema':1,'host':'192.168.68.61','purpose':'additive real re2 link prerequisite, no AIRK Cargo or final-link/model qualification','original_receipts':before,'commands':[],'nofile_before':[soft,hard],'nofile_after':list(resource.getrlimit(resource.RLIMIT_NOFILE))}}
for name,flags in [('supplement',['--supplement-link-closure']),('verify',['--verify'])]:
 argv=[sys.executable,str(area/'scripts/provision-static-onnx-runtime.py'),'--target','aarch64-apple-darwin','--destination',str(root),*flags,'--environment-file',str(area/(name+'-environment.json'))]
 tick=time.monotonic()
 with (area/(name+'.stdout.json')).open('wb') as out,(area/(name+'.stderr')).open('wb') as err:
  p=subprocess.run(argv,stdout=out,stderr=err,timeout=1500)
 result['commands'].append({{'stage':name,'argv':argv,'exit_code':p.returncode,'elapsed_seconds':round(time.monotonic()-tick,3)}})
 (area/'execution.json').write_text(json.dumps(result,indent=2)+'\\n')
 if p.returncode:break
after={{name:digest(root/name) for name in receipt_paths}};result['original_receipts_after']=after;result['original_receipts_unchanged']=before==after
assert before==after
result['result']='passed' if len(result['commands'])==2 and all(x['exit_code']==0 for x in result['commands']) else 'failed'
(area/'execution.json').write_text(json.dumps(result,indent=2)+'\\n');print(json.dumps(result));sys.exit(0 if result['result']=='passed' else 1)
'''
tick=time.monotonic();p=remote(code,timeout=1650);(local/'ssh.stdout').write_bytes(p.stdout);(local/'ssh.stderr').write_bytes(p.stderr)
(local/'transport.json').write_text(json.dumps({'exit_code':p.returncode,'elapsed_seconds':round(time.monotonic()-tick,3),'tool_area':area},indent=2)+'\n')
print('BATCH '+str(p.returncode)+' '+p.stdout.decode(errors='replace'),flush=True)
code=f'''
import io,sys,tarfile
from pathlib import Path
root=Path({root!r});area=Path({area!r})
with tarfile.open(fileobj=sys.stdout.buffer,mode='w|gz') as out:
 for p in area.rglob('*'):
  if p.is_file():out.add(p,arcname='tool-area/'+p.relative_to(area).as_posix(),recursive=False)
 for name in ['link-supplement-receipt.json','link-supplement-attempt.json','logs/link-supplement.log']:
  p=root/name
  if p.exists():out.add(p,arcname=name,recursive=False)
'''
collection=remote(code,timeout=60);(local/'complete-evidence.tar.gz').write_bytes(collection.stdout);(local/'collection.stderr').write_bytes(collection.stderr)
assert collection.returncode==0
with tarfile.open(fileobj=io.BytesIO(collection.stdout),mode='r:gz') as tar:tar.extractall(local/'complete',filter='data')
print('EVIDENCE '+str(local),flush=True)
raise SystemExit(p.returncode)
