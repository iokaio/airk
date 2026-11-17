# SPDX-License-Identifier: Apache-2.0
"""Explicit Root-owned incremental continuation of failed cu8; never retry unchanged."""
import argparse,copy,hashlib,importlib.util,json,os,shutil,subprocess,time,datetime
from pathlib import Path
HERE=Path(__file__).resolve().parent
SOURCE_REL='onnxruntime/core/session/onnxruntime_c_api.cc'
def digest(p):
 h=hashlib.sha256()
 with Path(p).open('rb') as f:
  for b in iter(lambda:f.read(4*1024*1024),b''):h.update(b)
 return h.hexdigest()
def load(p,n):
 s=importlib.util.spec_from_file_location(n,p);m=importlib.util.module_from_spec(s);s.loader.exec_module(m);return m
def row(p):return {'path':str(p),'bytes':p.stat().st_size,'sha256':digest(p)}
def changed_rows(before,old,new,size):
 result=copy.deepcopy(before);found=0
 for item in result:
  if item['path']==SOURCE_REL:
   if item['sha256']!=old:raise ValueError('parent CAPI source identity differs')
   item.update(bytes=size,sha256=new);found+=1
 if found!=1:raise ValueError('exact single CAPI target missing')
 return result
def verify_parent(parent,o):
 if parent.get('status')!='FAILED_PRESERVED_NO_RETRY':raise ValueError('exact failed parent required')
 cs=parent.get('commands',[])
 if len(cs)!=8 or any(not o.successful(c) for c in cs[:-1]):raise ValueError('parent precompile stages not successful')
 last=cs[-1]
 if last.get('exit_code')!=1 or not last.get('owned_descendants_settled') or last.get('owned_job_forced_termination') or last.get('timed_out') or last.get('log_oversized'):raise ValueError('original failed compile not naturally settled')
def wall_remaining(started, now, active_elapsed):
 if started.utcoffset()!=datetime.timedelta(0) or now.utcoffset()!=datetime.timedelta(0) or now<started or active_elapsed<0:raise ValueError('invalid original wall clock')
 return min(10800-(now-started).total_seconds(),10800-active_elapsed)
def verify_diagnostics(bundle,o,source_sha,request_sha):
 if bundle.get('schema')!='airk.private.cuda-source-diagnostics/1' or bundle.get('source_after_sha256')!=source_sha or bundle.get('request_sha256')!=request_sha:raise ValueError('exact corrected-source diagnostic identity required')
 rows=bundle.get('syntax_runs',[])
 if [r.get('name') for r in rows]!=['capi-syntax-r9','capi-syntax-r10','capi-syntax-r11']:raise ValueError('exact original syntax diagnostic roster required')
 facts=[];elapsed=0
 for index,item in enumerate(rows):
  o.verify_file(o.plain(item['path'],regular=True),item);value=o.read_json(item['path']);command=value.get('command',{})
  if value.get('status')!='FAILED_PRESERVED_NO_RETRY' or value.get('source_sha256')!=source_sha or value.get('request_sha256')!=request_sha:raise ValueError('failed diagnostic identity/status must remain intact')
  if command.get('owned_descendants_settled') is not True or command.get('owned_job_forced_termination') is not True or command.get('owned_job_final_active_processes')!=0 or command.get('owned_job_final_member_pids')!=[]:raise ValueError('actual failed diagnostic cleanup proof differs')
  if index<2:
   if command.get('exit_code')!=0 or command.get('timed_out') is not False:raise ValueError('actual CL exit0 diagnostic absent')
   kind='compiler-exit0-only; helper cleanup FAILED'
  else:
   if command.get('exit_code')!=77 or command.get('timed_out') is not True or value.get('process_tree_owner') is not True:raise ValueError('actual compiler-tree timeout diagnostic absent')
   kind='actual tree timeout FAILED; no CL exit inferred'
  elapsed+=value['elapsed_seconds'];facts.append({'name':item['name'],'receipt':{k:item[k] for k in ['path','bytes','sha256']},'kind':kind,'syntax_qualified':False})
 witness=bundle['tree_witness'];o.verify_file(o.plain(witness['path'],regular=True),witness);tree=o.read_json(witness['path']);cases=tree.get('cases',[])
 if tree.get('status')!='PASS_PROCESS_TREE_WITNESSES_ONLY' or len(cases)!=2 or [c.get('mode') for c in cases]!=['normal','linger'] or any(c.get('passed') is not True or not o.successful(c.get('owner',{})) for c in cases) or cases[1].get('parent_exit_observed_while_child_alive_and_wrapper_pending') is not True:raise ValueError('actual independent process-tree witnesses missing')
 elapsed+=tree['elapsed_seconds']
 return {'scope':'preparatory source compatibility and failed helper-lifetime diagnostics only','syntax_qualified':False,'runs':facts,'tree_witness':witness,'elapsed_seconds':elapsed}
def main():
 ap=argparse.ArgumentParser();ap.add_argument('--execute-build',action='store_true');ap.add_argument('--approved-script-sha256',required=True);ap.add_argument('--approved-request-sha256',required=True);ap.add_argument('--approved-baseline-sha256',required=True);ap.add_argument('--approved-wall-origin-sha256',required=True);ap.add_argument('--approved-environment-input-sha256',required=True);ap.add_argument('--approved-diagnostic-evidence-sha256',required=True);a=ap.parse_args()
 if not a.execute_build or digest(__file__)!=a.approved_script_sha256 or digest(HERE/'continuation-request.json')!=a.approved_request_sha256:raise ValueError('explicit reviewed continuation inputs required')
 if digest(HERE/'continuation-baseline.json')!=a.approved_baseline_sha256:raise ValueError('reviewed failed-tree baseline differs')
 req=json.loads((HERE/'continuation-request.json').read_bytes());recipe=load(req['recipe']['path'],'reviewed_r8_continuation');lock=json.loads(Path(req['recipe_lock']['path']).read_bytes());o=recipe.cpu_owner(lock);dest=o.plain(req['destination']);source=o.plain(dest/'source'/SOURCE_REL,regular=True)
 if dest!=recipe.PARENT/'cu8':raise ValueError('only reviewed cu8 continuation allowed')
 for name in ['recipe','recipe_lock','before','after','patch','syntax_card','syntax_response','parent_receipt','parent_failed_log','parent_source_inventory']:o.verify_file(o.plain(req[name]['path'],regular=True),req[name])
 parent=o.read_json(req['parent_receipt']['path']);verify_parent(parent,o)
 if digest(HERE/'diagnostic-evidence.json')!=a.approved_diagnostic_evidence_sha256:raise ValueError('reviewed actual diagnostic bundle differs')
 diagnostics=o.read_json(HERE/'diagnostic-evidence.json');diagnostic_facts=verify_diagnostics(diagnostics,o,req['after']['sha256'],a.approved_request_sha256)
 if digest(HERE/'wall-origin.json')!=a.approved_wall_origin_sha256:raise ValueError('reviewed absolute wall cutoff differs')
 wall=o.read_json(HERE/'wall-origin.json');o.verify_file(o.plain(wall['start_file']['path'],regular=True),wall['start_file']);original_start=o.read_json(wall['start_file']['path'])
 started=datetime.datetime.fromisoformat(original_start['started_utc']);cutoff=started+datetime.timedelta(seconds=10800)
 if original_start.get('outer_timeout_seconds')!=10800 or original_start.get('actual_destination')!=str(dest) or original_start.get('argv',[])[-1]!='10800' or wall['started_utc']!=original_start['started_utc'] or wall['absolute_cutoff_utc']!=cutoff.isoformat():raise ValueError('original outer wall identity differs')
 start=time.monotonic();observed_utc=datetime.datetime.now(datetime.timezone.utc)
 if observed_utc<datetime.datetime.fromisoformat(wall['prepared_utc']):raise ValueError('UTC moved backwards before continuation')
 active_elapsed=parent['elapsed_seconds']+diagnostic_facts['elapsed_seconds'];remaining=wall_remaining(started,observed_utc,active_elapsed);deadline=start+remaining
 if remaining<=0 or remaining>req['remaining_original_budget_seconds']:raise ValueError('original10800s budget exhausted or widened')
 out=recipe.PARENT/'cu8-continuation-r12';idlock=copy.deepcopy(lock);idlock['build_path']['fresh_child']=out.name;policy=recipe.identity_and_parent(o,out,idlock);out.mkdir()
 if digest(HERE/'input-pins.json')!=a.approved_environment_input_sha256:raise ValueError('reviewed environment input closure differs')
 environment_pins=o.read_json(HERE/'input-pins.json')
 for item in environment_pins:o.verify_file(o.plain(item['path'],regular=True),item)
 env_owner=load(HERE/'producer_environment.py','exact_r8_environment');environment=env_owner.producer_environment(out,recipe.CUDA)
 before=o.read_json(req['parent_source_inventory']['path']);after=changed_rows(before,req['before']['sha256'],req['after']['sha256'],req['after']['bytes'])
 closure=o.read_json(dest/'frozen-input-closure.json');front=o.read_json(dest/'frontend-source-files.json');cuda=o.read_json(dest/'cuda-toolkit-inputs.json');cudnn=o.read_json(recipe.INPUT/'extraction.json');tools=parent['toolchain'];mirrors=[]
 for item in o.LOCK['dependencies']+lock['dependencies']:mirrors.append({'path':o.mirror_relative(item),'bytes':item['bytes'],'sha256':item['sha256']})
 metadata=[dest/name for name in ['owner.json','gpu-build-receipt.json','gpu-source-files.json','source-files.json','frontend-source-files.json','frontend-source-before.json','cuda-toolkit-inputs.json','frozen-input-closure.json','b/Release/CMakeCache.txt','b/Release/build.ninja','b/Release/CMakeFiles/rules.ninja']]
 metadata_pins=o.read_json(HERE/'continuation-baseline.json')
 if [str(x) for x in metadata]!=[item['path'] for item in metadata_pins]:raise ValueError('baseline metadata roster differs')
 def verify(expected):
  verify_diagnostics(diagnostics,o,req['after']['sha256'],a.approved_request_sha256)
  for item in environment_pins:o.verify_file(o.plain(item['path'],regular=True),item)
  for item in metadata_pins:o.verify_file(o.plain(item['path'],regular=True),item)
  for name in ['recipe','recipe_lock','before','after','patch','syntax_card','syntax_response','parent_receipt','parent_failed_log','parent_source_inventory']:o.verify_file(o.plain(req[name]['path'],regular=True),req[name])
  if digest(__file__)!=a.approved_script_sha256 or digest(HERE/'continuation-request.json')!=a.approved_request_sha256 or digest(HERE/'continuation-baseline.json')!=a.approved_baseline_sha256 or digest(HERE/'wall-origin.json')!=a.approved_wall_origin_sha256 or digest(HERE/'diagnostic-evidence.json')!=a.approved_diagnostic_evidence_sha256 or digest(HERE/'input-pins.json')!=a.approved_environment_input_sha256:raise ValueError('continuation inputs drifted')
  for item in tools.values():
   if isinstance(item,dict) and {'path','bytes','sha256'}.issubset(item):o.verify_file(o.plain(item['path'],regular=True),item)
  o.verify_file(o.plain(wall['start_file']['path'],regular=True),wall['start_file'])
  recipe.verify_input_closure(o,closure);recipe.verify_mirror_closure(o,dest/'source/mirror',mirrors)
  if o.source_inventory(dest/'source',exclude_mirror=True)!=expected or o.source_inventory(dest/'frontend')!=front:raise ValueError('complete compiled source changed outside reviewed CAPI delta')
  for item in cuda:o.verify_file(o.child(recipe.CUDA,item['path']),item)
  for item in cudnn['files']:o.verify_file(o.child(recipe.INPUT/'cudnn-dev',item['path']),item)
 receipt={'schema':'airk.private.cuda-runtime-continuation/2','status':'RUNNING','parent_failed_receipt':req['parent_receipt'],'parent_failed_log':req['parent_failed_log'],'parent_source_inventory':req['parent_source_inventory'],'request_sha256':a.approved_request_sha256,'baseline_sha256':a.approved_baseline_sha256,'continuation_script_sha256':a.approved_script_sha256,'diagnostic_evidence':row(HERE/'diagnostic-evidence.json'),'syntax_diagnostics':diagnostic_facts,'syntax_qualification':False,'policy':parent['policy'],'original_budget_seconds':10800,'parent_elapsed_seconds':parent['elapsed_seconds'],'diagnostic_elapsed_seconds':diagnostic_facts['elapsed_seconds'],'remaining_seconds_at_start':remaining,'wall_origin_sha256':a.approved_wall_origin_sha256,'original_start_utc':started.isoformat(),'original_absolute_cutoff_utc':cutoff.isoformat(),'observed_utc':observed_utc.isoformat(),'observed_monotonic_seconds':start,'converted_monotonic_deadline':deadline,'consumed_seconds_at_start':10800-remaining,'wall_gap_charged':True,'identity_policy':policy,'environment_input_sha256':a.approved_environment_input_sha256,'environment_sha256':hashlib.sha256(json.dumps(environment,sort_keys=True,separators=(',',':')).encode()).hexdigest(),'environment_policy_source_sha256':digest(HERE/'environment-source.json'),'environment_policy_equivalent_to_R8':True,'controlled_environment':{k:environment[k] for k in ['PATH','TEMP','TMP','TMPDIR','PYTHONNOUSERSITE','CUDA_CACHE_PATH']},'new_telemetry_flags':False,'commands':[],'configure_argv':parent['configure_argv'],'toolchain':tools,'frozen_parent_metadata':metadata_pins,'original_source_preserved':False,'source_applied':False}
 try:
  verify(before)
  if source.read_bytes()!=Path(req['before']['path']).read_bytes():raise ValueError('actual CAPI before bytes differ')
  # Preserve complete original target bytes and failed evidence before in-place content edit.
  for name in ['before','parent_receipt','parent_source_inventory']:
   target=out/({'before':'onnxruntime_c_api.cc.before','parent_receipt':'parent-failed-receipt.json','parent_source_inventory':'parent-gpu-source-files.json'}[name]);shutil.copyfile(req[name]['path'],target);o.verify_file(target,req[name])
  receipt['original_source_preserved']=True
  acl_cmd="$a=Get-Acl -LiteralPath 'C:/Users/tdotn/AppData/Local/airk-final-exec-20261116T052942Z/cu8/source/onnxruntime/core/session/onnxruntime_c_api.cc'; @{owner=$a.Owner;sddl=$a.Sddl}|ConvertTo-Json -Compress"
  def acl():return json.loads(subprocess.run(['pwsh','-NoProfile','-Command',acl_cmd],check=True,capture_output=True,text=True,timeout=15).stdout)
  previous_acl=acl()
  if previous_acl['owner']!='TYAI'+chr(92)+'tdotn':raise ValueError('source owner is not current human')
  source.write_bytes(Path(req['after']['path']).read_bytes());receipt['source_applied']=True
  if acl()!=previous_acl:raise ValueError('source owner/access policy changed')
  receipt['source_acl']=previous_acl;verify(after)
  recipe.write(out/'gpu-source-files.json',after)
  newlock=copy.deepcopy(lock);newlock['final_placement']['after_sha256']=req['after']['sha256']
  preflight=recipe.configured_build_preflight(o,dest,parent['configure_argv'],newlock)
  if {k:v for k,v in preflight.items() if k!='final_capi_source_sha256'}!={k:v for k,v in parent['configured_preflight'].items() if k!='final_capi_source_sha256'}:raise ValueError('configure/build target identity changed')
  receipt['configured_preflight']=preflight
  inputs=closure+[row(o.plain(HERE/n,regular=True)) for n in ['continuation-request.json','continuation-baseline.json','wall-origin.json','continue_cuda_runtime.py','onnxruntime_c_api.cc.before','onnxruntime_c_api.cc','initialized-public-api.patch','syntax-card.json','capi-syntax.rsp','producer_environment.py','environment-source.json','input-pins.json','process_tree.py','wait_process_tree.ps1','CONTRACT.md','diagnostic-evidence.json']]+[{k:item[k] for k in ['path','bytes','sha256']} for item in diagnostics['syntax_runs']]+[diagnostics['tree_witness']]
  recipe.write(out/'frozen-input-closure.json',inputs);receipt['frozen_input_closure_sha256']=digest(out/'frozen-input-closure.json')
  for stage,targets in [('core-session-build-r12',['onnxruntime_session']),('build-r12',['all','re2'])]:
   left=min(deadline-time.monotonic(),(cutoff-datetime.datetime.now(datetime.timezone.utc)).total_seconds())
   if left<=0:raise TimeoutError('original10800s build budget expired')
   argv=[tools['cmake']['path'],'--build',str(dest/'b/Release'),'--config','Release','--target',*targets,'--parallel','2'];receipt[stage+'_argv']=argv
   actual=o.windows_command(argv,tools,out,stage);tree_owner=load(HERE/'process_tree.py','reviewed_continuation_tree_owner');command=tree_owner.run_tree(o,actual,dest,out,stage,left,environment);receipt['commands'].append(command)
   if not o.successful(command):raise ValueError('continuation stage failed or unsettled: '+stage)
  verify(after);recipe.verify_input_closure(o,inputs)
  if digest(out/'frozen-input-closure.json')!=receipt['frozen_input_closure_sha256']:raise ValueError('new closure record drift')
  libraries=[]
  for path in sorted((dest/'b').rglob('*.lib')):
   path=o.plain(path,regular=True)
   with path.open('rb') as f:
    if f.read(8)!=b'!<arch>\n':raise ValueError('invalid actual output archive')
   item=row(path);item['path']=path.relative_to(dest).as_posix();libraries.append(item)
  pair=[]
  for name in ['onnxruntime_providers_shared.dll','onnxruntime_providers_cuda.dll']:
   found=list((dest/'b').rglob(name))
   if len(found)!=1:raise ValueError('exact same-build provider pair missing')
   path=o.plain(found[0],regular=True);item=row(path);item['path']=path.relative_to(dest).as_posix();pair.append(item)
  if list((dest/'b').rglob('onnxruntime.dll')):raise ValueError('second dynamic core refused')
  if time.monotonic()>=deadline or datetime.datetime.now(datetime.timezone.utc)>=cutoff:raise TimeoutError('original build budget expired during final closure')
  receipt.update(status='PASS_PRODUCER_ONLY_UNQUALIFIED',libraries=libraries,provider_dlls=pair,source_inventory_sha256=digest(out/'gpu-source-files.json'),frontend_inventory_sha256=parent.get('frontend_inventory_sha256') or digest(dest/'frontend-source-files.json'),cuda_inventory_sha256=parent.get('cuda_inventory_sha256') or digest(dest/'cuda-toolkit-inputs.json'),reopened_frozen_input_count=len(inputs),actual_build_root=str(dest),changed_source_after_sha256=req['after']['sha256'])
 except BaseException as exc:receipt.update(status='FAILED_PRESERVED_NO_RETRY',error=type(exc).__name__+': '+str(exc));raise
 finally:receipt['elapsed_seconds']=time.monotonic()-start;recipe.write(out/'gpu-build-continuation-receipt.json',receipt)
 print(json.dumps({'status':receipt['status'],'output':str(out)}))
if __name__=='__main__':main()
