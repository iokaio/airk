# SPDX-License-Identifier: Apache-2.0
"""Additive exact R9 continuation proof; original R8/CPU owners unchanged."""
from pathlib import Path
import copy,datetime,importlib.util,os
R9_SCRIPT='e93ac8f7ddca94642f09deef952396b1a7ff4457ac9e61e3a68261a0724238e9'
REQUEST='816a866cd757d4642cdd729e12e53fe1552ee9ea4ec22be4a7e5823d1843cd57'
BASELINE='504285d4158092583465a919f8a42be4f7144696c1cfe72888ca6f1d89dbfebe'
WALL='0b86c7f034fd949e36523bc6adece4263e4212533edf494bf19b0d08c19d6088'
R8_SCRIPT='c2f9c3e305106469d6a58699eaa36ec2afed7a6cd4be63275498fd78af2d3f4a'
R8_LOCK='549be258fd21be030f994f6da7f74146a9e9edd3bb611bee83305d9a4deade6d'
AFTER='7acabdbefcd7ee3936305ff150fc2369c3d341b6838e10113435ebab03378ab8'

def module(name):
 p=Path(__file__).with_name(name+'.py');s=importlib.util.spec_from_file_location(name.replace('-','_'),p);m=importlib.util.module_from_spec(s);s.loader.exec_module(m);return m

def verify_continuation(destination,continuation,proposal,expected_receipt_sha256,repo_root):
 runtime=module('cuda-static-runtime');o=runtime.load(Path(repo_root)/'scripts/provision-static-onnx-runtime.py','cpu_owner_r9_verifier')
 destination=o.plain(destination);continuation=o.plain(continuation);proposal=o.plain(proposal)
 pins={'continue_cuda_runtime.py':R9_SCRIPT,'continuation-request.json':REQUEST,'continuation-baseline.json':BASELINE,'wall-origin.json':WALL}
 for name,sha in pins.items():
  if o.digest(o.plain(proposal/name,regular=True))!=sha:raise runtime.GpuProofUnavailable('R9 reviewed source/input bridge differs')
 req=o.read_json(proposal/'continuation-request.json');baseline=o.read_json(proposal/'continuation-baseline.json');wall=o.read_json(proposal/'wall-origin.json')
 if destination!=Path(req['destination']) or continuation!=destination.parent/'cu8-continuation-r9':raise runtime.GpuProofUnavailable('R9 exact build/continuation roots differ')
 raw=o.plain(continuation/'gpu-build-continuation-receipt.json',regular=True)
 if o.digest(raw)!=expected_receipt_sha256:raise runtime.GpuProofUnavailable('actual successful continuation receipt pin differs')
 value=o.read_json(raw)
 if value.get('schema')!='airk.private.cuda-runtime-continuation/1' or value.get('status')!='PASS_PRODUCER_ONLY_UNQUALIFIED' or value.get('request_sha256')!=REQUEST or value.get('baseline_sha256')!=BASELINE or value.get('wall_origin_sha256')!=WALL or value.get('continuation_script_sha256')!=R9_SCRIPT:raise runtime.GpuProofUnavailable('actual reviewed R9 PASS unavailable')
 for name in ['recipe','recipe_lock','before','after','patch','syntax_card','syntax_response','parent_receipt','parent_failed_log','parent_source_inventory']:o.verify_file(o.plain(req[name]['path'],regular=True),req[name])
 if req['recipe']['sha256']!=R8_SCRIPT or req['recipe_lock']['sha256']!=R8_LOCK or req['after']['sha256']!=AFTER:raise runtime.GpuProofUnavailable('R9 ancestry source bridge differs')
 recipe=runtime.load(req['recipe']['path'],'exact_r8_for_r9');lock=o.read_json(req['recipe_lock']['path']);recipe.cpu_owner(lock)
 child=runtime.load(proposal/'continue_cuda_runtime.py','exact_r9_verifier_helpers');parent=o.read_json(req['parent_receipt']['path']);child.verify_parent(parent,o)
 for key in ['parent_failed_receipt','parent_failed_log','parent_source_inventory']:
  parent_key={'parent_failed_receipt':'parent_receipt','parent_failed_log':'parent_failed_log','parent_source_inventory':'parent_source_inventory'}[key]
  if value.get(key)!=req[parent_key]:raise runtime.GpuProofUnavailable('exact original failed ancestry pin differs')
 if value.get('policy')!=parent.get('policy') or value.get('toolchain')!=parent.get('toolchain') or value.get('frozen_parent_metadata')!=baseline or value.get('configure_argv')!=parent.get('configure_argv') or not value.get('original_source_preserved') or not value.get('source_applied'):raise runtime.GpuProofUnavailable('R9 changed unrelated build policy/ancestry')
 for item in baseline:o.verify_file(o.plain(item['path'],regular=True),item)
 for original,name in [('before','onnxruntime_c_api.cc.before'),('parent_receipt','parent-failed-receipt.json'),('parent_source_inventory','parent-gpu-source-files.json')]:o.verify_file(o.plain(continuation/name,regular=True),req[original])
 tools=parent['toolchain']
 for name in ['compiler','cmake','ninja','patch','python','vcvars']:o.verify_file(o.plain(tools[name]['path'],regular=True),tools[name])
 syntax_pin=value['syntax_receipt'];syntax_path=o.plain(syntax_pin['path'],regular=True);o.verify_file(syntax_path,syntax_pin);syntax=o.read_json(syntax_path)
 if syntax.get('status')!='PASS_SYNTAX_ONLY' or syntax.get('request_sha256')!=REQUEST or syntax.get('source_sha256')!=AFTER or not o.successful(syntax.get('command',{})):raise runtime.GpuProofUnavailable('actual corrected CAPI syntax proof missing')
 before=o.read_json(req['parent_source_inventory']['path']);expected=child.changed_rows(before,req['before']['sha256'],AFTER,req['after']['bytes'])
 if o.source_inventory(destination/'source',exclude_mirror=True)!=expected or o.read_json(continuation/'gpu-source-files.json')!=expected or o.digest(continuation/'gpu-source-files.json')!=value.get('source_inventory_sha256'):raise runtime.GpuProofUnavailable('R9 complete source delta differs')
 frontend=o.read_json(destination/'frontend-source-files.json');cuda=o.read_json(destination/'cuda-toolkit-inputs.json')
 if o.source_inventory(destination/'frontend')!=frontend or o.digest(destination/'frontend-source-files.json')!=value.get('frontend_inventory_sha256') or o.digest(destination/'cuda-toolkit-inputs.json')!=value.get('cuda_inventory_sha256'):raise runtime.GpuProofUnavailable('R9 frontend/CUDA census differs')
 for item in cuda:o.verify_file(o.child(recipe.CUDA,item['path']),item)
 cudnn=o.read_json(recipe.INPUT/'extraction.json')
 if o.digest(recipe.INPUT/'extraction.json')!=lock['cudnn']['extraction_receipt_sha256']:raise runtime.GpuProofUnavailable('official cuDNN pin differs')
 for item in cudnn['files']:o.verify_file(o.child(recipe.INPUT/'cudnn-dev',item['path']),item)
 mirrors=[{'path':o.mirror_relative(item),'bytes':item['bytes'],'sha256':item['sha256']} for item in o.LOCK['dependencies']+lock['dependencies']];recipe.verify_mirror_closure(o,destination/'source/mirror',mirrors)
 closure=o.read_json(continuation/'frozen-input-closure.json');recipe.verify_input_closure(o,closure)
 if len(closure)!=value.get('reopened_frozen_input_count') or o.digest(continuation/'frozen-input-closure.json')!=value.get('frozen_input_closure_sha256'):raise runtime.GpuProofUnavailable('new continuation input closure differs')
 old_closure=o.read_json(destination/'frozen-input-closure.json')
 if closure[:len(old_closure)]!=old_closure or len(old_closure)!=44:raise runtime.GpuProofUnavailable('original44 frozen inputs not retained')
 expected_inputs=old_closure+[child.row(o.plain(proposal/name,regular=True)) for name in ['continuation-request.json','continuation-baseline.json','wall-origin.json','continue_cuda_runtime.py','onnxruntime_c_api.cc.before','onnxruntime_c_api.cc','initialized-public-api.patch','syntax-card.json','capi-syntax.rsp']]+[child.row(syntax_path)]
 if closure!=expected_inputs:raise runtime.GpuProofUnavailable('exact continuation input roster differs')
 o.verify_file(o.plain(wall['start_file']['path'],regular=True),wall['start_file']);origin=o.read_json(wall['start_file']['path']);started=datetime.datetime.fromisoformat(origin['started_utc']);observed=datetime.datetime.fromisoformat(value['observed_utc']);cutoff=started+datetime.timedelta(seconds=10800)
 active=parent['elapsed_seconds']+syntax['elapsed_seconds'];remaining=child.wall_remaining(started,observed,active)
 if origin.get('outer_timeout_seconds')!=10800 or value.get('original_absolute_cutoff_utc')!=cutoff.isoformat() or wall['absolute_cutoff_utc']!=cutoff.isoformat() or value.get('remaining_seconds_at_start')!=remaining or value.get('consumed_seconds_at_start')!=10800-remaining or value.get('wall_gap_charged') is not True or value.get('elapsed_seconds',10801)>remaining:raise runtime.GpuProofUnavailable('original shared wall budget not preserved')
 newlock=copy.deepcopy(lock);newlock['final_placement']['after_sha256']=AFTER;preflight=recipe.configured_build_preflight(o,destination,parent['configure_argv'],newlock)
 if value.get('configured_preflight')!=preflight:raise runtime.GpuProofUnavailable('actual corrected CAPI target/configuration differs')
 commands=value.get('commands',[])
 if len(commands)!=2 or any(not o.successful(c) for c in commands):raise runtime.GpuProofUnavailable('actual settled continuation build commands missing')
 for (stage,targets),command in zip([('core-session-build-r9',['onnxruntime_session']),('build-r9',['all','re2'])],commands):
  argv=[tools['cmake']['path'],'--build',str(destination/'b/Release'),'--config','Release','--target',*targets,'--parallel','2']
  expected_command=[str(Path(os.environ.get('SystemRoot','C:/Windows'))/'System32/cmd.exe'),'/d','/c',str(continuation/'commands'/(stage+'.cmd'))]
  if value.get(stage+'_argv')!=argv or command.get('argv')!=expected_command or os.path.normcase(command.get('log',''))!=os.path.normcase(str(continuation/'logs'/(stage+'.log'))):raise runtime.GpuProofUnavailable('actual continuation target/log attribution differs')
 libraries=[{'path':q.relative_to(destination).as_posix(),'bytes':q.stat().st_size,'sha256':o.digest(o.plain(q,regular=True))} for q in sorted((destination/'b').rglob('*.lib'))]
 if value.get('libraries')!=libraries:raise runtime.GpuProofUnavailable('actual continuation library census differs')
 pair=value.get('provider_dlls',[])
 if sorted(Path(item['path']).name for item in pair)!=['onnxruntime_providers_cuda.dll','onnxruntime_providers_shared.dll']:raise runtime.GpuProofUnavailable('same-build provider pair missing')
 for item in pair:o.verify_file(o.child(destination,item['path']),item)
 if list((destination/'b').rglob('onnxruntime.dll')):raise runtime.GpuProofUnavailable('second dynamic core refused')
 ninja=destination/'b/Release/build.ninja'
 return {'schema':'airk.cuda-static-build-proof/1','target':runtime.TARGET,'ort_version':runtime.VERSION,'producer_script_sha256':R9_SCRIPT,'producer_lock_sha256':REQUEST,'build_receipt':child.row(raw),'source_inventory_sha256':value['source_inventory_sha256'],'frontend_inventory_sha256':value['frontend_inventory_sha256'],'cuda_inventory_sha256':value['cuda_inventory_sha256'],'frozen_input_closure_sha256':value['frozen_input_closure_sha256'],'ninja':child.row(ninja),'libraries':libraries,'provider_dlls':pair,'recorded_compiler':tools['compiler'],'toolchain':tools,'qualification':False,'activation_pins_available':False,'ancestry':{'original_failed_receipt':req['parent_receipt'],'original_r8_script':R8_SCRIPT,'original_r8_lock':R8_LOCK,'corrected_capi_sha256':AFTER,'baseline_sha256':BASELINE,'wall_origin_sha256':WALL,'actual_syntax_receipt':syntax_pin}}
