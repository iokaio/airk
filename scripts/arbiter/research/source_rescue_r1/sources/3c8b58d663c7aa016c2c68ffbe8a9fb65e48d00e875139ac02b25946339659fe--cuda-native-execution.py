# SPDX-License-Identifier: Apache-2.0
"""One actual NN0 package projection child; no model, resource or training alias."""
import argparse,datetime,hashlib,importlib.util,json,math,os,sys,time
from pathlib import Path
sys.dont_write_bytecode=True
SID='S-1-5-21-3678370870-3858642417-826968105-1002'
SELECTOR='inference_engine::pool::arbiter::qualification::software_v2::cuda_qualification::emit_cuda_runtime_package_projection_from_held_native_closure'
HELPERS={'reference_owned.py':'2d7a31a64eaba745e32ed83adfbeedb75983ab6f9b5c4cc82714e9371c1895a1','reference_parent.py':'775e32da732446919789490ca5a30b0b3d863f570b7bf1852d56e9e7c7391354','reference_worker.py':'9c04be28ef06c0a06b8e2cffd4c274348ff141046114cc493d725ce527b694c7'}
CAMPAIGN=datetime.datetime(2026,11,19,5,29,42,tzinfo=datetime.timezone.utc)
def digest(p):
 with Path(p).open('rb') as f:return hashlib.file_digest(f,'sha256').hexdigest()
def regular(p):
 p=Path(p)
 if not p.is_absolute() or p.resolve(strict=True)!=p:raise ValueError('canonical absolute regular input required')
 for x in [p,*p.parents]:
  st=x.lstat()
  if x.is_symlink() or getattr(st,'st_file_attributes',0)&0x400:raise ValueError('aliased input refused')
 if not p.is_file():raise ValueError('regular input required')
 return p
def pin(p):p=regular(p);return {'path':str(p),'bytes':p.stat().st_size,'sha256':digest(p)}
def pinned(row):
 p=regular(row['path'])
 if pin(p)!=row:raise ValueError('exact byte pin differs: '+str(p))
 return p
def read(p,limit=16<<20):
 p=regular(p)
 if p.stat().st_size>limit:raise ValueError('bounded metadata required')
 return json.loads(p.read_bytes())
def load(p,name):
 spec=importlib.util.spec_from_file_location(name,p);m=importlib.util.module_from_spec(spec);sys.modules[name]=m;spec.loader.exec_module(m);return m

def helpers(plan):
 base=Path(plan['ownership_helper_root'])
 for name,sha in HELPERS.items():
  if digest(regular(base/name))!=sha:raise ValueError('original ownership primitive source differs')
 sys.path.insert(0,str(base));parent=load(base/'reference_parent.py','cuda_nn0_existing_parent');owner=load(base/'reference_owned.py','cuda_nn0_existing_owned')
 if digest(regular(parent.worker.__file__))!=HELPERS['reference_worker.py']:raise ValueError('ownership dependency shadowed')
 return parent,owner

def source_image(plan):
 packet=read(pinned(plan['image_packet']));build=read(pinned(plan['native_build_receipt']),32<<20)
 if packet.get('schema')!='airk.preserved-native-test-images/1' or not core_pin_matches(packet.get('actual_build_receipt'),plan['native_build_receipt']) or packet.get('native_group')!=plan['native_group'] or build.get('passed') is not True or build.get('inputs_unchanged') is not True or build['groups']['native']['sha256']!=plan['native_group']:raise ValueError('actual preserved native image/source build differs')
 rows=[r for r in packet['images'] if r['binary_id']=='airk-inference']
 if len(rows)!=1:raise ValueError('exact preserved Inference image required')
 row=rows[0]
 if {k:row[k] for k in ['path','bytes','sha256']}!=plan['native_image'] or row['original_file_owner_sid']!=SID or row['preserved_file_owner_sid']!=SID:raise ValueError('native image/custody differs')
 pinned(plan['native_image'])
 required=plan['required_native_sources']
 if not {'crates/airk-inference/src/inference_engine/pool/arbiter/qualification/software_v2/cuda_qualification.rs','crates/airk-retrieval/src/arbiter/runtime/cuda/windows.rs'}.issubset(required) or any(build['groups']['native']['files'].get(k)!=v for k,v in required.items()):raise ValueError('native package source closure differs')
 return row

def census(root):
 root=Path(root)
 if not root.is_absolute() or root.resolve(strict=True)!=root:raise ValueError('canonical runtime source root required')
 rows=[];stack=[root]
 while stack:
  directory=stack.pop()
  st=directory.lstat()
  if directory.is_symlink() or getattr(st,'st_file_attributes',0)&0x400:raise ValueError('runtime directory alias refused')
  for child in directory.iterdir():
   st=child.lstat()
   if child.is_symlink() or getattr(st,'st_file_attributes',0)&0x400:raise ValueError('runtime file alias refused')
   if child.is_dir():stack.append(child)
   elif child.is_file():rows.append(pin(child))
   else:raise ValueError('special runtime input refused')
   if len(rows)>128:raise ValueError('runtime file census exceeds128')
 return sorted(rows,key=lambda r:r['path'])

def preflight(plan):
 if plan.get('schema')!='airk.private.cuda-package-projection-plan/1' or plan.get('human_sid')!=SID or plan.get('limits')!={'wall_seconds':120,'host_committed_bytes':8<<30,'log_bytes':64<<20}:raise ValueError('exact NN0 plan/bounds required')
 if plan['wrapper']['sha256']!=digest(__file__):raise ValueError('actual reviewed parent source differs')
 pinned(plan['wrapper']);source_image(plan)
 for key in ['native_input','descriptor','image_packet','native_build_receipt','output_guard','output_guard_lock','parent_python']:pinned(plan[key])
 if plan['output_guard']['sha256']!='c2f9c3e305106469d6a58699eaa36ec2afed7a6cd4be63275498fd78af2d3f4a' or plan['output_guard_lock']['sha256']!='549be258fd21be030f994f6da7f74146a9e9edd3bb611bee83305d9a4deade6d':raise ValueError('reviewed same-human protected output guard required')
 if census(plan['runtime_root'])!=plan['runtime_files']:raise ValueError('complete exact runtime byte census differs')
 descriptor=Path(plan['descriptor']['path'])
 if descriptor.parent!=Path(plan['runtime_root']) or descriptor.name!='airk-cuda-runtime.json' or plan['descriptor'] not in plan['runtime_files']:raise ValueError('native descriptor/root pin differs')
 if Path(plan['output_root']).is_relative_to(Path(plan['runtime_root'])):raise ValueError('fresh execution output must be separate from sealed runtime source')
 required=[plan[k] for k in ['parent_python','wrapper','native_image','native_input','descriptor','image_packet','native_build_receipt','output_guard','output_guard_lock']]+plan['runtime_files']
 required +=[pin(Path(plan['ownership_helper_root'])/name) for name in HELPERS]
 lock=read(plan['output_guard_lock']['path']);repository=Path(plan['repository_root'])
 if repository!=Path('D:/code/Github/airk'):raise ValueError('actual reviewed repository root required')
 required +=[{'path':str(repository/'scripts'/name),'sha256':lock[key]} for name,key in [('provision-static-onnx-runtime.py','cpu_owner_sha256'),('static-onnx-runtime-lock.json','cpu_lock_sha256')]]
 seals=plan['input_seals']
 if any(seals.get(row['path'])!=row['sha256'] for row in required) or len(seals)>16380:raise ValueError('complete original input seals required')
 for path,sha in seals.items():
  if digest(regular(path))!=sha:raise ValueError('original input seal differs')
 return plan

def core_pin_matches(actual,expected):
 return isinstance(actual,dict) and actual.get('path')==expected['path'] and actual.get('sha256')==expected['sha256']
def emission_facts(value,plan,output,owned):
 if value.get('schema')!='airk.private.cuda-runtime-package-projection-emission/1' or not core_pin_matches(value.get('input'),plan['native_input']) or not core_pin_matches(value.get('descriptor'),plan['descriptor']) or not core_pin_matches(value.get('projection'),output) or value.get('pid')!=owned.get('owned_leader_pid') or value.get('execution_user')!=SID:raise ValueError('actual inner native emission identity differs')
 if any(value.get(k) is not False for k in ['model_requests_dispatched','runtime_admission_dispatched','activation','qualification']) or value.get('forward_count_observed','missing') is not None or type(value.get('elapsed_us')) is not int or not 0<=value['elapsed_us']<120000000:raise ValueError('metadata-only native emission scope/deadline differs')

def verify_execution(path,sha,projection_pin=None):
 path=regular(path)
 if digest(path)!=sha:raise ValueError('reviewed owned NN0 receipt differs')
 v=read(path);plan=read(pinned(v['plan']));preflight(plan);parent,_=helpers(plan)
 if v.get('schema')!='airk.private.owned-cuda-package-projection/1' or v.get('complete') is not True or v.get('failure') is not None or v.get('execution_user_sid')!=SID or v.get('whole_phase_deadline_not_refreshed') is not True or v.get('whole_phase_expired') is not False or v.get('post_input_drift')!=[] or v.get('activation') is not False or v.get('qualification') is not False or not parent.settled(v.get('owned',{})):raise ValueError('actual naturally settled original NN0 ownership required')
 if type(v['elapsed_seconds']) not in (int,float) or not math.isfinite(v['elapsed_seconds']) or not 0<=v['elapsed_seconds']<120 or type(v['owned'].get('peak_host_committed_bytes')) is not int or not 0<v['owned']['peak_host_committed_bytes']<=8<<30:raise ValueError('original NN0 whole-phase/committed bound exceeded')
 expected=[plan['native_image']['path'],'--exact',SELECTOR,'--ignored','--nocapture','--test-threads=1']
 if v.get('parent_python')!=plan['parent_python'] or type(v.get('parent_pid')) is not int or v['parent_pid']<=0 or v.get('command')!=expected or v.get('input_seals')!=dict(plan['input_seals'],**{v['plan']['path']:v['plan']['sha256']}):raise ValueError('actual original selector/input seals differ')
 suspend=v['owned'].get('suspended_assignment',{})
 if suspend.get('pid')!=v['owned'].get('owned_leader_pid') or suspend.get('execution_user_sid')!=SID or suspend.get('image')!={k:plan['native_image'][k] for k in ['path','sha256']}:raise ValueError('suspended original native PID/SID/image proof differs')
 for key in ['projection','native_emission','stdout']:pinned(v[key])
 if v['projection']['bytes']>1<<20 or v['native_emission']['bytes']>1<<20 or v['stdout']['bytes']>64<<20:raise ValueError('original NN0 output/log retention cap exceeded')
 if projection_pin is not None and v['projection']!=projection_pin:raise ValueError('native projection output differs')
 emission_facts(read(v['native_emission']['path']),plan,v['projection'],v['owned'])
 if digest(path)!=sha:raise ValueError('owned NN0 receipt changed during verification')
 return plan,v

def finish_observation(receipt,started,deadline,log,clock=time.monotonic,hash_file=pin):
 try:
  if log.exists():
   receipt['stdout']=hash_file(log)
   if receipt['stdout'].get('bytes',0)>64<<20:raise ValueError('original64MiB stdout cap exceeded during retention')
 except BaseException as exc:
  receipt['retention_failure']={'type':type(exc).__name__,'message':str(exc)};receipt['complete']=False
  receipt['failure']=receipt['failure'] or receipt['retention_failure']
 now=clock();receipt['elapsed_seconds']=now-started;receipt['whole_phase_expired']=now>=deadline
 if receipt['whole_phase_expired']:receipt['complete']=False;receipt['failure']=receipt['failure'] or {'type':'TimeoutError','message':'original NN0 phase expired during final retention'}

def run(plan_path,expected):
 started=time.monotonic();deadline=started+min(120,(CAMPAIGN-datetime.datetime.now(datetime.timezone.utc)).total_seconds())
 pp=regular(plan_path)
 if digest(pp)!=expected:raise ValueError('reviewed package plan differs')
 plan=read(pp)
 if Path(plan['parent_python']['path'])!=Path(sys.executable) or pin(sys.executable)!=plan['parent_python']:raise ValueError('actual parent interpreter bytes differ')
 preflight(plan);parent,owner=helpers(plan)
 if parent.worker.windows_sid()!=SID:raise ValueError('actual parent SID differs from logged-in human')
 if time.monotonic()>=deadline:raise TimeoutError('original NN0 phase expired during initial authentication')
 guard=load(plan['output_guard']['path'],'nn0_existing_output_guard');lock=read(plan['output_guard_lock']['path']);cpu=guard.cpu_owner(lock);out=Path(plan['output_root']);guard_lock=json.loads(json.dumps(lock));guard_lock['build_path']['fresh_child']=out.name;policy=guard.identity_and_parent(cpu,out,guard_lock);out.mkdir();(out/'temp').mkdir()
 output=out/'native-projection.json';log=out/'stdout-stderr.log';argv=[plan['native_image']['path'],'--exact',SELECTOR,'--ignored','--nocapture','--test-threads=1'];environment=dict(os.environ)
 for key in list(environment):
  if key.upper().startswith(('AIRK_','ORT_','PYTHON','CUDA_','CUDNN_')):environment.pop(key)
 environment.update({'PATH':'C:/Windows/System32;C:/Windows','TEMP':str(out/'temp'),'TMP':str(out/'temp'),'TMPDIR':str(out/'temp'),'AIRK_CUDA_QUALIFICATION_INPUT':plan['native_input']['path'],'AIRK_CUDA_QUALIFICATION_INPUT_SHA256':plan['native_input']['sha256'],'AIRK_CUDA_QUALIFICATION_OUTPUT':str(output)})
 receipt={'schema':'airk.private.owned-cuda-package-projection/1','plan':pin(pp),'execution_user_sid':SID,'parent_pid':os.getpid(),'parent_python':pin(sys.executable),'command':argv,'input_seals':dict(plan['input_seals'],**{str(pp):expected}),'whole_phase_deadline_not_refreshed':True,'whole_phase_expired':False,'complete':False,'failure':None,'post_input_drift':[],'activation':False,'qualification':False,'output_policy':policy}
 try:
  if time.monotonic()>=deadline:raise TimeoutError('original NN0 phase expired before native child')
  def authenticate(api,job,info,child_deadline):return parent.suspended_authentication(api,info,deadline,child_deadline,SID,plan['native_image'],owner.image_proof)
  with (out/'stdin.txt').open('xb') as f:f.flush()
  with (out/'stdin.txt').open('rb') as initial,log.open('xb') as stdout:receipt['owned']=owner.run_windows_owned_training(argv,Path(plan['repository_root']),stdout,deadline-time.monotonic(),environment,initial,8<<30,before_resume=authenticate,absolute_deadline=deadline)
  if not parent.settled(receipt['owned']) or receipt['owned']['peak_host_committed_bytes']>8<<30:raise ValueError('actual original native child did not naturally settle within bounds')
  receipt['projection']=pin(output)
  if receipt['projection']['bytes']>1<<20:raise ValueError('native projection exceeds1MiB')
  receipt['native_emission']=pin(output.with_suffix('.receipt.json'));emission_facts(read(receipt['native_emission']['path']),plan,receipt['projection'],receipt['owned'])
  for path,sha in receipt['input_seals'].items():
   if digest(regular(path))!=sha:receipt['post_input_drift'].append(path)
  preflight(plan)
  if receipt['post_input_drift']:raise ValueError('actual NN0 source/input/runtime drift')
  receipt['complete']=True
 except BaseException as exc:
  receipt['failure']={'type':type(exc).__name__,'message':str(exc)};receipt['complete']=False
  if hasattr(exc,'owned_cleanup'):receipt['actual_cleanup_failure']=exc.owned_cleanup
  raise
 finally:
  original_error=sys.exc_info()[1];finish_observation(receipt,started,deadline,log)
  with (out/'execution.json').open('x') as f:json.dump(receipt,f,indent=2);f.write('\n')
  if original_error is None and receipt['failure'] is not None:raise ValueError(receipt['failure'])
 return receipt

def main():
 ap=argparse.ArgumentParser();ap.add_argument('--execute-reviewed-nn0',action='store_true');ap.add_argument('--plan',type=Path,required=True);ap.add_argument('--plan-sha256',required=True);ap.add_argument('--approved-script-sha256',required=True);a=ap.parse_args()
 if not a.execute_reviewed_nn0 or digest(__file__)!=a.approved_script_sha256:raise ValueError('explicit reviewed NN0 wrapper required')
 run(a.plan,a.plan_sha256)
if __name__=='__main__':main()
