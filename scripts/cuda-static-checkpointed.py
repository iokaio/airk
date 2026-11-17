# SPDX-License-Identifier: Apache-2.0
"""Exact checkpointed-warm producer bridge; no old failed receipt status alias."""
import datetime,importlib.util,math
from pathlib import Path
COMPOSER='096a69ca0500b240f5f0b54a9951decee26cff917be707702fb76861a3ee5199'
INPUT='e610c60b7dc684a84e43d962888bf2a4ebf5bde165cad379bbba43b9191824ba'
PLAN='6bee6b8b95a01fea28adf04117260d12503012ea8120ae225afa232b6589df97'
R12='3a4c18932dbafbcc719cb77b30f616372b220cc315a1acdba82df24a9734c026'
SUCCESSOR='797c6d2e1a97865f5a8b95adf4d68ad0d91e4cfcf72f8f43d0329a2bba352b1b'
SUCCESSOR_INPUT='c118835f41e95820fed5645c7de59606f583ff403a32f13422d5bb6120168c55'
def module(name):
 p=Path(__file__).with_name(name+'.py');s=importlib.util.spec_from_file_location(name.replace('-','_'),p);m=importlib.util.module_from_spec(s);s.loader.exec_module(m);return m

def require_header(v):
 if v.get('schema')!='airk.private.cuda-checkpointed-build/1' or v.get('status')!='PASS_CHECKPOINTED_PRODUCER_ONLY_UNQUALIFIED' or v.get('mode')!='checkpointed-warm' or v.get('qualification') is not False or v.get('cold_build_qualified') is not False or v.get('producer_source',{}).get('sha256')!=COMPOSER or v.get('input_contract',{}).get('sha256')!=INPUT or v.get('plan',{}).get('sha256')!=PLAN or v.get('original_failed_build',{}).get('sha256')!=R12:raise ValueError('exact successful checkpointed composite authority required')

def toolchain_pins(tools):
 return [{field:tools[k][field] for field in ['path','bytes','sha256']} for k in ['compiler','cmake','ninja','patch','python','vcvars']]

def execution_context(build,core):
 if build.get('schema')!='airk.cuda-static-build-proof/2':raise ValueError('checkpoint proof/2 required')
 pin=build['build_receipt'];core.verify_file(core.plain(pin['path'],regular=True),pin);v=core.read_json(pin['path']);require_header(v)
 ip=v['input_contract'];core.verify_file(core.plain(ip['path'],regular=True),ip);inputs=core.read_json(ip['path'])
 tools=v['toolchain']
 if build['toolchain_pins']!=toolchain_pins(tools):raise ValueError('canonical exact toolchain pins differ')
 for row in build['toolchain_pins']:core.verify_file(core.plain(row['path'],regular=True),row)
 return Path(inputs['r12_proposal']),tools

def verify_checkpointed(destination,composite_root,proposal,expected_receipt_sha256,repo_root):
 runtime=module('cuda-static-runtime');core=runtime.load(Path(repo_root)/'scripts/provision-static-onnx-runtime.py','cpu_checkpoint_acquisition_owner');destination=core.plain(destination);proposal=core.plain(proposal);composite_root=core.plain(composite_root)
 script=core.plain(proposal/'compose_checkpointed_build.py',regular=True);ip=core.plain(proposal/'input-contract.json',regular=True)
 if core.digest(script)!=COMPOSER or core.digest(ip)!=INPUT:raise ValueError('reviewed exact checkpoint composer/input bridge differs')
 raw=core.plain(composite_root/'gpu-checkpointed-build-receipt.json',regular=True)
 if core.digest(raw)!=expected_receipt_sha256:raise ValueError('actual completed checkpointed receipt pin differs')
 value=core.read_json(raw);require_header(value);inputs=core.read_json(ip)
 for row in inputs['pins']:core.verify_file(core.plain(row['path'],regular=True),row)
 if value['producer_source']!=runtime._pin(core,script) or value['input_contract']!=runtime._pin(core,ip) or value['plan']!=inputs['approved_plan']:raise ValueError('actual composite reviewed source/input identities differ')
 composer=runtime.load(script,'reviewed_checkpoint_composer_helpers');helper=runtime.load(inputs['successor_runner']['path'],'reviewed_checkpoint_successor_helpers');plan=core.read_json(inputs['approved_plan']['path'])
 if inputs['successor_runner']['sha256']!=SUCCESSOR or inputs['successor_input']['sha256']!=SUCCESSOR_INPUT or destination!=Path(plan['actual_build_root']) or composite_root!=destination.parent/'cu8-checkpointed-final-r1' or value['actual_build_root']!=str(destination):raise ValueError('exact checkpoint roots/successor bridge differ')
 if [row['section_index'] for row in value['sections']]!=[0,1,2]:raise ValueError('exact ordered three sections required')
 receipts=[row['receipt'] for row in value['sections']];bodies=composer.validate_sections(core,helper,receipts,SUCCESSOR,SUCCESSOR_INPUT,PLAN)
 normalized=[];tree=module('cuda-static-continuation-r12');r12=Path(inputs['r12_proposal']);prior=core.read_json(plan['previous_failed_R12']['path']);tools=prior['toolchain']
 for index,body in enumerate(bodies):
  expected=plan['sections'][index];custody=composer.normalized_custody(core,index,body,bodies);normalized.append({'section_id':expected['id'],'section_index':index,'receipt':receipts[index],'ninja_state_custody':custody})
  if body['native_argv']!=expected['argv']:raise ValueError('exact checkpoint target command differs')
  tree.verify_tree_command(core,None,r12,Path(receipts[index]['path']).parent,destination,['flash-section','llm-section','final-section'][index],expected['argv'],tools,body['command'],{'observed_utc':body['started_utc']},datetime.datetime.fromisoformat(body['absolute_cutoff_utc']))
 if normalized!=value['sections']:raise ValueError('exact immutable section state custody differs')
 for index in range(1,3):
  for kind in ['log','deps']:
   if not composer.same_bytes(normalized[index-1]['ninja_state_custody']['after_'+kind],normalized[index]['ninja_state_custody']['before_'+kind]):raise ValueError('checkpoint Ninja continuity differs')
 failed=[{k:r[k] for k in ['path','bytes','sha256']} for r in core.read_json(inputs['failed_ancestry']['path'])]
 if value['original_failed_build']!=plan['previous_failed_R12'] or value['failed_ancestry']!=failed or value['warm_baseline']!=inputs['warm_checkpoint'] or value['warm_final']!=bodies[-1]['warm_checkpoint_after']:raise ValueError('warm/original failed ancestry changed')
 for row in failed:core.verify_file(core.plain(row['path'],regular=True),row)
 if prior['status']!='FAILED_PRESERVED_NO_RETRY' or prior['commands'][-1].get('owned_job_final_active_processes')!=0 or not prior['commands'][-1].get('owned_descendants_settled'):raise ValueError('original failed R12 settlement changed')
 source=helper.verify_materialized(inputs,plan)
 if source!=core.read_json(bodies[-1]['source_manifest_after']['path']) or value['source_manifest']!=bodies[-1]['source_manifest_after']:raise ValueError('actual final source/tool/config closure differs')
 for key in ['source_inventory_sha256','frontend_inventory_sha256','cuda_inventory_sha256','frozen_original_inputs_sha256','configure_argv','configured_preflight','policy']:
  if value[key]!=source[key]:raise ValueError('composite materialized closure field differs: '+key)
 if value['toolchain']!=tools:raise ValueError('composite toolchain differs')
 for row in core.read_json(value['warm_final']['path']):core.verify_file(core.plain(destination/'b/Release'/row['path'],regular=True),row)
 ninja=runtime._pin(core,destination/'b/Release/build.ninja')
 if value['ninja_sha256']!=ninja['sha256'] or ninja['sha256']!=source['configured_preflight']['ninja_sha256']:raise ValueError('actual configured Ninja identity differs')
 for kind in ['log','deps']:
  if not composer.same_bytes(runtime._pin(core,destination/'b/Release'/('.ninja_log' if kind=='log' else '.ninja_deps')),normalized[-1]['ninja_state_custody']['after_'+kind]):raise ValueError('actual final Ninja state changed')
 archive=module('cuda-static-archive');roots=['onnxruntime_'+x+'.lib' for x in core.STATIC_COMPONENTS]+['_deps/re2-build/re2.lib','_deps/pytorch_cpuinfo-build/cpuinfo.lib'];graph=archive.runtime_closure((destination/'b/Release/build.ninja').read_text(),roots)
 if len(graph['static_inputs'])!=96 or core.read_json(composite_root/'static-target-closure.json')!=graph or core.digest(composite_root/'static-target-closure.json')!=value['target_closure_sha256']:raise ValueError('actual96 static target closure differs')
 libraries=[dict(runtime._pin(core,p),path=p.relative_to(destination).as_posix()) for p in sorted((destination/'b').rglob('*.lib'))]
 if libraries!=value['libraries'] or any('b/Release/'+name not in {r['path'] for r in libraries} for name in graph['static_inputs']):raise ValueError('actual static output census differs')
 pair=value['provider_dlls']
 if sorted(Path(r['path']).name for r in pair)!=['onnxruntime_providers_cuda.dll','onnxruntime_providers_shared.dll'] or list((destination/'b').rglob('onnxruntime.dll')):raise ValueError('same-build provider pair/one static core required')
 for row in pair:core.verify_file(core.child(destination,row['path']),row)
 for name in ['onnxruntime_providers_cuda.dll','onnxruntime_providers_shared.dll']:
  if len(list((destination/'b').rglob(name)))!=1:raise ValueError('ambiguous actual provider output')
 completed=datetime.datetime.fromisoformat(value['completed_utc']);origin=datetime.datetime.fromisoformat(core.read_json(inputs['first_attempt_start']['path'])['started_utc'])
 if value['original_attempt_started_utc']!=origin.isoformat() or completed>=helper.CAMPAIGN or completed<max(datetime.datetime.fromisoformat(b['completed_utc']) for b in bodies) or value['global_campaign_cutoff_utc']!=helper.CAMPAIGN.isoformat() or value['wall_since_first_attempt_seconds']!=(completed-origin).total_seconds() or value['total_active_section_seconds']!=sum(b['elapsed_seconds'] for b in bodies) or not 0<=value['metadata_composition_seconds']<300:raise ValueError('checkpoint chronology/metadata deadline differs')
 for row in inputs['pins']:core.verify_file(core.plain(row['path'],regular=True),row)
 if core.digest(raw)!=expected_receipt_sha256:raise ValueError('composite changed during verification')
 return {'schema':'airk.cuda-static-build-proof/2','target':runtime.TARGET,'ort_version':runtime.VERSION,'build_receipt':runtime._pin(core,raw),'producer_source':value['producer_source'],'approved_plan_sha256':PLAN,'original_failed_R12':value['original_failed_build'],'failed_ancestry':failed,'sections':normalized,'source_manifest':value['source_manifest'],'source_inventory_sha256':value['source_inventory_sha256'],'frontend_inventory_sha256':value['frontend_inventory_sha256'],'cuda_inventory_sha256':value['cuda_inventory_sha256'],'frozen_input_closure_sha256':value['frozen_original_inputs_sha256'],'ninja':ninja,'static_target_closure':{'sha256':value['target_closure_sha256'],'count':96},'libraries':libraries,'provider_dlls':pair,'toolchain_pins':toolchain_pins(tools),'qualification':False,'activation_pins_available':False}
