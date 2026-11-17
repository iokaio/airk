# SPDX-License-Identifier: Apache-2.0
"""Exactly two owning prepare/profile metadata selectors under retained actual process custody."""
import argparse,datetime,importlib.util,json,os,sys,time
from pathlib import Path
sys.dont_write_bytecode=True
BASE=Path('D:/code/Github/airk/artifacts/v190/final-plan/exec-20261116T052942Z/gpu-maintained-owner-proposal-r6/scripts/cuda-native-execution.py')
BASE_SHA='3c8b58d663c7aa016c2c68ffbe8a9fb65e48d00e875139ac02b25946339659fe'
ANCHOR=Path('C:/Users/tdotn/AppData/Local/airk-final-exec-20261116T052942Z')
PREFIX='inference_engine::pool::arbiter::qualification::software_v2::cuda_qualification::'
SELECTORS={'request':PREFIX+'emit_cuda_component_probe_request','metadata':PREFIX+'emit_cuda_fp32_probe_metadata'}
def owner():
 import hashlib
 with BASE.open('rb') as f:
  if hashlib.file_digest(f,'sha256').hexdigest()!=BASE_SHA:raise ValueError('reviewed original custody owner changed')
 s=importlib.util.spec_from_file_location('three_selector_original_owner',BASE);m=importlib.util.module_from_spec(s);s.loader.exec_module(m);return m
def output_paths(plan,*,before):
 e=plan['environment'];kind=next((k for k,v in SELECTORS.items() if v==plan['selector']),None)
 if kind is None:raise ValueError('only dedicated prepare/profile metadata selectors allowed')
 if kind=='request':
  if e.get('AIRK_CUDA_REQUEST_KIND')!='prepare':raise ValueError('request kind must be prepare only')
  paths=[Path(e['AIRK_CUDA_REQUEST_OUTPUT'])];anchor=paths[0].parent
 else:
  directory=Path(e['AIRK_CUDA_METADATA_DIRECTORY']);anchor=directory.parent
  if before and directory.exists():raise ValueError('native metadata directory must be fresh absent')
  paths=[directory/x for x in ['profile.json','native-input.json','probe-request.json','commands.json','metadata-receipt.json']]
 if [str(p) for p in paths]!=plan['output_pins_expected']:raise ValueError('exact native metadata output paths differ')
 if not anchor.is_absolute() or not anchor.is_relative_to(ANCHOR) or anchor.resolve(strict=True)!=anchor or not anchor.is_dir():raise ValueError('existing protected output ancestor required')
 for p in [anchor,*anchor.parents]:
  if p.is_symlink() or getattr(p.lstat(),'st_file_attributes',0)&0x400:raise ValueError('output ancestor alias refused')
 if kind=='metadata' and not before:
  if directory.resolve(strict=True)!=directory or not directory.is_dir() or directory.is_symlink() or getattr(directory.lstat(),'st_file_attributes',0)&0x400:raise ValueError('native metadata output directory changed')
 return kind,paths

def preflight(plan,b,*,before):
 if plan.get('schema')!='airk.private.cuda-profile-nn0-plan/1' or plan.get('human_sid')!=b.SID or plan.get('limits')!={'wall_seconds':120,'host_committed_bytes':8<<30,'log_bytes':64<<20}:raise ValueError('exact prepare/profile metadata plan/bounds required')
 if plan['wrapper']['sha256']!=b.digest(__file__) or plan['custody_owner']!={'path':str(BASE),'bytes':BASE.stat().st_size,'sha256':BASE_SHA}:raise ValueError('reviewed exact parent/custody sources required')
 kind,paths=output_paths(plan,before=before);e=plan['environment'];common={'AIRK_CUDA_DEADLINE_MS','AIRK_PRIVATE_CENSUS_BASE','TEMP','TMP','TMPDIR'}
 keys={'request':{'AIRK_CUDA_REQUEST_KIND','AIRK_SOFTWARE_V2_NATIVE_INPUT','AIRK_SOFTWARE_V2_NATIVE_INPUT_SHA256','AIRK_CUDA_RUNTIME_MANIFEST','AIRK_CUDA_RUNTIME_MANIFEST_SHA256','AIRK_CUDA_HOST_LIMIT_BYTES','AIRK_CUDA_DEVICE_LIMIT_BYTES','AIRK_CUDA_COLD_BYTES','AIRK_CUDA_WARM_BYTES','AIRK_CUDA_HEADROOM_BYTES','AIRK_CUDA_JOB_LIMIT_BYTES','AIRK_CUDA_REQUEST_OUTPUT'},'metadata':{'AIRK_CUDA_QUALIFICATION_INPUT','AIRK_CUDA_QUALIFICATION_INPUT_SHA256','AIRK_CUDA_METADATA_DIRECTORY'}}
 if set(e)-common-keys[kind] or e.get('AIRK_CUDA_DEADLINE_MS')!='120000' or any(not isinstance(k,str) or not isinstance(v,str) or '\0' in k+v for k,v in e.items()):raise ValueError('metadata-only explicit environment required')
 for key,value in e.items():
  if key.endswith('_SHA256') and key[:-7] in e:
   path=str(Path(e[key[:-7]]))
   if plan['input_seals'].get(path)!=value:raise ValueError('native environment input pin omitted from original seals')

 if before and any(p.exists() for p in paths):raise ValueError('native create-new outputs already exist')
 b.source_image(plan)
 required=[plan[k] for k in ['wrapper','custody_owner','native_image','image_packet','native_build_receipt','parent_python','output_guard','output_guard_lock']]
 required +=[b.pin(Path(plan['ownership_helper_root'])/name) for name in b.HELPERS]
 for row in required:
  b.pinned(row)
  if plan['input_seals'].get(row['path'])!=row['sha256']:raise ValueError('required original metadata/source/image input seal missing')
 if plan['output_guard']['sha256']!='c2f9c3e305106469d6a58699eaa36ec2afed7a6cd4be63275498fd78af2d3f4a' or plan['output_guard_lock']['sha256']!='549be258fd21be030f994f6da7f74146a9e9edd3bb611bee83305d9a4deade6d':raise ValueError('same reviewed protected output guard required')
 lock=b.read(plan['output_guard_lock']['path']);repository=Path(plan['repository_root'])
 if repository!=Path('D:/code/Github/airk') or len(plan['input_seals'])>16380:raise ValueError('bounded same repository input closure required')
 for name,key in [('provision-static-onnx-runtime.py','cpu_owner_sha256'),('static-onnx-runtime-lock.json','cpu_lock_sha256')]:
  if plan['input_seals'].get(str(repository/'scripts'/name))!=lock[key]:raise ValueError('protected output guard source omitted')
 for path,sha in plan['input_seals'].items():
  if b.digest(b.regular(path))!=sha:raise ValueError('original metadata/input seal changed')
 return kind,paths

def native_output(kind,paths,plan,receipt,b):
 if kind=='request':
  value=b.read(paths[0])
  if value.get('kind')!='prepare' or value.get('schema')!='airk.private.cuda-component-probe-request/1':raise ValueError('owning Rust output must remain a prepare request')
  pins=[b.pin(paths[0])]
 else:
  directory=paths[0].parent;profile=b.read(paths[0]);cal=[asset for asset in profile.get('assets',[]) if asset.get('role')=='calibration']
  if len(cal)!=1 or not isinstance(cal[0].get('name'),str) or Path(cal[0]['name']).name!=cal[0]['name'] or cal[0]['name'] in ('.','..') or '/' in cal[0]['name'] or '\\' in cal[0]['name']:raise ValueError('one native emitted flat calibration asset required')
  actual=b.census(directory);expected={str(p) for p in paths}|{str(directory/cal[0]['name'])}
  if {r['path'] for r in actual}!=expected or len(actual)!=6:raise ValueError('exact native six-file metadata roster required')
  pins=[b.pin(p) for p in paths]+[b.pin(directory/cal[0]['name'])];cal_pin=pins[-1]
  if (cal_pin['bytes'],cal_pin['sha256'])!=(cal[0].get('bytes'),cal[0].get('sha256')):raise ValueError('native calibration asset pin differs')
  v=b.read(paths[-1]);e=plan['environment'];source={'path':e['AIRK_CUDA_QUALIFICATION_INPUT'],'sha256':e['AIRK_CUDA_QUALIFICATION_INPUT_SHA256']}
  if v.get('schema')!='airk.private.cuda-probe-metadata/1' or v.get('qualified') is not False or v.get('model_requests_dispatched') is not False or v.get('forward_count_observed','missing') is not None or not b.core_pin_matches(v.get('request'),source) or not b.core_pin_matches(v.get('probe_request'),pins[2]) or v.get('profile')!=profile.get('manifest') or type(v.get('elapsed_us')) is not int or not 0<=v['elapsed_us']<120000000:raise ValueError('native metadata receipt/pins/scope differs')
  native_input=b.read(paths[1]);probe=b.read(paths[2])
  if not b.core_pin_matches(native_input.get('profile'),pins[0]) or probe.get('kind')!='probe' or probe.get('schema')!='airk.private.cuda-component-probe-request/1' or not b.core_pin_matches(probe.get('gpu_input'),pins[1]):raise ValueError('native emitted opaque profile/input/probe links differ')
 if any(p['bytes']>16<<20 for p in pins):raise ValueError('bounded native metadata outputs required')
 return pins

def run(plan_path,expected):
 started=time.monotonic();b=owner();deadline=started+min(120,(b.CAMPAIGN-datetime.datetime.now(datetime.timezone.utc)).total_seconds());pp=b.regular(plan_path)
 if b.digest(pp)!=expected:raise ValueError('reviewed actual metadata plan differs')
 plan=b.read(pp);kind,paths=preflight(plan,b,before=True);parent,primitive=b.helpers(plan)
 if parent.worker.windows_sid()!=b.SID or b.pin(sys.executable)!=plan['parent_python']:raise ValueError('actual same-human parent interpreter required')
 if time.monotonic()>=deadline:raise TimeoutError('original metadata phase expired before launch')
 guard=b.load(plan['output_guard']['path'],'three_selector_existing_output_guard');lock=b.read(plan['output_guard_lock']['path']);cpu=guard.cpu_owner(lock);out=Path(plan['output_root']);copy=json.loads(json.dumps(lock));copy['build_path']['fresh_child']=out.name;policy=guard.identity_and_parent(cpu,out,copy);out.mkdir();(out/'temp').mkdir();log=out/'stdout-stderr.log';argv=[plan['native_image']['path'],'--exact',plan['selector'],'--ignored','--nocapture','--test-threads=1'];env=dict(os.environ)
 for key in list(env):
  if key.upper().startswith(('AIRK_','ORT_','PYTHON','CUDA_','CUDNN_')):env.pop(key)
 env.update(plan['environment']);env.update({'PATH':'C:/Windows/System32;C:/Windows','TEMP':str(out/'temp'),'TMP':str(out/'temp'),'TMPDIR':str(out/'temp')})
 receipt={'schema':'airk.private.owned-cuda-profile-nn0/1','plan':b.pin(pp),'selector':plan['selector'],'kind':kind,'command':argv,'input_seals':dict(plan['input_seals'],**{str(pp):expected}),'execution_user_sid':b.SID,'parent_pid':os.getpid(),'parent_python':b.pin(sys.executable),'output_policy':policy,'complete':False,'failure':None,'post_input_drift':[],'whole_phase_deadline_not_refreshed':True,'whole_phase_expired':False,'activation':False,'qualification':False}
 try:
  def authenticate(api,job,info,child_deadline):return parent.suspended_authentication(api,info,deadline,child_deadline,b.SID,plan['native_image'],primitive.image_proof)
  with (out/'stdin.txt').open('xb') as f:f.flush()
  remaining=deadline-time.monotonic()
  if remaining<=0:raise TimeoutError('original metadata deadline expired before child dispatch')
  with (out/'stdin.txt').open('rb') as initial,log.open('xb') as stdout:receipt['owned']=primitive.run_windows_owned_training(argv,Path(plan['repository_root']),stdout,remaining,env,initial,8<<30,before_resume=authenticate,absolute_deadline=deadline)
  if not parent.settled(receipt['owned']) or type(receipt['owned'].get('peak_host_committed_bytes')) is not int or not 0<receipt['owned']['peak_host_committed_bytes']<=8<<30:raise ValueError('actual native metadata child not naturally settled within original bounds')
  receipt['outputs']=native_output(kind,paths,plan,receipt,b);preflight(plan,b,before=False)
  for path,sha in receipt['input_seals'].items():
   if b.digest(b.regular(path))!=sha:receipt['post_input_drift'].append(path)
  if receipt['post_input_drift']:raise ValueError('post-execution original input drift')
  receipt['complete']=True
 except BaseException as exc:
  receipt['failure']={'type':type(exc).__name__,'message':str(exc)};receipt['complete']=False
  if hasattr(exc,'owned_cleanup'):receipt['actual_cleanup_failure']=exc.owned_cleanup
  raise
 finally:
  original=sys.exc_info()[1];b.finish_observation(receipt,started,deadline,log)
  with (out/'execution.json').open('x') as f:json.dump(receipt,f,indent=2);f.write('\n')
  if original is None and receipt['failure'] is not None:raise ValueError(receipt['failure'])
 return receipt

def main():
 ap=argparse.ArgumentParser();ap.add_argument('--execute-reviewed-nn0',action='store_true');ap.add_argument('--approved-script-sha256',required=True);ap.add_argument('--plan',type=Path,required=True);ap.add_argument('--plan-sha256',required=True);a=ap.parse_args();b=owner()
 if not a.execute_reviewed_nn0 or b.digest(__file__)!=a.approved_script_sha256:raise ValueError('explicit reviewed metadata-only parent required')
 run(a.plan,a.plan_sha256)
if __name__=='__main__':main()
