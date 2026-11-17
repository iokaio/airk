# SPDX-License-Identifier: Apache-2.0
"""Separate GPU static-build verifier; never delegates GPU proof to CPU verify_build.
This owner verifies build provenance only, not model/device/task qualification.
"""
import hashlib,importlib.util,json,os
from pathlib import Path
ROOT=Path(__file__).resolve().parents[1]
TARGET='x86_64-pc-windows-msvc'
VERSION='1.30.0'
SCRIPT_SHA='c2f9c3e305106469d6a58699eaa36ec2afed7a6cd4be63275498fd78af2d3f4a'
LOCK_SHA='549be258fd21be030f994f6da7f74146a9e9edd3bb611bee83305d9a4deade6d'
FIELDS=('build_receipt_sha256','target','static_link_closure_sha256','static_link_supplement_sha256','static_link_recovery_sha256','verified_inputs_sha256')
class GpuProofUnavailable(ValueError):pass

def load(path,name):
 spec=importlib.util.spec_from_file_location(name,path);module=importlib.util.module_from_spec(spec);spec.loader.exec_module(module);return module

def canonical(value,limit=4*1024**2):
 data=json.dumps(value,sort_keys=True,separators=(',',':'),ensure_ascii=False,allow_nan=False).encode()
 if len(data)>limit:raise ValueError('GPU provenance exceeds bound')
 return hashlib.sha256(data).hexdigest()

def _json(o,path,limit=16*1024**2):return o.read_json(path,limit)
def _pin(o,path):
 path=o.plain(path,regular=True);return {'path':str(path),'bytes':path.stat().st_size,'sha256':o.digest(path)}
def _same(o,path,row):o.verify_file(o.plain(path,regular=True),row)

def verify_build(destination,producer_file,lock_file,expected_receipt_sha256,repo_root=ROOT):
 o=load(Path(repo_root)/'scripts/provision-static-onnx-runtime.py','airk_cpu_build_owner_for_gpu')
 destination=o.plain(destination);producer_file=o.plain(producer_file,regular=True);lock_file=o.plain(lock_file,regular=True)
 if o.digest(producer_file)!=SCRIPT_SHA or o.digest(lock_file)!=LOCK_SHA:raise GpuProofUnavailable('GPU producer source bridge is not the reviewed R8')
 lock=_json(o,lock_file)
 if o.digest(Path(repo_root)/'scripts/provision-static-onnx-runtime.py')!=lock['cpu_owner_sha256'] or o.digest(Path(repo_root)/'scripts/static-onnx-runtime-lock.json')!=lock['cpu_lock_sha256']:raise GpuProofUnavailable('CPU acquisition/tool owner changed; explicit source bridge required')
 if not (destination/'gpu-build-receipt.json').is_file():raise GpuProofUnavailable('actual completed GPU build receipt unavailable')
 receipt_path=o.plain(destination/'gpu-build-receipt.json',regular=True)
 if not isinstance(expected_receipt_sha256,str) or len(expected_receipt_sha256)!=64 or o.digest(receipt_path)!=expected_receipt_sha256:raise GpuProofUnavailable('actual completed GPU receipt pin missing/different')
 receipt=_json(o,receipt_path);owner=_json(o,destination/'owner.json')
 if owner.get('schema')!='airk.private.cuda-runtime-producer/1' or owner.get('script_sha256')!=SCRIPT_SHA or owner.get('lock_sha256')!=LOCK_SHA or owner.get('qualification') is not False:raise GpuProofUnavailable('GPU producer owner/source identity differs')
 required_policy={'one_static_core':True,'optional_shared_cuda_provider':True,'runtime_version':VERSION,'SM':86,'compiled_flash_attention':True,'compiled_memory_efficient_attention':True,'first_model_gate':'FP32-unfused-TF32off','fp16_model_admission':'separate-later-G5-unqualified','cpu_qualification_transferred':False}
 if receipt.get('status')!='PASS_PRODUCER_ONLY_UNQUALIFIED' or receipt.get('policy')!=required_policy:raise GpuProofUnavailable('GPU producer has no actual complete unqualified build proof')
 commands=receipt.get('commands')
 if not isinstance(commands,list) or len(commands)!=9 or any(not o.successful(c) for c in commands):raise GpuProofUnavailable('GPU build command/settlement proof incomplete')
 stages=['loader-patch','offline-patch','final-placement','frontend-arm64','frontend-auth-imports','frontend-header-probe','configure','core-session-build','build']
 for stage,command in zip(stages,commands):
  log=o.plain(destination/'logs'/(stage+'.log'),regular=True)
  if os.path.normcase(command.get('log',''))!=os.path.normcase(str(log)):raise GpuProofUnavailable('GPU stage log attribution differs')
 tools=receipt['toolchain']
 for name in ('compiler','cmake','ninja','patch','python','vcvars'):_same(o,tools[name]['path'],tools[name])
 closure=_json(o,destination/'frozen-input-closure.json')
 if o.digest(destination/'frozen-input-closure.json')!=receipt.get('frozen_input_closure_sha256') or not isinstance(closure,list) or len(closure)!=44:raise GpuProofUnavailable('GPU frozen input closure differs')
 seen=set()
 for row in closure:
  key=os.path.normcase(row['path'])
  if key in seen:raise GpuProofUnavailable('duplicate frozen input')
  seen.add(key);_same(o,row['path'],row)
 if receipt.get('reopened_frozen_input_count')!=len(closure):raise GpuProofUnavailable('GPU final input recensus missing')
 # Reconstruct exact transformed source census, not a self-reported group hash.
 original=_json(o,destination/'source-files.json')
 if o.digest(destination/'source-files.json')!=o.LOCK['source']['inventory_sha256']:raise GpuProofUnavailable('original source census drift')
 changes={r['path']:(r['before_sha256'],r['proposed_sha256']) for r in lock['loader_files']}
 for key in ('offline_helper','final_placement'):
  r=lock[key];changes[r['path']]=(r['before_sha256'],r['after_sha256'])
 expected=[]
 for row in original:
  r=dict(row)
  if r['path'] in changes:
   before,after=changes[r['path']]
   if r['sha256']!=before:raise GpuProofUnavailable('patch target original source differs')
   actual=o.plain(destination/'source'/r['path'],regular=True)
   if o.digest(actual)!=after:raise GpuProofUnavailable('reviewed GPU source bytes differ')
   r.update(bytes=actual.stat().st_size,sha256=after)
  expected.append(r)
 if len(changes)!=5 or o.source_inventory(destination/'source',exclude_mirror=True)!=expected or _json(o,destination/'gpu-source-files.json')!=expected or o.digest(destination/'gpu-source-files.json')!=receipt.get('source_inventory_sha256'):raise GpuProofUnavailable('complete GPU source census differs')
 frontend=_json(o,destination/'frontend-source-files.json')
 if o.source_inventory(destination/'frontend')!=frontend or o.digest(destination/'frontend-source-files.json')!=receipt.get('frontend_inventory_sha256'):raise GpuProofUnavailable('frontend compiled source drift')
 before_frontend=_json(o,destination/'frontend-source-before.json')
 expected_front=[]
 for row in before_frontend['files_before_patches']:
  r=dict(row)
  for key in ('frontend_upstream_arm64','frontend_final'):
   changed=lock[key]
   if r['path']==changed['path']:
    if r['sha256']!=changed['before_sha256']:raise GpuProofUnavailable('frontend original target differs')
    f=o.plain(destination/'frontend'/r['path'],regular=True)
    if o.digest(f)!=changed['after_sha256']:raise GpuProofUnavailable('frontend after bytes differ')
    r.update(bytes=f.stat().st_size,sha256=changed['after_sha256'])
  expected_front.append(r)
 if frontend!=expected_front or before_frontend['excluded_inert_link']!=['.claude/skills']:raise GpuProofUnavailable('frontend transformation scope differs')
 cuda=_json(o,destination/'cuda-toolkit-inputs.json')
 if o.digest(destination/'cuda-toolkit-inputs.json')!=receipt.get('cuda_inventory_sha256'):raise GpuProofUnavailable('toolkit input census drift')
 recipe=load(producer_file,'airk_reviewed_gpu_recipe')
 for row in cuda:_same(o,o.child(recipe.CUDA,row['path']),row)
 cudnn=_json(o,recipe.INPUT/'extraction.json')
 if o.digest(recipe.INPUT/'extraction.json')!=lock['cudnn']['extraction_receipt_sha256']:raise GpuProofUnavailable('official cuDNN extraction pin differs')
 for row in cudnn['files']:_same(o,o.child(recipe.INPUT/'cudnn-dev',row['path']),row)
 # Exact selected mirror roster is separate from all original input pins.
 expected_mirrors=[]
 for row in o.LOCK['dependencies']+lock['dependencies']:
  rel=o.mirror_relative(row);expected_mirrors.append({'path':rel,'bytes':row['bytes'],'sha256':row['sha256']})
 if o.source_inventory(destination/'source/mirror')!=sorted(expected_mirrors,key=lambda r:r['path']):raise GpuProofUnavailable('copied mirror closure drift')
 expected_configure=recipe.gpu_command(o,destination,tools,destination/'frontend')
 if receipt.get('configure_argv')!=expected_configure:raise GpuProofUnavailable('GPU configure policy differs')
 actual_preflight=recipe.configured_build_preflight(o,destination,expected_configure,lock)
 if receipt.get('configured_preflight')!=actual_preflight:raise GpuProofUnavailable('actual configured target/source preflight differs')
 for key,targets in [('core_build_argv',['onnxruntime_session']),('build_argv',['all','re2'])]:
  actual=[tools['cmake']['path'],'--build',str(destination/'b/Release'),'--config','Release','--target',*targets,'--parallel','2']
  if receipt.get(key)!=actual:raise GpuProofUnavailable('GPU native target/parallel policy differs')
 actual_libraries=[{'path':p.relative_to(destination).as_posix(),'bytes':p.stat().st_size,'sha256':o.digest(o.plain(p,regular=True))} for p in sorted((destination/'b').rglob('*.lib'))]
 if receipt.get('libraries')!=actual_libraries:raise GpuProofUnavailable('actual GPU library output census differs')
 pair=receipt.get('provider_dlls')
 if not isinstance(pair,list) or sorted(Path(r['path']).name for r in pair)!=['onnxruntime_providers_cuda.dll','onnxruntime_providers_shared.dll']:raise GpuProofUnavailable('actual same-build provider pair missing')
 for row in pair:_same(o,o.child(destination,row['path']),row)
 if list((destination/'b').rglob('onnxruntime.dll')):raise GpuProofUnavailable('second dynamic ORT core refused')
 ninja=_pin(o,destination/'b/Release/build.ninja')
 return {'schema':'airk.cuda-static-build-proof/1','target':TARGET,'ort_version':VERSION,'producer_script_sha256':SCRIPT_SHA,'producer_lock_sha256':LOCK_SHA,'build_receipt':_pin(o,receipt_path),'source_inventory_sha256':receipt['source_inventory_sha256'],'frontend_inventory_sha256':receipt['frontend_inventory_sha256'],'cuda_inventory_sha256':receipt['cuda_inventory_sha256'],'frozen_input_closure_sha256':receipt['frozen_input_closure_sha256'],'ninja':ninja,'libraries':actual_libraries,'provider_dlls':pair,'recorded_compiler':tools['compiler'],'toolchain':tools,'qualification':False,'activation_pins_available':False}

def static_projection(build,aggregate):
 if build.get('schema') not in ('airk.cuda-static-build-proof/1','airk.cuda-static-build-proof/2') or aggregate.get('schema')!='airk.cuda-static-aggregate-proof/1' or aggregate.get('producer_proof_sha256')!=canonical(build):raise GpuProofUnavailable('matching genuine GPU build/aggregate proof required')
 return {'build_receipt_sha256':build['build_receipt']['sha256'],'target':TARGET,'static_link_closure_sha256':canonical(aggregate),'static_link_supplement_sha256':None,'static_link_recovery_sha256':None,'verified_inputs_sha256':canonical({'gpu_build':build,'gpu_aggregate':aggregate})}
