# SPDX-License-Identifier: Apache-2.0
"""Read-only source/configuration checks; historical runnables are data only."""
import copy,hashlib,json,os,subprocess
from pathlib import Path
SOURCE_REL="onnxruntime/core/session/onnxruntime_c_api.cc"
CUDA=Path("C:/Program Files/NVIDIA GPU Computing Toolkit/CUDA/v12.8")
INPUT=Path("C:/Users/tdotn/AppData/Local/airk-final-exec-20261116T052942Z/gpu-runtime-inputs-r1")
POWERSHELL=Path("C:/Program Files/PowerShell/7/pwsh.exe")

def digest(p):
 h=hashlib.sha256()
 with Path(p).open('rb') as f:
  for b in iter(lambda:f.read(4*1024*1024),b''):h.update(b)
 return h.hexdigest()
def changed_rows(before,old,new,size):
 result=copy.deepcopy(before);found=0
 for item in result:
  if item['path']==SOURCE_REL:
   if item['sha256']!=old:raise ValueError('parent CAPI source identity differs')
   item.update(bytes=size,sha256=new);found+=1
 if found!=1:raise ValueError('exact single CAPI target missing')
 return result
def verify_input_closure(o,rows):
 seen=set()
 for row in rows:
  path=o.plain(row['path'],regular=True);key=os.path.normcase(str(path))
  if key in seen:raise ValueError('duplicate frozen input path')
  seen.add(key);o.verify_file(path,row)
 return len(seen)

def verify_mirror_closure(o,root,expected):
 if o.source_inventory(root)!=sorted(expected,key=lambda r:r['path']):raise ValueError('exact copied mirror closure drift')

def configured_build_preflight(o,destination,command,lock):
 build=destination/'b/Release';cache=o.verify_cmake_cache(build/'CMakeCache.txt',command)
 ninja=o.plain(build/'build.ninja',regular=True);rules=o.plain(build/'CMakeFiles/rules.ninja',regular=True)
 if ninja.stat().st_size>64*1024**2 or rules.stat().st_size>16*1024**2:raise ValueError('configured Ninja graph exceeds bound')
 text=ninja.read_text(encoding='utf8');rule_text=rules.read_text(encoding='utf8')
 if 'allow-unsupported-compiler' in (text+rule_text+str(cache)).lower():raise ValueError('unsupported CUDA compiler override refused')
 lines=text.splitlines();targets={}
 for name in ('onnxruntime_session.lib','all','re2','onnxruntime_providers_cuda.dll','onnxruntime_providers_shared.dll'):
  found=[line for line in lines if line.startswith('build '+name+':') or line.startswith('build '+name+' ')]
  if len(found)!=1:raise ValueError('configured native target missing/ambiguous: '+name)
  targets[name]=found[0]
 if ': CXX_STATIC_LIBRARY_LINKER' not in targets['onnxruntime_session.lib']:raise ValueError('session target is not a static archive')
 capi=[line for line in lines if line.startswith('build ') and 'onnxruntime_c_api.cc.obj: CXX_COMPILER__onnxruntime_session_' in line]
 if len(capi)!=1:raise ValueError('final CAPI compilation target missing/ambiguous')
 output,rest=capi[0][6:].split(': ',1);source=rest.split()[1].replace('$:',':').replace(chr(92),'/')
 expected=(destination/'source'/lock['final_placement']['path']).as_posix()
 if source!=expected or output not in targets['onnxruntime_session.lib'].split():raise ValueError('final CAPI source/object is not in actual session archive')
 if o.digest(destination/'source'/lock['final_placement']['path'])!=lock['final_placement']['after_sha256']:raise ValueError('final CAPI source bytes differ before compile')
 maximum=0;objects=0
 for line in lines:
  if line.startswith('build ') and ('.obj: CXX_COMPILER' in line or '.obj: CUDA_COMPILER' in line):
   relative=line[6:].split(': ',1)[0];path=o.plain(build/relative)
   if not path.is_relative_to(build):raise ValueError('configured object escapes build root')
   maximum=max(maximum,len(str(path)));objects+=1
 if not objects or maximum>240:raise ValueError('configured object path exceeds reviewed Windows bound')
 return {'cache_sha256':o.digest(build/'CMakeCache.txt'),'ninja_sha256':o.digest(ninja),'rules_sha256':o.digest(rules),'checked_flags':sum(x.startswith('-D') for x in command),'objects':objects,'max_object_path':maximum,'final_capi_source_sha256':lock['final_placement']['after_sha256']}
def command_request(argv,cwd,out,stage,events=None):
 if not argv or not Path(argv[0]).is_absolute():raise ValueError('absolute original command executable required')
 request={'schema':'airk.private.process-tree-command/1','executable':str(argv[0]),'arguments':subprocess.list2cmdline([str(x) for x in argv[1:]]),'cwd':str(cwd),'stdout':str(out/'logs'/(stage+'-tree.stdout.log')),'stderr':str(out/'logs'/(stage+'-tree.stderr.log')),'result':str(out/(stage+'-tree-result.json')),'started':str(out/(stage+'-tree-started.json')),'started_event':None,'done_event':None}
 if events:request.update(started_event=events['started'],done_event=events['done'])
 return request
def verify_materialized(inputs,plan,o,repo_root):
 r12_path=Path(inputs['r12_proposal']);req=json.loads((r12_path/'continuation-request.json').read_bytes());lock=json.loads(Path(req['recipe_lock']['path']).read_bytes())
 if digest(Path(repo_root)/'scripts/provision-static-onnx-runtime.py')!=lock['cpu_owner_sha256'] or digest(Path(repo_root)/'scripts/static-onnx-runtime-lock.json')!=lock['cpu_lock_sha256']:raise ValueError('canonical CPU owner drift')
 o.validate_lock();dest=o.plain(plan['actual_build_root']);prior=o.read_json(plan['previous_failed_R12']['path']);tools=prior['toolchain']
 for row in inputs['pins']:o.verify_file(o.plain(row['path'],regular=True),row)
 for row in o.read_json(inputs['failed_ancestry']['path']):o.verify_file(o.plain(row['path'],regular=True),row)
 for row in tools.values():
  if isinstance(row,dict) and {'path','bytes','sha256'}.issubset(row):o.verify_file(o.plain(row['path'],regular=True),row)
 original=o.read_json(dest/'frozen-input-closure.json');verify_input_closure(o,original);mirrors=[{'path':o.mirror_relative(r),'bytes':r['bytes'],'sha256':r['sha256']} for r in o.LOCK['dependencies']+lock['dependencies']];verify_mirror_closure(o,dest/'source/mirror',mirrors)
 source_rows=changed_rows(o.read_json(req['parent_source_inventory']['path']),req['before']['sha256'],req['after']['sha256'],req['after']['bytes'])
 if o.source_inventory(dest/'source',exclude_mirror=True)!=source_rows or o.source_inventory(dest/'frontend')!=o.read_json(dest/'frontend-source-files.json'):raise ValueError('actual compiled source changed')
 for row in o.read_json(dest/'cuda-toolkit-inputs.json'):o.verify_file(o.child(CUDA,row['path']),row)
 for row in o.read_json(INPUT/'extraction.json')['files']:o.verify_file(o.child(INPUT/'cudnn-dev',row['path']),row)
 newlock=copy.deepcopy(lock);newlock['final_placement']['after_sha256']=req['after']['sha256'];preflight=configured_build_preflight(o,dest,prior['configure_argv'],newlock)
 if preflight!=plan['configured_preflight']:raise ValueError('actual source/target/cache/configuration changed')
 return {'schema':'airk.private.cuda-materialized-source/1','actual_build_root':str(dest),'corrected_capi_sha256':req['after']['sha256'],'source_inventory_sha256':digest(Path(plan['current_source_inventory']['path'])),'frontend_inventory_sha256':digest(dest/'frontend-source-files.json'),'cuda_inventory_sha256':digest(dest/'cuda-toolkit-inputs.json'),'frozen_original_inputs_sha256':digest(dest/'frozen-input-closure.json'),'tools':tools,'configured_preflight':preflight,'configure_argv':prior['configure_argv'],'policy':prior['policy'],'failed_r12_sha256':plan['previous_failed_R12']['sha256']}
