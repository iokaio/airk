# SPDX-License-Identifier: Apache-2.0
"""Separate exact R12 /2 continuation proof; original R8/CPU owners unchanged."""
from pathlib import Path
import copy,datetime,importlib.util,os,subprocess,re
R12_SCRIPT='c4a4cf3f06f9cc645af67d70200add6236c6f9f7a1e8d4d46a3f65d2604c517f'
ENVIRONMENT='d22c0fa24228309040a6a59dbdd6fbf2296408de354a7fb6d2925225a9f22259'
DIAGNOSTICS='e52cac836fb6d01d5630474f498732a41c37a614784db34a39a203e8ab3c8834'
REQUEST='816a866cd757d4642cdd729e12e53fe1552ee9ea4ec22be4a7e5823d1843cd57'
BASELINE='504285d4158092583465a919f8a42be4f7144696c1cfe72888ca6f1d89dbfebe'
WALL='0b86c7f034fd949e36523bc6adece4263e4212533edf494bf19b0d08c19d6088'
R8_SCRIPT='c2f9c3e305106469d6a58699eaa36ec2afed7a6cd4be63275498fd78af2d3f4a'
R8_LOCK='549be258fd21be030f994f6da7f74146a9e9edd3bb611bee83305d9a4deade6d'
AFTER='7acabdbefcd7ee3936305ff150fc2369c3d341b6838e10113435ebab03378ab8'

def module(name):
 p=Path(__file__).with_name(name+'.py');s=importlib.util.spec_from_file_location(name.replace('-','_'),p);m=importlib.util.module_from_spec(s);s.loader.exec_module(m);return m

def canonical_command_bytes(argv,tools,continuation,stage):
 runtime=module('cuda-static-runtime')
 values=[*argv,tools['vcvars']['path'],tools['windows_sdk'],tools['msvc_toolset'],str(continuation)]
 if any(re.search(r'[\x00-\x1f"%!?^&|<>]',v) for v in values):raise runtime.GpuProofUnavailable('unsafe original command path')
 text=('@echo off\r\ncall "'+tools['vcvars']['path']+'" '+tools['windows_sdk']+' -vcvars_ver='+tools['msvc_toolset']+' > "'+str(continuation/'logs'/(stage+'-toolchain.log'))+'" 2>&1\r\nif errorlevel 1 exit /b %errorlevel%\r\n'+subprocess.list2cmdline(argv)+'\r\nexit /b %errorlevel%\r\n')
 return text.encode()

def verify_tree_command(o,child,proposal,continuation,cwd,stage,argv,tools,command,value,cutoff):
 runtime=module('cuda-static-runtime');script=o.plain(continuation/'commands'/(stage+'.cmd'),regular=True)
 if script.read_bytes()!=canonical_command_bytes(argv,tools,continuation,stage):raise runtime.GpuProofUnavailable('actual canonical command bytes differ')
 original=[str(Path(os.environ.get('SystemRoot','C:/Windows'))/'System32/cmd.exe'),'/d','/c',str(script)]
 owner=runtime.load(proposal/'process_tree.py','exact_r12_tree_contract');request=owner.command_request(original,cwd,continuation,stage);request_path=o.plain(continuation/(stage+'-tree-request.json'),regular=True)
 if o.read_json(request_path)!=request:raise runtime.GpuProofUnavailable('actual tree request differs from exact original command')
 expected=[str(owner.POWERSHELL),'-NoLogo','-NoProfile','-NonInteractive','-File',str(proposal/'wait_process_tree.ps1'),'-Request',str(request_path)]
 if command.get('argv')!=expected or command.get('tree_request')!=str(request_path) or os.path.normcase(command.get('log',''))!=os.path.normcase(str(continuation/'logs'/(stage+'.log'))) or not o.successful(command):raise runtime.GpuProofUnavailable('actual natural complete command tree missing')
 started=o.read_json(o.plain(request['started'],regular=True));result=o.read_json(o.plain(request['result'],regular=True))
 if result!=command.get('actual_tree_result') or result.get('schema')!='airk.private.process-tree-result/1' or result.get('exit_code')!=0 or result.get('wait')!='Start-Process-Wait-process-and-descendants' or result.get('powershell')!='7.6.10' or result.get('command_pid')!=started.get('pid') or result.get('wrapper_pid')!=started.get('wrapper_pid') or not isinstance(result.get('command_pid'),int) or result['command_pid']<=0 or result.get('wrapper_pid')==result.get('command_pid'):raise runtime.GpuProofUnavailable('actual original command/tree identity differs')
 first=datetime.datetime.fromisoformat(result['start_utc']);last=datetime.datetime.fromisoformat(result['finished_utc']);observed=datetime.datetime.fromisoformat(value['observed_utc'])
 if first.utcoffset()!=datetime.timedelta(0) or last.utcoffset()!=datetime.timedelta(0) or not observed<=first<=last<=cutoff:raise runtime.GpuProofUnavailable('actual tree completion outside original absolute operation cutoff')
 for name in ['stdout','stderr']:o.plain(request[name],regular=True)
 return {'request_sha256':o.digest(request_path),'result_sha256':o.digest(request['result']),'command_sha256':o.digest(script)}

def require_original_input_prefix(old_closure,closure):
 if len(old_closure)!=44 or closure[:44]!=old_closure:raise ValueError('original44 frozen inputs not retained exactly')

def require_r12_header(value):
 runtime=module('cuda-static-runtime')
 if value.get('schema')!='airk.private.cuda-runtime-continuation/2' or value.get('status')!='PASS_PRODUCER_ONLY_UNQUALIFIED' or value.get('request_sha256')!=REQUEST or value.get('baseline_sha256')!=BASELINE or value.get('wall_origin_sha256')!=WALL or value.get('continuation_script_sha256')!=R12_SCRIPT:raise runtime.GpuProofUnavailable('actual reviewed R12 PASS unavailable')

def verify_continuation(destination,continuation,proposal,expected_receipt_sha256,repo_root):
 runtime=module('cuda-static-runtime');o=runtime.load(Path(repo_root)/'scripts/provision-static-onnx-runtime.py','cpu_owner_r12_verifier')
 destination=o.plain(destination);continuation=o.plain(continuation);proposal=o.plain(proposal)
 pins={'continue_cuda_runtime.py':R12_SCRIPT,'continuation-request.json':REQUEST,'continuation-baseline.json':BASELINE,'wall-origin.json':WALL,'input-pins.json':ENVIRONMENT,'diagnostic-evidence.json':DIAGNOSTICS}
 for name,sha in pins.items():
  if o.digest(o.plain(proposal/name,regular=True))!=sha:raise runtime.GpuProofUnavailable('R12 reviewed source/input bridge differs')
 req=o.read_json(proposal/'continuation-request.json');baseline=o.read_json(proposal/'continuation-baseline.json');wall=o.read_json(proposal/'wall-origin.json')
 if destination!=Path(req['destination']) or continuation!=destination.parent/'cu8-continuation-r12':raise runtime.GpuProofUnavailable('R12 exact build/continuation roots differ')
 if not (continuation/'gpu-build-continuation-receipt.json').is_file():raise runtime.GpuProofUnavailable('genuine completed R12 producer receipt unavailable')
 raw=o.plain(continuation/'gpu-build-continuation-receipt.json',regular=True)
 if o.digest(raw)!=expected_receipt_sha256:raise runtime.GpuProofUnavailable('actual successful continuation receipt pin differs')
 value=o.read_json(raw)
 require_r12_header(value)
 for name in ['recipe','recipe_lock','before','after','patch','syntax_card','syntax_response','parent_receipt','parent_failed_log','parent_source_inventory']:o.verify_file(o.plain(req[name]['path'],regular=True),req[name])
 if req['recipe']['sha256']!=R8_SCRIPT or req['recipe_lock']['sha256']!=R8_LOCK or req['after']['sha256']!=AFTER:raise runtime.GpuProofUnavailable('R12 ancestry source bridge differs')
 recipe=runtime.load(req['recipe']['path'],'exact_r8_for_r12');lock=o.read_json(req['recipe_lock']['path']);recipe.cpu_owner(lock)
 child=runtime.load(proposal/'continue_cuda_runtime.py','exact_r12_verifier_helpers');parent=o.read_json(req['parent_receipt']['path']);child.verify_parent(parent,o)
 for key in ['parent_failed_receipt','parent_failed_log','parent_source_inventory']:
  parent_key={'parent_failed_receipt':'parent_receipt','parent_failed_log':'parent_failed_log','parent_source_inventory':'parent_source_inventory'}[key]
  if value.get(key)!=req[parent_key]:raise runtime.GpuProofUnavailable('exact original failed ancestry pin differs')
 if value.get('policy')!=parent.get('policy') or value.get('toolchain')!=parent.get('toolchain') or value.get('frozen_parent_metadata')!=baseline or value.get('configure_argv')!=parent.get('configure_argv') or not value.get('original_source_preserved') or not value.get('source_applied'):raise runtime.GpuProofUnavailable('R12 changed unrelated build policy/ancestry')
 for item in o.read_json(proposal/'input-pins.json'):o.verify_file(o.plain(item['path'],regular=True),item)
 for item in baseline:o.verify_file(o.plain(item['path'],regular=True),item)
 for original,name in [('before','onnxruntime_c_api.cc.before'),('parent_receipt','parent-failed-receipt.json'),('parent_source_inventory','parent-gpu-source-files.json')]:o.verify_file(o.plain(continuation/name,regular=True),req[original])
 tools=parent['toolchain']
 for name in ['compiler','cmake','ninja','patch','python','vcvars']:o.verify_file(o.plain(tools[name]['path'],regular=True),tools[name])
 diagnostic_pin=value.get('diagnostic_evidence',{});o.verify_file(o.plain(proposal/'diagnostic-evidence.json',regular=True),diagnostic_pin)
 if diagnostic_pin.get('sha256')!=DIAGNOSTICS or value.get('syntax_qualification') is not False or value.get('environment_input_sha256')!=ENVIRONMENT:raise runtime.GpuProofUnavailable('exact R12 diagnostic/environment bridge differs')
 diagnostics=o.read_json(proposal/'diagnostic-evidence.json');facts=child.verify_diagnostics(diagnostics,o,AFTER,REQUEST)
 if value.get('syntax_diagnostics')!=facts or facts.get('syntax_qualified') is not False:raise runtime.GpuProofUnavailable('failed diagnostics relabelled or changed')
 before=o.read_json(req['parent_source_inventory']['path']);expected=child.changed_rows(before,req['before']['sha256'],AFTER,req['after']['bytes'])
 if o.source_inventory(destination/'source',exclude_mirror=True)!=expected or o.read_json(continuation/'gpu-source-files.json')!=expected or o.digest(continuation/'gpu-source-files.json')!=value.get('source_inventory_sha256'):raise runtime.GpuProofUnavailable('R12 complete source delta differs')
 frontend=o.read_json(destination/'frontend-source-files.json');cuda=o.read_json(destination/'cuda-toolkit-inputs.json')
 if o.source_inventory(destination/'frontend')!=frontend or o.digest(destination/'frontend-source-files.json')!=value.get('frontend_inventory_sha256') or o.digest(destination/'cuda-toolkit-inputs.json')!=value.get('cuda_inventory_sha256'):raise runtime.GpuProofUnavailable('R12 frontend/CUDA census differs')
 for item in cuda:o.verify_file(o.child(recipe.CUDA,item['path']),item)
 cudnn=o.read_json(recipe.INPUT/'extraction.json')
 if o.digest(recipe.INPUT/'extraction.json')!=lock['cudnn']['extraction_receipt_sha256']:raise runtime.GpuProofUnavailable('official cuDNN pin differs')
 for item in cudnn['files']:o.verify_file(o.child(recipe.INPUT/'cudnn-dev',item['path']),item)
 mirrors=[{'path':o.mirror_relative(item),'bytes':item['bytes'],'sha256':item['sha256']} for item in o.LOCK['dependencies']+lock['dependencies']];recipe.verify_mirror_closure(o,destination/'source/mirror',mirrors)
 closure=o.read_json(continuation/'frozen-input-closure.json');recipe.verify_input_closure(o,closure)
 if len(closure)!=value.get('reopened_frozen_input_count') or o.digest(continuation/'frozen-input-closure.json')!=value.get('frozen_input_closure_sha256'):raise runtime.GpuProofUnavailable('new continuation input closure differs')
 old_closure=o.read_json(destination/'frozen-input-closure.json')
 require_original_input_prefix(old_closure,closure)
 expected_inputs=old_closure+[child.row(o.plain(proposal/name,regular=True)) for name in ['continuation-request.json','continuation-baseline.json','wall-origin.json','continue_cuda_runtime.py','onnxruntime_c_api.cc.before','onnxruntime_c_api.cc','initialized-public-api.patch','syntax-card.json','capi-syntax.rsp','producer_environment.py','environment-source.json','input-pins.json','process_tree.py','wait_process_tree.ps1','CONTRACT.md','diagnostic-evidence.json']]+[{k:item[k] for k in ['path','bytes','sha256']} for item in diagnostics['syntax_runs']]+[diagnostics['tree_witness']]
 if closure!=expected_inputs:raise runtime.GpuProofUnavailable('exact continuation input roster differs')
 o.verify_file(o.plain(wall['start_file']['path'],regular=True),wall['start_file']);origin=o.read_json(wall['start_file']['path']);started=datetime.datetime.fromisoformat(origin['started_utc']);observed=datetime.datetime.fromisoformat(value['observed_utc']);cutoff=started+datetime.timedelta(seconds=10800)
 active=parent['elapsed_seconds']+facts['elapsed_seconds'];remaining=child.wall_remaining(started,observed,active)
 if origin.get('outer_timeout_seconds')!=10800 or value.get('original_absolute_cutoff_utc')!=cutoff.isoformat() or wall['absolute_cutoff_utc']!=cutoff.isoformat() or value.get('remaining_seconds_at_start')!=remaining or value.get('consumed_seconds_at_start')!=10800-remaining or value.get('wall_gap_charged') is not True or value.get('original_budget_seconds')!=10800 or value.get('diagnostic_elapsed_seconds')!=facts['elapsed_seconds'] or value.get('converted_monotonic_deadline')!=value.get('observed_monotonic_seconds',0)+remaining or value.get('elapsed_seconds',10801)>remaining:raise runtime.GpuProofUnavailable('original shared wall budget not preserved')
 newlock=copy.deepcopy(lock);newlock['final_placement']['after_sha256']=AFTER;preflight=recipe.configured_build_preflight(o,destination,parent['configure_argv'],newlock)
 if {k:v for k,v in preflight.items() if k!='final_capi_source_sha256'}!={k:v for k,v in parent['configured_preflight'].items() if k!='final_capi_source_sha256'}:raise runtime.GpuProofUnavailable('unreviewed configured build delta')
 if value.get('configured_preflight')!=preflight:raise runtime.GpuProofUnavailable('actual corrected CAPI target/configuration differs')
 commands=value.get('commands',[])
 if len(commands)!=2 or any(not o.successful(c) for c in commands):raise runtime.GpuProofUnavailable('actual settled continuation build commands missing')
 for (stage,targets),command in zip([('core-session-build-r12',['onnxruntime_session']),('build-r12',['all','re2'])],commands):
  argv=[tools['cmake']['path'],'--build',str(destination/'b/Release'),'--config','Release','--target',*targets,'--parallel','2']
  if value.get(stage+'_argv')!=argv:raise runtime.GpuProofUnavailable('recorded original stage argv differs')
  verify_tree_command(o,child,proposal,continuation,destination,stage,argv,tools,command,value,cutoff)
 libraries=[{'path':q.relative_to(destination).as_posix(),'bytes':q.stat().st_size,'sha256':o.digest(o.plain(q,regular=True))} for q in sorted((destination/'b').rglob('*.lib'))]
 if value.get('libraries')!=libraries:raise runtime.GpuProofUnavailable('actual continuation library census differs')
 pair=value.get('provider_dlls',[])
 if sorted(Path(item['path']).name for item in pair)!=['onnxruntime_providers_cuda.dll','onnxruntime_providers_shared.dll']:raise runtime.GpuProofUnavailable('same-build provider pair missing')
 for item in pair:o.verify_file(o.child(destination,item['path']),item)
 if list((destination/'b').rglob('onnxruntime.dll')):raise runtime.GpuProofUnavailable('second dynamic core refused')
 ninja=destination/'b/Release/build.ninja'
 return {'schema':'airk.cuda-static-build-proof/1','target':runtime.TARGET,'ort_version':runtime.VERSION,'producer_script_sha256':R12_SCRIPT,'producer_lock_sha256':REQUEST,'build_receipt':child.row(raw),'source_inventory_sha256':value['source_inventory_sha256'],'frontend_inventory_sha256':value['frontend_inventory_sha256'],'cuda_inventory_sha256':value['cuda_inventory_sha256'],'frozen_input_closure_sha256':value['frozen_input_closure_sha256'],'ninja':child.row(ninja),'libraries':libraries,'provider_dlls':pair,'recorded_compiler':tools['compiler'],'toolchain':tools,'qualification':False,'activation_pins_available':False,'ancestry':{'receipt_schema':'airk.private.cuda-runtime-continuation/2','r12_proposal':str(proposal),'original_failed_receipt':req['parent_receipt'],'original_r8_script':R8_SCRIPT,'original_r8_lock':R8_LOCK,'corrected_capi_sha256':AFTER,'baseline_sha256':BASELINE,'wall_origin_sha256':WALL,'diagnostic_evidence':diagnostic_pin,'syntax_qualified':False,'failed_diagnostic_receipts':diagnostics['syntax_runs'],'actual_tree_witness':diagnostics['tree_witness'],'environment_input_sha256':ENVIRONMENT}}
