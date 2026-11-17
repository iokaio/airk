# SPDX-License-Identifier: Apache-2.0
"""Root-only explicit final checkpoint composition; no build or runtime activation."""
import argparse,datetime,hashlib,importlib.util,json,math,time
from pathlib import Path
HERE=Path(__file__).resolve().parent
def digest(p):
 h=hashlib.sha256()
 with Path(p).open('rb') as f:
  for b in iter(lambda:f.read(4*1024*1024),b''):h.update(b)
 return h.hexdigest()
def pin(p):p=Path(p);return {'path':str(p),'bytes':p.stat().st_size,'sha256':digest(p)}
def load(p,n):
 s=importlib.util.spec_from_file_location(n,p);m=importlib.util.module_from_spec(s);s.loader.exec_module(m);return m
def same_bytes(a,b):return (a['bytes'],a['sha256'])==(b['bytes'],b['sha256'])
def normalized_custody(o,index,section,all_sections):
 if index==0:
  declared=all_sections[1]['predecessor_custody'];o.verify_file(o.plain(declared['path'],regular=True),declared);body=o.read_json(declared['path'])
  if body.get('schema')!='airk.private.cuda-first-section-custody/1' or body.get('phase_receipt_unchanged') is not True:raise ValueError('explicit first immutable custody bridge required')
  result={}
  for name in ['ninja_log','ninja_deps']:
   row=next(r for r in body['rows'] if r['kind']==name)
   if row['before_original']!=section[name+'_before'] or row['after_original']!=section[name+'_after'] or not same_bytes(row['before_original'],row['before_copy']) or not same_bytes(row['after_original'],row['after_copy']):raise ValueError('first original/copied Ninja identities differ')
   result['before_'+('log' if name=='ninja_log' else 'deps')]=row['before_copy'];result['after_'+('log' if name=='ninja_log' else 'deps')]=row['after_copy']
 else:result={'before_log':section['ninja_log_before'],'before_deps':section['ninja_deps_before'],'after_log':section['ninja_log_after'],'after_deps':section['ninja_deps_after']}
 for row in result.values():o.verify_file(o.plain(row['path'],regular=True),row)
 return result
def validate_sections(o,helper,rows,expected_successor_source,expected_successor_input,plan_sha):
 if len(rows)!=3:raise ValueError('exact three ordered actual sections required')
 bodies=[]
 for index,row in enumerate(rows):
  o.verify_file(o.plain(row['path'],regular=True),row);body=o.read_json(row['path']);helper.verify_section(body,index,o,plan_sha)
  if body.get('cold_build_qualified') is not False or not math.isfinite(body['elapsed_seconds']) or body['elapsed_seconds']<0:raise ValueError('no cold qualification/invalid elapsed')
  if index and (body['runner']['sha256']!=expected_successor_source or body['input_contract']['sha256']!=expected_successor_input or body.get('predecessor')!=rows[index-1]):raise ValueError('exact successor/predecessor source ancestry differs')
  if index and body['warm_checkpoint_before']!=bodies[-1]['warm_checkpoint_after']:raise ValueError('warm checkpoint chain differs')
  if index and o.read_json(body['source_manifest_after']['path'])!=o.read_json(bodies[0]['source_manifest_after']['path']):raise ValueError('section source/tool/config changed')
  bodies.append(body)
 return bodies
def main():
 ap=argparse.ArgumentParser();ap.add_argument('--execute-compose',action='store_true');ap.add_argument('--approved-script-sha256',required=True);ap.add_argument('--approved-input-sha256',required=True)
 for name in ['flash','llm','final']:ap.add_argument('--'+name+'-receipt',type=Path,required=True);ap.add_argument('--'+name+'-sha256',required=True)
 a=ap.parse_args()
 if not a.execute_compose or digest(__file__)!=a.approved_script_sha256 or digest(HERE/'input-contract.json')!=a.approved_input_sha256:raise ValueError('explicit reviewed composition source/input required')
 started=time.monotonic();observed_utc=datetime.datetime.now(datetime.timezone.utc);limit=min(300,(datetime.datetime(2026,11,19,5,29,42,tzinfo=datetime.timezone.utc)-observed_utc).total_seconds());deadline=started+limit
 if limit<=0:raise ValueError('campaign metadata composition cutoff expired')
 inputs=json.loads((HERE/'input-contract.json').read_bytes())
 for row in inputs['pins']:
  if pin(row['path'])!=row:raise ValueError('frozen composition input drift before import')
 helper=load(inputs['successor_runner']['path'],'exact_successor_composition');r12=Path(inputs['r12_proposal']);request=json.loads((r12/'continuation-request.json').read_bytes());r8=load(request['recipe']['path'],'r8_composition_owner');lock=json.loads(Path(request['recipe_lock']['path']).read_bytes());o=r8.cpu_owner(lock)
 for row in inputs['pins']:o.verify_file(o.plain(row['path'],regular=True),row)
 rows=[]
 for name in ['flash','llm','final']:
  path=o.plain(getattr(a,name+'_receipt'),regular=True);row=pin(path)
  if row['sha256']!=getattr(a,name+'_sha256'):raise ValueError('actual reviewed section receipt differs')
  rows.append(row)
 plan=o.read_json(inputs['approved_plan']['path']);bodies=validate_sections(o,helper,rows,inputs['successor_runner']['sha256'],inputs['successor_input']['sha256'],inputs['approved_plan']['sha256'])
 verifier=load(inputs['tree_verifier']['path'],'strict_actual_tree_composition')
 for index,body in enumerate(bodies):
  argv=plan['sections'][index]['argv']
  if body['native_argv']!=argv:raise ValueError('actual exact section target argv differs')
  section_root=Path(rows[index]['path']).parent;stage=['flash-section','llm-section','final-section'][index]
  verifier.verify_tree_command(o,None,r12,section_root,Path(plan['actual_build_root']),stage,argv,o.read_json(plan['previous_failed_R12']['path'])['toolchain'],body['command'],{'observed_utc':body['started_utc']},datetime.datetime.fromisoformat(body['absolute_cutoff_utc']))
 custodies=[normalized_custody(o,i,b,bodies) for i,b in enumerate(bodies)]
 for index in range(1,3):
  for kind in ['log','deps']:
   if not same_bytes(custodies[index-1]['after_'+kind],custodies[index]['before_'+kind]):raise ValueError('immutable Ninja state continuity differs')
 source=helper.verify_materialized(inputs,plan)
 if source!=o.read_json(bodies[-1]['source_manifest_after']['path']):raise ValueError('actual final materialized source differs')
 dest=o.plain(plan['actual_build_root']);build=dest/'b/Release';final_warm=o.read_json(bodies[-1]['warm_checkpoint_after']['path'])
 for row in final_warm:o.verify_file(o.plain(build/row['path'],regular=True),row)
 for kind in ['log','deps']:
  path=build/('.ninja_log' if kind=='log' else '.ninja_deps')
  if not same_bytes(pin(path),custodies[-1]['after_'+kind]):raise ValueError('actual final Ninja state changed')
 archive=load(inputs['archive_owner']['path'],'pinned_static_target_graph');roots=['onnxruntime_'+x+'.lib' for x in o.STATIC_COMPONENTS]+['_deps/re2-build/re2.lib','_deps/pytorch_cpuinfo-build/cpuinfo.lib'];graph=archive.runtime_closure((build/'build.ninja').read_text(),roots)
 if len(graph['static_inputs'])!=96 or graph['provider_modules']['onnxruntime_providers_cuda.dll']['import_library'] is not None:raise ValueError('exact96 static targets/no fake CUDA import library required')
 libraries=[]
 for path in sorted((dest/'b').rglob('*.lib')):
  path=o.plain(path,regular=True)
  with path.open('rb') as f:
   if f.read(8)!=b'!<arch>\n':raise ValueError('invalid actual output archive')
  row=pin(path);row['path']=path.relative_to(dest).as_posix();libraries.append(row)
 recorded={r['path']:r for r in libraries}
 for name in graph['static_inputs']:
  path='b/Release/'+name
  if path not in recorded:raise ValueError('actual required static target archive missing')
 pair=[]
 for name in ['onnxruntime_providers_shared.dll','onnxruntime_providers_cuda.dll']:
  found=list((dest/'b').rglob(name))
  if len(found)!=1:raise ValueError('exact same-build provider pair missing/ambiguous')
  path=o.plain(found[0],regular=True);row=pin(path);row['path']=path.relative_to(dest).as_posix();pair.append(row)
 if list((dest/'b').rglob('onnxruntime.dll')):raise ValueError('second dynamic ORT core refused')
 out=r8.PARENT/'cu8-checkpointed-final-r1';guard=json.loads(json.dumps(lock));guard['build_path']['fresh_child']=out.name;policy=r8.identity_and_parent(o,out,guard);out.mkdir()
 if limit<=0:raise ValueError('campaign metadata composition cutoff expired')
 receipt={'schema':'airk.private.cuda-checkpointed-build/1','status':'RUNNING','mode':'checkpointed-warm','qualification':False,'cold_build_qualified':False,'plan':inputs['approved_plan'],'producer_source':pin(Path(__file__)),'input_contract':pin(HERE/'input-contract.json'),'original_failed_build':plan['previous_failed_R12'],'failed_ancestry':[{k:r[k] for k in ['path','bytes','sha256']} for r in o.read_json(inputs['failed_ancestry']['path'])],'warm_baseline':inputs['warm_checkpoint'],'warm_final':bodies[-1]['warm_checkpoint_after'],'sections':[{'section_id':b['section_id'],'section_index':i,'receipt':rows[i],'ninja_state_custody':custodies[i]} for i,b in enumerate(bodies)],'source_manifest':bodies[-1]['source_manifest_after'],'actual_build_root':str(dest),'source_inventory_sha256':source['source_inventory_sha256'],'frontend_inventory_sha256':source['frontend_inventory_sha256'],'cuda_inventory_sha256':source['cuda_inventory_sha256'],'frozen_original_inputs_sha256':source['frozen_original_inputs_sha256'],'ninja_sha256':source['configured_preflight']['ninja_sha256'],'target_closure_sha256':'','configure_argv':source['configure_argv'],'configured_preflight':source['configured_preflight'],'toolchain':source['tools'],'policy':source['policy'],'libraries':libraries,'provider_dlls':pair,'global_campaign_cutoff_utc':helper.CAMPAIGN.isoformat(),'total_active_section_seconds':sum(b['elapsed_seconds'] for b in bodies),'output_policy':policy}
 try:
  r8.write(out/'static-target-closure.json',graph);receipt['target_closure_sha256']=digest(out/'static-target-closure.json')
  first=o.read_json(inputs['first_attempt_start']['path']);origin=datetime.datetime.fromisoformat(first['started_utc']);now=datetime.datetime.now(datetime.timezone.utc);receipt['original_attempt_started_utc']=origin.isoformat();receipt['completed_utc']=now.isoformat();receipt['wall_since_first_attempt_seconds']=(now-origin).total_seconds()
  for row in inputs['pins']:o.verify_file(o.plain(row['path'],regular=True),row)
  validate_sections(o,helper,rows,inputs['successor_runner']['sha256'],inputs['successor_input']['sha256'],inputs['approved_plan']['sha256'])
  for row in receipt['failed_ancestry']:o.verify_file(o.plain(row['path'],regular=True),row)
  for row in libraries+pair:o.verify_file(o.child(dest,row['path']),row)
  if digest(__file__)!=a.approved_script_sha256 or digest(HERE/'input-contract.json')!=a.approved_input_sha256:raise ValueError('composer source/input drift')
  now=datetime.datetime.now(datetime.timezone.utc);receipt['completed_utc']=now.isoformat();receipt['wall_since_first_attempt_seconds']=(now-origin).total_seconds()
  if time.monotonic()>=deadline or now>=helper.CAMPAIGN:raise TimeoutError('fixed composition/campaign cutoff expired')
  receipt['status']='PASS_CHECKPOINTED_PRODUCER_ONLY_UNQUALIFIED'
 except BaseException as exc:receipt.update(status='FAILED_PRESERVED_NO_RETRY',error=type(exc).__name__+': '+str(exc));raise
 finally:receipt['metadata_composition_seconds']=time.monotonic()-started;r8.write(out/'gpu-checkpointed-build-receipt.json',receipt)
 print(json.dumps({'status':receipt['status'],'output':str(out)}))
if __name__=='__main__':main()
