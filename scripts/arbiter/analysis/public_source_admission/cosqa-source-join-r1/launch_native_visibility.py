# SPDX-License-Identifier: Apache-2.0
"""Reviewed private one-child visibility custody; no execution without --execute-reviewed."""
import argparse,hashlib,importlib.util,json,os,re,sys,time
from pathlib import Path
sys.dont_write_bytecode=True
LIMITS={'wall_seconds':120,'host_committed_bytes':4<<30,'log_bytes':64<<20,'report_bytes':512<<20,'cleanup_seconds':5}
SELECTOR='inference_owner::evaluation_tests::train_units::public_human_visibility::emit_public_human_native_visible_admission'
def digest(path):
 with Path(path).open('rb') as f:return hashlib.file_digest(f,'sha256').hexdigest()
def regular(path):
 p=Path(path)
 if not p.is_absolute() or p.resolve(strict=True)!=p or not p.is_file():raise ValueError('canonical regular input required')
 for x in [p,*p.parents]:
  st=x.lstat()
  if x.is_symlink() or getattr(st,'st_file_attributes',0)&0x400:raise ValueError('input alias refused')
 return p
def pin(value):
 p=regular(value['path'])
 if digest(p)!=value['sha256']:raise ValueError('input changed: '+str(p))
 return p
def load(value,name):
 p=pin(value);sys.path.insert(0,str(p.parent));spec=importlib.util.spec_from_file_location(name,p);m=importlib.util.module_from_spec(spec);sys.modules[name]=m;spec.loader.exec_module(m);return m
def census(root):
 root=Path(root);result={}
 for p in root.rglob('*'):
  st=p.lstat()
  if p.is_symlink() or getattr(st,'st_file_attributes',0)&0x400:raise ValueError('source/model census alias refused')
  if p.is_file():result[p.relative_to(root).as_posix()]=digest(p)
 return result
def check_deadline(deadline,clock=time.monotonic):
 if clock()>=deadline:raise TimeoutError('original whole-phase deadline exhausted')
def seal_inputs(plan,deadline,clock=time.monotonic):
 for path,expected in plan['input_seals'].items():
  check_deadline(deadline,clock)
  if digest(regular(path))!=expected:raise ValueError('sealed input drift: '+path)
 for root,expected in plan['directory_censuses'].items():
  check_deadline(deadline,clock)
  if census(root)!=expected:raise ValueError('source/model directory census changed: '+root)
 check_deadline(deadline,clock)
def validate_plan(plan):
 if plan.get('schema')!='airk.private.public-human-visibility-owned-plan/1' or plan.get('limits')!=LIMITS or plan.get('selector')!=SELECTOR:raise ValueError('unreviewed visibility plan/limits')
 if plan.get('original_denominator')!=1955 or plan.get('human_sid')!='S-1-5-21-3678370870-3858642417-826968105-1002':raise ValueError('original denominator/human changed')
 if Path(plan['runner']['path']).resolve()!=Path(__file__).resolve() or digest(__file__)!=plan['runner']['sha256']:raise ValueError('runner not original reviewed source')
def validate_body(body,plan,intake):
 if body.get('schema')!='airk.private.public-human-native-visibility-result/1' or body.get('complete') is not True:raise ValueError('incomplete native visibility report')
 if body.get('input')!=plan['native_input'] or body.get('native_binary_sha256')!=plan['image']['sha256']:raise ValueError('native input/image identity differs')
 if body.get('original_denominator')!=1955 or len(body.get('rows',[]))!=1955 or [x.get('original') for x in body['rows']]!=intake['rows']:raise ValueError('original1955 rows lost/changed')
 if any(body.get(k) is not False for k in ['gold_admission','rights_admission','published','training_gradients','nonblank_embedding_request_dispatched','focus_scorer_request_dispatched']):raise ValueError('unexpected semantic admission/request dispatch')
 if any(body.get(k) is not None for k in ['embedding_forwards','focus_forwards']):raise ValueError('unobserved forwards cannot be represented as measured')
 if len(body.get('actual_units',[]))>100000:raise ValueError('native unit census bound changed')
def monitor(api,info,deadline,report,log,clock=time.monotonic):
 while True:
  check_deadline(deadline,clock)
  for path,cap in [(report,LIMITS['report_bytes']),(log,LIMITS['log_bytes'])]:
   if path.exists() and path.stat().st_size>cap:raise ValueError('actual output cap exceeded: '+str(path))
  wait=api.WaitForSingleObject(info.process,max(1,min(50,int((deadline-clock())*1000))))
  if wait==0:return {'observed_leader_exit':True,'report_bytes_at_exit':report.stat().st_size if report.exists() else None},[]
  if wait!=258:raise OSError('native event wait failed')
def final_state(receipt,deadline,clock=time.monotonic):
 receipt['whole_phase_expired']=clock()>=deadline
 if receipt['whole_phase_expired']:
  receipt['complete']=False
  if receipt['failure'] is None:receipt['failure']={'type':'TimeoutError','message':'original whole phase expired, including final hash verification'}
def write(path,value):
 with path.open('x',encoding='utf-8',newline='\n') as f:json.dump(value,f,indent=2);f.write('\n');f.flush();os.fsync(f.fileno())
def run(plan_path,expected,started=None):
 started=time.monotonic() if started is None else started;deadline=started+LIMITS['wall_seconds']
 plan=json.loads(pin({'path':str(Path(plan_path).resolve()),'sha256':expected}).read_bytes());validate_plan(plan)
 worker=load(plan['worker'],'reference_worker');parent=load(plan['parent'],'visibility_reviewed_parent');owned=load(plan['ownership'],'visibility_reviewed_owned')
 if worker.windows_sid()!=plan['human_sid']:raise ValueError('parent not verified human')
 seal_inputs(plan,deadline)
 request=json.loads(pin(plan['native_input']).read_bytes());intake=json.loads(pin(request['rows']).read_bytes())
 for key in ['data','output']:
  if Path(request[key]).exists():raise ValueError('original native output not fresh: '+key)
 output=Path(plan['run_directory'])
 if output.exists() or output.parent.resolve()!=Path(plan_path).resolve().parent:raise ValueError('fresh immediate private run directory required')
 output.mkdir();log=output/'stdout-stderr.log';stdin=output/'stdin.bin';report=Path(request['output']);argv=[plan['image']['path'],'--ignored','--exact',SELECTOR,'--nocapture','--test-threads=1']
 environment=dict(os.environ);environment.update(plan['environment']);environment['AIRK_PUBLIC_HUMAN_VISIBILITY_INPUT']=plan['native_input']['path'];environment['AIRK_PUBLIC_HUMAN_VISIBILITY_INPUT_SHA256']=plan['native_input']['sha256']
 receipt={'schema':'airk.private.public-human-visibility-owned-execution/1','complete':False,'failure':None,'plan':{'path':str(Path(plan_path).resolve()),'sha256':expected},'command':argv,'execution_sid':plan['human_sid'],'original_denominator':1955,'limits':LIMITS,'whole_phase_deadline_not_refreshed':True,'whole_phase_expired':False,'post_input_drift':[],'no_automatic_retry':True,'rights_admission':False,'gold_admission':False,'qualification_granted':False,'native_image':plan['image'],'original_build_receipt':plan['native_build_receipt'],'original_native_group':plan['original_native_group'],'scope':'Native visibility diagnostic only; no model-forward observer, gradients or semantic/rights acceptance.'}
 write(output/'started.json',receipt)
 def before_resume(api,job,info,child_deadline):return parent.suspended_authentication(api,info,deadline,child_deadline,plan['human_sid'],plan['image'],owned.image_proof)
 def after_resume(api,job,info,child_deadline):
  if child_deadline>deadline:raise ValueError('original deadline renewed')
  return monitor(api,info,deadline,report,log)
 try:
  check_deadline(deadline)
  with stdin.open('xb'):pass
  with stdin.open('rb') as initial,log.open('xb') as stream:
   observed=owned.run_windows_owned_training(argv,Path(plan['repository_root']),stream,max(0,deadline-time.monotonic()),environment,initial,LIMITS['host_committed_bytes'],before_resume=before_resume,after_resume=after_resume,cleanup_seconds=LIMITS['cleanup_seconds'],absolute_deadline=deadline)
  receipt['owned']=observed
  if not parent.settled(observed):raise ValueError('actual process/Job/handle custody did not naturally settle')
  if not 0<observed['peak_host_committed_bytes']<=LIMITS['host_committed_bytes']:raise ValueError('actual committed memory outside cap')
  if report.stat().st_size>LIMITS['report_bytes'] or log.stat().st_size>LIMITS['log_bytes']:raise ValueError('retained output exceeds cap')
  text=log.read_text(encoding='utf-8',errors='replace')
  if not re.search(r'(?m)^running 1 test\s*$',text) or not re.search(r'test result: ok\. 1 passed; 0 failed;',text):raise ValueError('actual native selector did not pass exactly one test')
  body=json.loads(report.read_bytes());validate_body(body,plan,intake)
  receipt['native_report']={'path':str(report),'bytes':report.stat().st_size,'sha256':digest(report)};receipt['complete']=True
 except BaseException as error:
  receipt['failure']={'type':type(error).__name__,'message':str(error)}
  if hasattr(error,'owned_cleanup'):receipt['owned_cleanup']=error.owned_cleanup
 finally:
  try:seal_inputs(plan,deadline)
  except BaseException as error:
   receipt['complete']=False;receipt['post_input_drift'].append({'type':type(error).__name__,'message':str(error)})
   if receipt['failure'] is None:receipt['failure']={'type':'InputClosureRefusal','message':str(error)}
  final_state(receipt,deadline);receipt['elapsed_seconds']=time.monotonic()-started
  for label,path in [('native_report_observation',report),('log',log)]:
   if path.is_file():receipt[label]={'path':str(path),'bytes':path.stat().st_size,'sha256':digest(path)}
  final_state(receipt,deadline);write(output/'execution.json',receipt)
 print(json.dumps({'output':str(output),'complete':receipt['complete'],'failure':receipt['failure'],'qualification_granted':False}));return 0 if receipt['complete'] else 1
def main():
 started=time.monotonic()
 ap=argparse.ArgumentParser(description=__doc__);ap.add_argument('--plan',required=True);ap.add_argument('--plan-sha256',required=True);ap.add_argument('--execute-reviewed',action='store_true');a=ap.parse_args();p=json.loads(pin({'path':str(Path(a.plan).resolve()),'sha256':a.plan_sha256}).read_bytes());validate_plan(p)
 if not a.execute_reviewed:print(json.dumps({'plan_metadata_valid':True,'native_execution':False,'limits':LIMITS}));return 0
 return run(a.plan,a.plan_sha256,started)
if __name__=='__main__':raise SystemExit(main())
