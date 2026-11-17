# SPDX-License-Identifier: Apache-2.0
"""One Root-released Context candidate process using the unchanged native job owner."""
import argparse,hashlib,importlib.util,json,os,subprocess,time
from pathlib import Path
ROOT=Path(__file__).resolve().parents[5]
OWNER=ROOT/'tools/retrieval-train/training_execution.py'
OWNER_SHA='d363913b2424321822f9eee189f999eabb807a43c7d0c46fab567531dd1bd975'
SELECTOR='inference_engine::pool::arbiter::qualification::context::selected_context_fp32_native_numeric_and_resource_candidate'
PROFILES={'arbiter-context-qwen3-17b':{'cpu':4,'reservation':16*1024**3,'host':32*1024**3,'seconds':1200},'arbiter-context-qwen3-4b':{'cpu':16,'reservation':40*1024**3,'host':48*1024**3,'seconds':1800}}
def digest(path):
 with Path(path).open('rb') as f:return hashlib.file_digest(f,'sha256').hexdigest()
def remaining_seconds(deadline,now):
 value=deadline-now
 if value<=0:raise TimeoutError('Original whole-phase deadline expired')
 return value
def natural(value):
 return bool(value is not None and value['exit_code']==0 and not value['timed_out'] and not value['log_oversized'] and value['owned_descendants_settled'] and not value['owned_job_forced_termination'] and not value['unexpected_third_process'] and value['owned_total_processes']==1 and not value['owned_job_final_member_pids'] and value['owned_job_final_active_processes']==0 and len(value['retained_member_exits'])==1 and all(x['exit_observed'] and x['exit_code']==0 for x in value['retained_member_exits']))
def main():
 started=time.monotonic();p=argparse.ArgumentParser();p.add_argument('--packet',type=Path,required=True);p.add_argument('--packet-sha256',required=True);p.add_argument('--execute-reviewed',action='store_true');a=p.parse_args()
 if os.name!='nt' or digest(a.packet)!=a.packet_sha256:raise ValueError('Exact reviewed Windows packet required')
 packet=json.loads(a.packet.read_bytes());scope=PROFILES[packet['model']];deadline=started+scope['seconds']
 if packet.get('schema')!='airk.private-context-native-execution/1' or packet.get('limits')!={'wall_seconds':scope['seconds'],'host_committed_bytes':scope['host']} or packet.get('candidate_memory_reservation_bytes')!=scope['reservation'] or packet.get('cpu_fit')!={'minimum_physical_cores':scope['cpu'],'intra_threads':scope['cpu'],'inter_threads':1,'sequential':True,'spinning':False}:raise ValueError('Exact predeclared Context scope required')
 if digest(OWNER)!=OWNER_SHA or packet['runner_sha256']!=digest(__file__):raise ValueError('Unchanged job owner and reviewed runner required')
 pins=dict(packet['pins']);pins[str(OWNER)]=OWNER_SHA;binary=Path(packet['binary']['path']).resolve();pins[str(binary)]=packet['binary']['sha256'];pins[str(Path(__file__).resolve())]=packet['runner_sha256']
 def verify():
  if any(digest(path)!=pin for path,pin in pins.items()):raise ValueError('Native source/image/profile/assets/Cargo provenance drift')
 verify();remaining_seconds(deadline,time.monotonic())
 listed=subprocess.run([str(binary),'--exact',SELECTOR,'--ignored','--list'],capture_output=True,text=True,timeout=min(10,remaining_seconds(deadline,time.monotonic())),check=True)
 if listed.stdout.count(SELECTOR+': test')!=1:raise ValueError('Exact selector must select one native test')
 work=Path(packet['output']).resolve()
 if work.exists() or not work.is_relative_to(ROOT/'artifacts'):raise ValueError('Fresh private output required')
 env=dict(os.environ);env.update(packet['environment'])
 if any(env.get(k)!=v for k,v in {'CUDA_VISIBLE_DEVICES':'','OMP_NUM_THREADS':'1','MKL_NUM_THREADS':'1','OPENBLAS_NUM_THREADS':'1'}.items()):raise ValueError('Native CPU-only/session-owned threading differs')
 descriptor=Path(env['AIRK_CONTEXT_NATIVE_INPUT']).resolve()
 if pins.get(str(descriptor))!=env['AIRK_CONTEXT_NATIVE_INPUT_SHA256']:raise ValueError('Native Rust descriptor pin missing')
 capture=work/'capture.json';env['AIRK_CONTEXT_NATIVE_OUTPUT']=str(capture);argv=[str(binary),'--exact',SELECTOR,'--ignored','--nocapture','--test-threads=1']
 if not a.execute_reviewed:
  print(json.dumps({'preflight_passed':True,'selected':1,'model':packet['model'],'command':argv,'packet_sha256':a.packet_sha256,'model_executed':False}));return
 if packet.get('status')!='ROOT_RELEASED':raise ValueError('Separate Root release absent')
 spec=importlib.util.spec_from_file_location('unchanged_native_job_owner',OWNER);owner=importlib.util.module_from_spec(spec);spec.loader.exec_module(owner)
 work.mkdir();(work/'stdin.txt').write_bytes(b'');(work/'request.json').write_text(json.dumps({'packet_sha256':a.packet_sha256,'command':argv,'environment':packet['environment'],'pins':pins,'limits':packet['limits'],'selector_list':listed.stdout},indent=2)+'\n')
 def suspended(api,job,info,deadline):
  members,active,total=owner.job_snapshot(api,job)
  if members!=[int(info.pid)] or active!=1 or total!=1:raise ValueError('One exact suspended native member required')
  image=owner.image_proof(api,info.process,packet['binary'])
  if time.monotonic()>=deadline:raise ValueError('Original phase deadline expired before resume')
  return {'members':members,'active':active,'total':total,'image':image}
 owned=None;failure=None;drift=[];complete=False
 try:
  with (work/'native.log').open('xb') as log,(work/'stdin.txt').open('rb') as stdin:owned=owner.run_windows_owned_training(argv,ROOT,log,remaining_seconds(deadline,time.monotonic()),env,stdin,scope['host'],before_resume=suspended)
  value=json.loads(capture.read_bytes());resource=value.get('resource',{})
  if value.get('complete') is not True or value.get('semantic_or_CAL_or_publication_qualified') is not False or value.get('combined_service_admission_verified') is not False or len(value.get('numeric_rows',[]))!=3 or resource.get('measurement_complete') is not True or resource.get('completed_samples')!=20 or len(resource.get('samples_us',[]))!=20 or resource.get('candidate_numeric_resource_passed') is not True:raise ValueError('Complete scoped3shape/20sample capture missing')
  complete=natural(owned)
 except BaseException as error:
  failure={'type':type(error).__name__,'message':str(error),'owned_cleanup':getattr(error,'owned_cleanup',None)}
  raise
 finally:
  try:drift=[path for path,pin in pins.items() if digest(path)!=pin]
  except (OSError,ValueError) as error:drift=['post-pin-verification-failed: '+str(error)]
  expired=time.monotonic()>=deadline;complete=complete and not drift and not expired
  result={'schema':'airk.private-context-native-execution-receipt/1','complete':complete,'model':packet['model'],'packet_sha256':a.packet_sha256,'owned':owned,'failure':failure,'elapsed_seconds':time.monotonic()-started,'post_input_drift':drift,'quality_or_CAL_or_combined_or_release_qualified':False,'no_automatic_retry':True,'other_processes_touched':False,'whole_phase_deadline_not_refreshed':True,'whole_phase_expired':expired}
  if capture.is_file():result['capture_sha256']=digest(capture)
  (work/'execution.json').write_text(json.dumps(result,indent=2)+'\n');print(json.dumps({'complete':complete,'model':packet['model'],'elapsed_seconds':result['elapsed_seconds'],'capture_sha256':result.get('capture_sha256')}),flush=True)
 if not complete:raise SystemExit(1)
if __name__=='__main__':main()
