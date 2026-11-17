# SPDX-License-Identifier: Apache-2.0
"""Root-owned first Flash checkpoint section; no other section/automatic retry."""
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
def main():
 ap=argparse.ArgumentParser();ap.add_argument('--execute-flash-section',action='store_true');ap.add_argument('--approved-script-sha256',required=True);ap.add_argument('--approved-input-sha256',required=True);a=ap.parse_args()
 if not a.execute_flash_section or digest(__file__)!=a.approved_script_sha256 or digest(HERE/'input-contract.json')!=a.approved_input_sha256:raise ValueError('explicit reviewed first Flash source/input required')
 inputs=json.loads((HERE/'input-contract.json').read_bytes());plan_pin=inputs['approved_plan'];plan=json.loads(Path(plan_pin['path']).read_bytes())
 if plan_pin['sha256']!='6bee6b8b95a01fea28adf04117260d12503012ea8120ae225afa232b6589df97' or digest(plan_pin['path'])!=plan_pin['sha256']:raise ValueError('exact approved target plan differs')
 r12_path=Path(inputs['r12_proposal']);r12=load(r12_path/'continue_cuda_runtime.py','exact_prior_r12_helpers');req=json.loads((r12_path/'continuation-request.json').read_bytes());r8=load(req['recipe']['path'],'exact_r8_source_owner');lock=json.loads(Path(req['recipe_lock']['path']).read_bytes());o=r8.cpu_owner(lock);dest=o.plain(plan['actual_build_root']);build=dest/'b/Release';prior=o.read_json(plan['previous_failed_R12']['path']);r12.verify_parent(o.read_json(req['parent_receipt']['path']),o)
 for row in inputs['pins']:o.verify_file(o.plain(row['path'],regular=True),row)
 if prior['status']!='FAILED_PRESERVED_NO_RETRY' or prior['commands'][-1]['exit_code']!=77 or prior['commands'][-1].get('timed_out') is not True or prior['commands'][-1].get('owned_descendants_settled') is not True or prior['commands'][-1].get('owned_job_forced_termination') is not True or prior['commands'][-1].get('owned_job_final_active_processes')!=0:raise ValueError('original settled failed R12 must stay failed')
 section=plan['sections'][0]
 if section['id']!='flash-remainder' or section['targets']!=['onnxruntime_providers_cuda_flash_attention'] or section['fixed_original_seconds']!=10800 or section['parallel']!=2:raise ValueError('only exact first approved Flash section executable')
 started_mono=time.monotonic();started_utc=datetime.datetime.now(datetime.timezone.utc);budget=section_seconds(started_utc);deadline=started_mono+budget;cutoff=min(started_utc+datetime.timedelta(seconds=10800),CAMPAIGN)
 out=r8.PARENT/'cu8-checkpoint-flash-r1';identity_lock=copy.deepcopy(lock);identity_lock['build_path']['fresh_child']=out.name;policy=r8.identity_and_parent(o,out,identity_lock);out.mkdir()
 failed=o.read_json(inputs['failed_ancestry']['path']);before_rows=o.read_json(inputs['warm_checkpoint']['path']);tools=prior['toolchain'];source_rows=r12.changed_rows(o.read_json(req['parent_source_inventory']['path']),req['before']['sha256'],req['after']['sha256'],req['after']['bytes']);newlock=copy.deepcopy(lock);newlock['final_placement']['after_sha256']=req['after']['sha256'];old_inputs=o.read_json(dest/'frozen-input-closure.json');mirrors=[{'path':o.mirror_relative(r),'bytes':r['bytes'],'sha256':r['sha256']} for r in o.LOCK['dependencies']+lock['dependencies']]
 receipt={'schema':'airk.private.cuda-checkpoint-section/1','status':'RUNNING','qualification':False,'section_id':'flash-remainder','section_index':0,'plan':plan_pin,'runner':pin(Path(__file__)),'input_contract':pin(HERE/'input-contract.json'),'predecessor':None,'failed_ancestry':failed,'warm_checkpoint_before':inputs['warm_checkpoint'],'original_section_seconds':10800,'started_utc':started_utc.isoformat(),'observed_monotonic_seconds':started_mono,'absolute_cutoff_utc':cutoff.isoformat(),'converted_monotonic_deadline':deadline,'global_campaign_cutoff_utc':CAMPAIGN.isoformat(),'output_policy':policy,'native_argv':section['argv'],'cold_build_qualified':False}
 def verify_inputs():
  for row in inputs['pins']:o.verify_file(o.plain(row['path'],regular=True),row)
  for row in failed:o.verify_file(o.plain(row['path'],regular=True),row)
  if digest(__file__)!=a.approved_script_sha256 or digest(HERE/'input-contract.json')!=a.approved_input_sha256:raise ValueError('section producer/input source drift')
 def source_manifest():
  verify_inputs()
  for row in tools.values():
   if isinstance(row,dict) and {'path','bytes','sha256'}.issubset(row):o.verify_file(o.plain(row['path'],regular=True),row)
  r8.verify_input_closure(o,old_inputs);r8.verify_mirror_closure(o,dest/'source/mirror',mirrors)
  if o.source_inventory(dest/'source',exclude_mirror=True)!=source_rows or o.source_inventory(dest/'frontend')!=o.read_json(dest/'frontend-source-files.json'):raise ValueError('actual compiled source changed')
  for row in o.read_json(dest/'cuda-toolkit-inputs.json'):o.verify_file(o.child(r8.CUDA,row['path']),row)
  cudnn=o.read_json(r8.INPUT/'extraction.json')
  for row in cudnn['files']:o.verify_file(o.child(r8.INPUT/'cudnn-dev',row['path']),row)
  preflight=r8.configured_build_preflight(o,dest,prior['configure_argv'],newlock)
  if preflight!=plan['configured_preflight']:raise ValueError('actual source/target/cache/configuration changed')
  return {'schema':'airk.private.cuda-materialized-source/1','actual_build_root':str(dest),'corrected_capi_sha256':req['after']['sha256'],'source_inventory_sha256':digest(Path(plan['current_source_inventory']['path'])),'frontend_inventory_sha256':digest(dest/'frontend-source-files.json'),'cuda_inventory_sha256':digest(dest/'cuda-toolkit-inputs.json'),'frozen_original_inputs_sha256':digest(dest/'frozen-input-closure.json'),'tools':tools,'configured_preflight':preflight,'configure_argv':prior['configure_argv'],'policy':prior['policy'],'failed_r12_sha256':plan['previous_failed_R12']['sha256']}
 try:
  r8.write(out/'start.json',receipt);before=source_manifest();r8.write(out/'source-before.json',before);receipt['source_manifest_before']=pin(out/'source-before.json')
  if pin(build/'.ninja_log')!=plan['ninja_log'] or pin(build/'.ninja_deps')!=plan['ninja_deps']:raise ValueError('baseline Ninja state changed before first section')
  receipt['ninja_log_before']=pin(build/'.ninja_log');receipt['ninja_deps_before']=pin(build/'.ninja_deps')
  for row in before_rows:o.verify_file(o.plain(build/row['path'],regular=True),row)
  remaining=min(deadline-time.monotonic(),(cutoff-datetime.datetime.now(datetime.timezone.utc)).total_seconds())
  if remaining<=0:raise TimeoutError('fixed section budget exhausted before native command')
  env=load(r12_path/'producer_environment.py','same_r12_section_environment').producer_environment(out,r8.CUDA);tree=load(r12_path/'process_tree.py','same_r12_section_tree');actual=o.windows_command(section['argv'],tools,out,'flash-section');receipt['original_command_argv']=actual;receipt['command']=tree.run_tree(o,actual,dest,out,'flash-section',remaining,env)
  if not o.successful(receipt['command']):raise ValueError('actual Flash target tree failed or not naturally settled')
  after=source_manifest();r8.write(out/'source-after.json',after);receipt['source_manifest_after']=pin(out/'source-after.json')
  if before!=after:raise ValueError('source/tool/config identity differs after section')
  records=ninja_records(build/'.ninja_log');after_rows={}
  for row in before_rows:
   path=o.plain(build/row['path'],regular=True);observed=pin(path);record=records.get(row['path']) or records.get(path.as_posix());new_log_required(row,observed['sha256'],record);after_rows[row['path']]={**row,'bytes':observed['bytes'],'sha256':observed['sha256'],'mtime_ns':path.stat().st_mtime_ns,'ninja_success_record':record or row['ninja_success_record']}
  text=(build/'build.ninja').read_text();line=next(x for x in text.splitlines() if x.startswith('build onnxruntime_providers_cuda_flash_attention: phony '));names=[x.replace('\\','/') for x in line.split(': phony ',1)[1].split()]
  if len(names)!=48:raise ValueError('exact48 Flash target objects required')
  for name in names:
   path=o.plain(build/name,regular=True);record=records.get(name) or records.get(path.as_posix())
   if record is None:raise ValueError('Flash output lacks actual successful Ninja record')
   item=pin(path);after_rows[name]={'path':name,'bytes':item['bytes'],'sha256':item['sha256'],'mtime_ns':path.stat().st_mtime_ns,'ninja_success_record':record,'rule':'CUDA_COMPILER__onnxruntime_providers_cuda_flash_attention_unscanned_Release'}
  r8.write(out/'warm-output-after.json',sorted(after_rows.values(),key=lambda x:x['path']));receipt['warm_checkpoint_after']=pin(out/'warm-output-after.json');receipt['ninja_log_after']=pin(build/'.ninja_log');receipt['ninja_deps_after']=pin(build/'.ninja_deps');receipt['flash_outputs_completed']=48;verify_inputs()
  if time.monotonic()>=deadline or datetime.datetime.now(datetime.timezone.utc)>=cutoff:raise TimeoutError('fixed section/campaign cutoff expired during final closure')
  receipt['status']='PASS_TARGET_SECTION_ONLY_UNQUALIFIED'
 except BaseException as exc:receipt.update(status='FAILED_PRESERVED_NO_RETRY',error=type(exc).__name__+': '+str(exc));raise
 finally:receipt['elapsed_seconds']=time.monotonic()-started_mono;receipt['completed_utc']=datetime.datetime.now(datetime.timezone.utc).isoformat();r8.write(out/'receipt.json',receipt)
 print(json.dumps({'status':receipt['status'],'output':str(out)}))
if __name__=='__main__':main()
