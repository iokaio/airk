# SPDX-License-Identifier: Apache-2.0
import os,pwd,pathlib,json,hashlib,subprocess,time,datetime
assert os.getuid()==os.geteuid()==1000 and pwd.getpwuid(1000).pw_name=='tyler';os.umask(0o077)
B=pathlib.Path('/home/tyler/airk-builds/arbiter-final-linux-r1');S=B/'source';D=B/'inputs/static-ort';assert B.resolve()==B and B.stat().st_uid==1000
sha=lambda p:hashlib.sha256(p.read_bytes()).hexdigest();bridge=B/'evidence/reviewed-aggregate-overlay-r1.json';assert sha(bridge)=='fcf6f9030b801db930cfb509bd561229539dfd35f3d35abcb2f47ad7a31be317'
for row in json.loads(bridge.read_text())['files']:assert sha(S/row['path'])==row['after_sha256']
H=json.loads((B/'evidence/userspace-handoff-r1.json').read_text());env={**os.environ,**H['environment_for_future_owned_child'],'PYTHONNOUSERSITE':'1'}
F=B/'inputs/static-environment-aggregate-r2.json';assert not F.exists() and not (D/'aggregate').exists() and not (D/'static-aggregate-attempt.json').exists()
run=B/'evidence/static-aggregate-execution-r2';run.mkdir(mode=0o700)
argv=['/usr/bin/python3','-I','-B','scripts/provision-static-onnx-runtime.py','--target','x86_64-unknown-linux-gnu','--destination',str(D),'--aggregate-static','--environment-file',str(F)]
r={'schema':'airk.private.linux-static-aggregate-execution/1','passed':False,'uid':os.getuid(),'start_utc':datetime.datetime.now(datetime.timezone.utc).isoformat(),'argv':argv,'cwd':str(S),'source_bridge_sha256':sha(bridge),'environment_override':H['environment_for_future_owned_child'],'original_cpp_build_receipt_sha256':sha(D/'build-receipt.json'),'original_whole_failure_preserved_sha256':'7421c6d2a0949203f82cadac433640129a6738fef2b76babbffd816e035cfae8','original_aggregation_limits':{'seconds':300,'log_bytes':67108864,'cleanup_seconds':5},'CPP_rebuild':False,'Cargo_or_model_run':False};out=run/'execution.json';began=time.monotonic()
def save():out.write_text(json.dumps(r,indent=2)+'\n')
rc=1
try:
 with (run/'stdout.log').open('xb') as stdout,(run/'stderr.log').open('xb') as stderr:
  child=subprocess.Popen(argv,cwd=S,env=env,stdout=stdout,stderr=stderr);r['controller_pid']=child.pid;r['controller_uid']=os.stat('/proc/'+str(child.pid)).st_uid;assert r['controller_uid']==1000;save();rc=child.wait()
 r['exit_code']=rc;r['controller_return_observed']=True
 r['passed']=rc==0
 r['stdout_sha256']=sha(run/'stdout.log');r['stderr_sha256']=sha(run/'stderr.log')
 if rc:
  r['failure']={'stdout_tail':(run/'stdout.log').read_text(errors='replace')[-4000:],'stderr_tail':(run/'stderr.log').read_text(errors='replace')[-4000:]}
 else:
  data=json.loads(F.read_text());assert len(data['environment'])==5
  agg=D/'static-aggregate-receipt.json';native=json.loads(agg.read_text());assert native['result']=='genuine_aggregate_not_yet_final_link_or_model_qualified' and native['command']['exit_code']==0 and native['command']['owned_descendants_settled']
  r.update(environment_file={'path':str(F),'sha256':sha(F)},aggregate_receipt={'path':str(agg),'sha256':sha(agg)},native_aggregate_command=native['command'],native_output=native['output'],runtime_closure_count=len(native['plan']['inputs']),canonical_tool=native['tool'])
except BaseException as e:r['passed']=False;r['failure']={'type':type(e).__name__,'message':str(e)};rc=1
finally:
 r['seconds']=time.monotonic()-began;r['finish_utc']=datetime.datetime.now(datetime.timezone.utc).isoformat();save();print(json.dumps(r));print('RECEIPT_SHA256',sha(out))
raise SystemExit(0 if r['passed'] else rc or 1)
