# SPDX-License-Identifier: Apache-2.0
"""Root-owned LLM/final checkpoint sections and explicit final composition; first runner retained."""
import argparse,copy,datetime,hashlib,importlib.util,json,time
from pathlib import Path
HERE=Path(__file__).resolve().parent
CAMPAIGN=datetime.datetime(2026,11,19,5,29,42,tzinfo=datetime.timezone.utc)
def digest(p):
 h=hashlib.sha256()
 with Path(p).open('rb') as f:
  for b in iter(lambda:f.read(4*1024*1024),b''):h.update(b)
 return h.hexdigest()
def pin(p):p=Path(p);return {'path':str(p),'bytes':p.stat().st_size,'sha256':digest(p)}
def load(p,n):
 s=importlib.util.spec_from_file_location(n,p);m=importlib.util.module_from_spec(s);s.loader.exec_module(m);return m
def section_seconds(now):
 remaining=min(10800,(CAMPAIGN-now).total_seconds())
 if now.utcoffset()!=datetime.timedelta(0) or remaining<=0:raise ValueError('campaign cutoff reached/invalid UTC')
 return remaining
def new_log_required(old,new_sha,new_record):
 if old['sha256']!=new_sha and (new_record is None or new_record==old['ninja_success_record'] or new_record['command_hash']!=old['ninja_success_record']['command_hash']):raise ValueError('changed warm output lacks new successful same-command Ninja record')
def ninja_records(path):
 records={}
 for line in Path(path).read_text().splitlines():
  parts=line.split('\t')
  if len(parts)==5:records[parts[3].replace('\\','/') ]={'start_ms':int(parts[0]),'end_ms':int(parts[1]),'output_mtime':parts[2],'command_hash':parts[4]}
 return records
def require_complete_tree(command):
 tree=command.get('actual_tree_result')
 if not isinstance(tree,dict) or tree.get('schema')!='airk.private.process-tree-result/1' or tree.get('exit_code')!=0 or tree.get('wait')!='Start-Process-Wait-process-and-descendants':raise ValueError('actual original complete process-tree result required')
 if command.get('owned_job_final_active_processes')!=0 or command.get('owned_job_final_member_pids')!=[]:raise ValueError('actual final original Job0 required')
def verify_section(value,index,o,plan_sha):
 expected=['flash-remainder','llm-remainder','final-provider-all-closure'][index]
 require_complete_tree(value.get('command',{}))
 if value.get('schema')!='airk.private.cuda-checkpoint-section/1' or value.get('status')!='PASS_TARGET_SECTION_ONLY_UNQUALIFIED' or value.get('qualification') is not False or value.get('section_id')!=expected or value.get('section_index')!=index or value.get('plan',{}).get('sha256')!=plan_sha or not o.successful(value.get('command',{})):raise ValueError('actual natural predecessor target success required')
 first=datetime.datetime.fromisoformat(value['started_utc']);last=datetime.datetime.fromisoformat(value['completed_utc']);cutoff=datetime.datetime.fromisoformat(value['absolute_cutoff_utc'])
 if value.get('original_section_seconds')!=10800 or cutoff!=min(first+datetime.timedelta(seconds=10800),CAMPAIGN) or not first<=last<=cutoff or value.get('elapsed_seconds',10801)>10800:raise ValueError('predecessor fixed section/campaign deadline refused')
 for key in ['plan','runner','input_contract','warm_checkpoint_before','warm_checkpoint_after','source_manifest_before','source_manifest_after']:o.verify_file(o.plain(value[key]['path'],regular=True),value[key])
 if o.read_json(value['source_manifest_before']['path'])!=o.read_json(value['source_manifest_after']['path']):raise ValueError('predecessor source/tool/config changed')
 if index==0 and (value['runner']['sha256']!='b382a216d1a46a1245b6fc3f78289e9abb61a027eed5f86699713c0b4db0d619' or value['input_contract']['sha256']!='68a5f3109e3ad78a5453a966fd1d00db4217096536f7b7f7253ef309d9a396eb'):raise ValueError('exact first section source/input bridge required')
def copy_state(source,target,expected=None):
 import shutil
 before=pin(source)
 if expected and (before['bytes'],before['sha256'])!=(expected['bytes'],expected['sha256']):raise ValueError('exact state bytes unavailable for immutable custody')
 with Path(source).open('rb') as src,Path(target).open('xb') as dst:shutil.copyfileobj(src,dst,1024*1024)
 result=pin(target)
 if (before['bytes'],before['sha256'])!=(result['bytes'],result['sha256']) or pin(source)!=before:raise ValueError('state changed during immutable copy')
 return result
def write_first_custody(o,r8,out,first,current,before_pin):
 o.verify_file(o.plain(before_pin['path'],regular=True),before_pin);before=o.read_json(before_pin['path']);rows=[]
 for kind in ['ninja_log','ninja_deps']:
  row=next(r for r in before['rows'] if r['kind']==kind)
  if row['status']!='COPIED_EXACT_ORIGINAL_BASELINE' or row['original_pin']!=first[kind+'_before']:raise ValueError('first original baseline custody differs')
  o.verify_file(o.plain(row['copy']['path'],regular=True),row['copy']);rows.append({'kind':kind,'before_original':first[kind+'_before'],'before_copy':row['copy'],'after_original':first[kind+'_after'],'after_copy':current[kind+'_before']})
 path=out/'first-state-custody.json';r8.write(path,{'schema':'airk.private.cuda-first-section-custody/1','phase_receipt_unchanged':True,'rows':rows});return pin(path)
def target_outputs(build,roots):
 import re
 producers={}
 for line in (build/'build.ninja').read_text().replace('$\n','').splitlines():
  m=re.fullmatch(r'build (.+?): (\S+)(?: (.*))?',line)
  if not m:continue
  outs=[x.replace('\\','/') for x in m[1].split() if x!='|'];deps=[x.replace('\\','/') for x in (m[3] or '').split() if x not in ['|','||']];edge=(outs,m[2],deps)
  for name in outs:producers[name]=edge
 selected={};todo=list(roots)
 while todo:
  edge=producers.get(todo.pop())
  if edge is None or id(edge) in selected:continue
  selected[id(edge)]=edge;todo.extend(edge[2])
 if roots==['onnxruntime_providers_cuda_llm']:
  alias=producers.get(roots[0])
  if alias is None or alias[1]!='phony':raise ValueError('exact LLM alias missing')
  return {name:producers[name][1] for name in alias[2] if name.endswith('.obj') and name in producers}
 result={}
 for outs,rule,deps in selected.values():
  if rule=='phony':continue
  for name in outs:
   if (build/name).is_file():result[name]=rule
 return result
def verify_materialized(inputs,plan):
 r12_path=Path(inputs['r12_proposal']);r12=load(r12_path/'continue_cuda_runtime.py','exact_r12_materialized');req=json.loads((r12_path/'continuation-request.json').read_bytes());r8=load(req['recipe']['path'],'exact_r8_materialized');lock=json.loads(Path(req['recipe_lock']['path']).read_bytes());o=r8.cpu_owner(lock);dest=o.plain(plan['actual_build_root']);prior=o.read_json(plan['previous_failed_R12']['path']);tools=prior['toolchain']
 for row in inputs['pins']:o.verify_file(o.plain(row['path'],regular=True),row)
 for row in o.read_json(inputs['failed_ancestry']['path']):o.verify_file(o.plain(row['path'],regular=True),row)
 for row in tools.values():
  if isinstance(row,dict) and {'path','bytes','sha256'}.issubset(row):o.verify_file(o.plain(row['path'],regular=True),row)
 original=o.read_json(dest/'frozen-input-closure.json');r8.verify_input_closure(o,original);mirrors=[{'path':o.mirror_relative(r),'bytes':r['bytes'],'sha256':r['sha256']} for r in o.LOCK['dependencies']+lock['dependencies']];r8.verify_mirror_closure(o,dest/'source/mirror',mirrors)
 source_rows=r12.changed_rows(o.read_json(req['parent_source_inventory']['path']),req['before']['sha256'],req['after']['sha256'],req['after']['bytes'])
 if o.source_inventory(dest/'source',exclude_mirror=True)!=source_rows or o.source_inventory(dest/'frontend')!=o.read_json(dest/'frontend-source-files.json'):raise ValueError('actual compiled source changed')
 for row in o.read_json(dest/'cuda-toolkit-inputs.json'):o.verify_file(o.child(r8.CUDA,row['path']),row)
 for row in o.read_json(r8.INPUT/'extraction.json')['files']:o.verify_file(o.child(r8.INPUT/'cudnn-dev',row['path']),row)
 newlock=copy.deepcopy(lock);newlock['final_placement']['after_sha256']=req['after']['sha256'];preflight=r8.configured_build_preflight(o,dest,prior['configure_argv'],newlock)
 if preflight!=plan['configured_preflight']:raise ValueError('actual source/target/cache/configuration changed')
 return {'schema':'airk.private.cuda-materialized-source/1','actual_build_root':str(dest),'corrected_capi_sha256':req['after']['sha256'],'source_inventory_sha256':digest(Path(plan['current_source_inventory']['path'])),'frontend_inventory_sha256':digest(dest/'frontend-source-files.json'),'cuda_inventory_sha256':digest(dest/'cuda-toolkit-inputs.json'),'frozen_original_inputs_sha256':digest(dest/'frozen-input-closure.json'),'tools':tools,'configured_preflight':preflight,'configure_argv':prior['configure_argv'],'policy':prior['policy'],'failed_r12_sha256':plan['previous_failed_R12']['sha256']}
def main():
 ap=argparse.ArgumentParser();ap.add_argument('--execute-section',choices=['llm-remainder','final-provider-all-closure'],required=True);ap.add_argument('--predecessor-receipt',type=Path,required=True);ap.add_argument('--predecessor-sha256',required=True);ap.add_argument('--approved-script-sha256',required=True);ap.add_argument('--approved-input-sha256',required=True);a=ap.parse_args()
 if digest(__file__)!=a.approved_script_sha256 or digest(HERE/'input-contract.json')!=a.approved_input_sha256:raise ValueError('explicit reviewed first Flash source/input required')
 inputs=json.loads((HERE/'input-contract.json').read_bytes());plan_pin=inputs['approved_plan'];plan=json.loads(Path(plan_pin['path']).read_bytes())
 if plan_pin['sha256']!='6bee6b8b95a01fea28adf04117260d12503012ea8120ae225afa232b6589df97' or digest(plan_pin['path'])!=plan_pin['sha256']:raise ValueError('exact approved target plan differs')
 r12_path=Path(inputs['r12_proposal']);r12=load(r12_path/'continue_cuda_runtime.py','exact_prior_r12_helpers');req=json.loads((r12_path/'continuation-request.json').read_bytes());r8=load(req['recipe']['path'],'exact_r8_source_owner');lock=json.loads(Path(req['recipe_lock']['path']).read_bytes());o=r8.cpu_owner(lock);dest=o.plain(plan['actual_build_root']);build=dest/'b/Release';prior=o.read_json(plan['previous_failed_R12']['path']);r12.verify_parent(o.read_json(req['parent_receipt']['path']),o)
 for row in inputs['pins']:o.verify_file(o.plain(row['path'],regular=True),row)
 if prior['status']!='FAILED_PRESERVED_NO_RETRY' or prior['commands'][-1]['exit_code']!=77 or prior['commands'][-1].get('timed_out') is not True or prior['commands'][-1].get('owned_descendants_settled') is not True or prior['commands'][-1].get('owned_job_forced_termination') is not True or prior['commands'][-1].get('owned_job_final_active_processes')!=0:raise ValueError('original settled failed R12 must stay failed')
 index=1 if a.execute_section=='llm-remainder' else 2;section=plan['sections'][index]
 predecessor_path=o.plain(a.predecessor_receipt,regular=True)
 if digest(predecessor_path)!=a.predecessor_sha256:raise ValueError('reviewed predecessor receipt pin differs')
 predecessor=o.read_json(predecessor_path);verify_section(predecessor,index-1,o,plan_pin['sha256'])
 if index==2 and (predecessor['runner']['sha256']!=a.approved_script_sha256 or predecessor['input_contract']['sha256']!=a.approved_input_sha256):raise ValueError('exact same successor runner/input authority required')
 if section['id']!=a.execute_section or section['targets']!=(['onnxruntime_providers_cuda_llm'] if index==1 else ['all','re2']) or section['fixed_original_seconds']!=10800 or section['parallel']!=2:raise ValueError('exact approved successor target section required')
 started_mono=time.monotonic();started_utc=datetime.datetime.now(datetime.timezone.utc);budget=section_seconds(started_utc);deadline=started_mono+budget;cutoff=min(started_utc+datetime.timedelta(seconds=10800),CAMPAIGN)
 out=r8.PARENT/('cu8-checkpoint-llm-r1' if index==1 else 'cu8-checkpoint-final-r1');identity_lock=copy.deepcopy(lock);identity_lock['build_path']['fresh_child']=out.name;policy=r8.identity_and_parent(o,out,identity_lock);out.mkdir()
 failed=o.read_json(inputs['failed_ancestry']['path']);before_rows=o.read_json(predecessor['warm_checkpoint_after']['path']);tools=prior['toolchain'];source_rows=r12.changed_rows(o.read_json(req['parent_source_inventory']['path']),req['before']['sha256'],req['after']['sha256'],req['after']['bytes']);newlock=copy.deepcopy(lock);newlock['final_placement']['after_sha256']=req['after']['sha256'];old_inputs=o.read_json(dest/'frozen-input-closure.json');mirrors=[{'path':o.mirror_relative(r),'bytes':r['bytes'],'sha256':r['sha256']} for r in o.LOCK['dependencies']+lock['dependencies']]
 receipt={'schema':'airk.private.cuda-checkpoint-section/1','status':'RUNNING','qualification':False,'section_id':section['id'],'section_index':index,'plan':plan_pin,'runner':pin(Path(__file__)),'input_contract':pin(HERE/'input-contract.json'),'predecessor':{'path':str(predecessor_path),'bytes':predecessor_path.stat().st_size,'sha256':a.predecessor_sha256},'failed_ancestry':failed,'warm_checkpoint_before':predecessor['warm_checkpoint_after'],'original_section_seconds':10800,'started_utc':started_utc.isoformat(),'observed_monotonic_seconds':started_mono,'absolute_cutoff_utc':cutoff.isoformat(),'converted_monotonic_deadline':deadline,'global_campaign_cutoff_utc':CAMPAIGN.isoformat(),'output_policy':policy,'native_argv':section['argv'],'cold_build_qualified':False}
 def verify_inputs():
  o.verify_file(o.plain(predecessor_path,regular=True),receipt['predecessor'])
  for row in inputs['pins']:o.verify_file(o.plain(row['path'],regular=True),row)
  for row in failed:o.verify_file(o.plain(row['path'],regular=True),row)
  if digest(__file__)!=a.approved_script_sha256 or digest(HERE/'input-contract.json')!=a.approved_input_sha256:raise ValueError('section producer/input source drift')
 def source_manifest():
  verify_inputs()
  return verify_materialized(inputs,plan)
 try:
  r8.write(out/'start.json',receipt);before=source_manifest();r8.write(out/'source-before.json',before);receipt['source_manifest_before']=pin(out/'source-before.json')
  for kind in ['ninja_log','ninja_deps']:
   live=build/('.ninja_log' if kind=='ninja_log' else '.ninja_deps');expected=predecessor[kind+'_after']
   if pin(live)['sha256']!=expected['sha256'] or live.stat().st_size!=expected['bytes']:raise ValueError('predecessor exact Ninja after-state no longer available')
   receipt[kind+'_before']=copy_state(live,out/(kind+'-before'),expected)
  if index==1:
   receipt['predecessor_custody']=write_first_custody(o,r8,out,predecessor,receipt,inputs['first_before_custody'])
  for row in before_rows:o.verify_file(o.plain(build/row['path'],regular=True),row)
  remaining=min(deadline-time.monotonic(),(cutoff-datetime.datetime.now(datetime.timezone.utc)).total_seconds())
  if remaining<=0:raise TimeoutError('fixed section budget exhausted before native command')
  env=load(r12_path/'producer_environment.py','same_r12_section_environment').producer_environment(out,r8.CUDA);tree=load(r12_path/'process_tree.py','same_r12_section_tree');actual=o.windows_command(section['argv'],tools,out,'llm-section' if index==1 else 'final-section');receipt['original_command_argv']=actual;receipt['command']=tree.run_tree(o,actual,dest,out,'llm-section' if index==1 else 'final-section',remaining,env)
  if not o.successful(receipt['command']):raise ValueError('actual successor target tree failed or not naturally settled')
  after=source_manifest();r8.write(out/'source-after.json',after);receipt['source_manifest_after']=pin(out/'source-after.json')
  if before!=after:raise ValueError('source/tool/config identity differs after section')
  records=ninja_records(build/'.ninja_log');after_rows={}
  for row in before_rows:
   path=o.plain(build/row['path'],regular=True);observed=pin(path);record=records.get(row['path']) or records.get(path.as_posix());new_log_required(row,observed['sha256'],record);after_rows[row['path']]={**row,'bytes':observed['bytes'],'sha256':observed['sha256'],'mtime_ns':path.stat().st_mtime_ns,'ninja_success_record':record or row['ninja_success_record']}
  names=target_outputs(build,section['targets'])
  if index==1 and len(names)!=43:raise ValueError('exact43 LLM target objects required')
  for name,rule in names.items():
   path=o.plain(build/name,regular=True);record=records.get(name) or records.get(path.as_posix())
   if record is None and ('COMPILER' in rule or 'LINKER' in rule):raise ValueError('compiled/linked target output lacks actual successful Ninja record')
   item=pin(path);after_rows[name]={'path':name,'bytes':item['bytes'],'sha256':item['sha256'],'mtime_ns':path.stat().st_mtime_ns,'ninja_success_record':record,'rule':rule}
  r8.write(out/'warm-output-after.json',sorted(after_rows.values(),key=lambda x:x['path']));receipt['warm_checkpoint_after']=pin(out/'warm-output-after.json');receipt['ninja_log_after']=copy_state(build/'.ninja_log',out/'ninja_log-after');receipt['ninja_deps_after']=copy_state(build/'.ninja_deps',out/'ninja_deps-after');receipt['target_outputs_completed']=len(names);verify_inputs()
  if time.monotonic()>=deadline or datetime.datetime.now(datetime.timezone.utc)>=cutoff:raise TimeoutError('fixed section/campaign cutoff expired during final closure')
  receipt['status']='PASS_TARGET_SECTION_ONLY_UNQUALIFIED'
 except BaseException as exc:receipt.update(status='FAILED_PRESERVED_NO_RETRY',error=type(exc).__name__+': '+str(exc));raise
 finally:receipt['elapsed_seconds']=time.monotonic()-started_mono;receipt['completed_utc']=datetime.datetime.now(datetime.timezone.utc).isoformat();r8.write(out/'receipt.json',receipt)
 print(json.dumps({'status':receipt['status'],'output':str(out)}))
if __name__=='__main__':main()
