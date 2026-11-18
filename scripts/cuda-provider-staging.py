# SPDX-License-Identifier: Apache-2.0
"""Private provider-only staging; incomplete/unapproved until native full closure."""
from pathlib import Path
import hashlib,importlib.util,json,shutil

def load(name):
 p=Path(__file__).with_name(name+'.py');s=importlib.util.spec_from_file_location(name.replace('-','_'),p);m=importlib.util.module_from_spec(s);s.loader.exec_module(m);return m

def stage_providers(destination,build,aggregate,output,continuation,proposal,receipt_sha,repo_root):
 runtime=load('cuda-static-runtime');owner=load('cuda-static-continuation-r12');core=runtime.load(Path(repo_root)/'scripts/provision-static-onnx-runtime.py','cpu_owner_private_stage')
 # Refuse all mutation until both genuine completed producer and aggregate reopen.
 observed=owner.verify_continuation(destination,continuation,proposal,receipt_sha,repo_root);actual_aggregate=load('cuda-static-aggregate').verify_aggregate(Path(destination),observed,core)
 if observed!=build or actual_aggregate!=aggregate:raise ValueError('actual producer/aggregate changed before staging')
 recipe=runtime.load(Path(proposal)/'continue_cuda_runtime.py','r12_stage_helpers');request=core.read_json(Path(proposal)/'continuation-request.json');r8=runtime.load(request['recipe']['path'],'r8_stage_policy');lock=core.read_json(request['recipe_lock']['path']);output=core.plain(output)
 if output!=r8.PARENT/'gpu-provider-stage-r12':raise ValueError('exact fresh protected provider stage required')
 guard=dict(lock);guard['build_path']={'fresh_child':output.name};policy=r8.identity_and_parent(core,output,guard);output.mkdir()
 receipt={'schema':'airk.private.gpu-provider-stage/1','status':'RUNNING','qualification':False,'complete_cuda_runtime':False,'producer_proof_sha256':runtime.canonical(build),'aggregate_proof_sha256':runtime.canonical(aggregate),'output_policy':policy,'files':[]}
 try:
  sources=[(core.child(destination,row['path']),Path(row['path']).name,row) for row in build['provider_dlls']]
  sources.append((Path(build['build_receipt']['path']),'gpu-build-continuation-receipt.json',build['build_receipt']))
  if sorted(name for _,name,_ in sources)!=['gpu-build-continuation-receipt.json','onnxruntime_providers_cuda.dll','onnxruntime_providers_shared.dll']:raise ValueError('exact pair/private receipt roster differs')
  for source,name,row in sources:
   core.verify_file(core.plain(source,regular=True),row);target=output/name
   with source.open('rb') as src,target.open('xb') as dst:shutil.copyfileobj(src,dst,1024*1024)
   core.verify_file(core.plain(target,regular=True),row);receipt['files'].append({'path':name,'bytes':target.stat().st_size,'sha256':core.digest(target)})
  if owner.verify_continuation(destination,continuation,proposal,receipt_sha,repo_root)!=build:raise ValueError('actual producer drifted during staging')
  receipt['status']='PASS_PRIVATE_PROVIDER_COPY_ONLY_INCOMPLETE_UNAPPROVED'
 except BaseException as exc:receipt.update(status='FAILED_PRESERVED_NO_RETRY',error=type(exc).__name__+': '+str(exc));raise
 finally:
  with (output/'staging-receipt.json').open('xb') as f:f.write((json.dumps(receipt,indent=2)+'\n').encode())
 return receipt


def stage_checkpointed_providers(destination,build,aggregate,output,composite_root,proposal,receipt_sha,repo_root):
 runtime=load('cuda-static-runtime');owner=load('cuda-static-checkpointed');core=runtime.load(Path(repo_root)/'scripts/provision-static-onnx-runtime.py','cpu_owner_checkpoint_stage')
 observed=owner.verify_checkpointed(destination,composite_root,proposal,receipt_sha,repo_root);actual=load('cuda-static-aggregate').verify_aggregate(Path(destination),observed,core)
 if observed!=build or actual!=aggregate:raise ValueError('actual checkpoint producer/aggregate changed before staging')
 r12,_=owner.execution_context(build,core);request=core.read_json(r12/'continuation-request.json');r8=runtime.load(request['recipe']['path'],'r8_checkpoint_stage');lock=core.read_json(request['recipe_lock']['path']);output=core.plain(output)
 if output!=r8.PARENT/'gpu-provider-stage-checkpointed-r1':raise ValueError('exact fresh protected checkpoint provider stage required')
 guard=dict(lock);guard['build_path']={'fresh_child':output.name};policy=r8.identity_and_parent(core,output,guard);output.mkdir()
 receipt={'schema':'airk.private.gpu-provider-stage/2','status':'RUNNING','build_mode':'checkpointed-warm','qualification':False,'complete_cuda_runtime':False,'producer_proof_sha256':runtime.canonical(build),'aggregate_proof_sha256':runtime.canonical(aggregate),'output_policy':policy,'files':[]}
 try:
  sources=[(core.child(destination,row['path']),Path(row['path']).name,row) for row in build['provider_dlls']]+[(Path(build['build_receipt']['path']),'gpu-checkpointed-build-receipt.json',build['build_receipt'])]
  if sorted(name for _,name,_ in sources)!=['gpu-checkpointed-build-receipt.json','onnxruntime_providers_cuda.dll','onnxruntime_providers_shared.dll']:raise ValueError('exact checkpoint pair/private receipt roster differs')
  for source,name,row in sources:
   core.verify_file(core.plain(source,regular=True),row);target=output/name
   with source.open('rb') as src,target.open('xb') as dst:shutil.copyfileobj(src,dst,1024*1024)
   core.verify_file(core.plain(target,regular=True),row);receipt['files'].append({'path':name,'bytes':target.stat().st_size,'sha256':core.digest(target)})
  if owner.verify_checkpointed(destination,composite_root,proposal,receipt_sha,repo_root)!=build:raise ValueError('actual checkpoint producer changed during staging')
  receipt['status']='PASS_PRIVATE_PROVIDER_COPY_ONLY_INCOMPLETE_UNAPPROVED'
 except BaseException as exc:receipt.update(status='FAILED_PRESERVED_NO_RETRY',error=type(exc).__name__+': '+str(exc));raise
 finally:
  with (output/'staging-receipt.json').open('xb') as f:f.write((json.dumps(receipt,indent=2)+'\n').encode())
 return receipt


def stage_incremental_providers(destination,build,aggregate,output,incremental_root,proposal,receipt_sha,repo_root):
 import time
 deadline=time.monotonic()+300
 runtime=load('cuda-static-runtime');owner=load('cuda-static-incremental');guards=load('cuda-runtime-environment');core=runtime.load(Path(repo_root)/'scripts/provision-static-onnx-runtime.py','cpu_owner_incremental_stage')
 expected = {'build': build, 'aggregate': aggregate}
 observed = owner.verify_incremental_bundle(
  destination, incremental_root, proposal, receipt_sha, repo_root, deadline=deadline)
 if observed != expected:
  raise ValueError('actual incremental producer/aggregate changed before staging')
 output=core.plain(output);policy=guards.incremental_output_policy(build,'provider_stage',output,core)
 recipe_path=core.plain(build['recipe']['path'],regular=True);core.verify_file(recipe_path,build['recipe'])
 provider_base=core.plain(core.read_json(recipe_path)['parent_context']['destination'])
 if output.exists():raise ValueError('existing incremental provider stage retained')
 def audit(path,created=False):
  remaining=deadline-time.monotonic()
  if remaining<=0:raise TimeoutError('incremental staging original consumer deadline expired')
  guards.audit_incremental_output(policy,path,core,created,min(10,remaining))
  if time.monotonic()>=deadline:raise TimeoutError('incremental staging original consumer deadline expired after guard')
 sources=[(core.child(provider_base,row['path']),Path(row['path']).name,row) for row in build['provider_dlls']]+[(Path(build['build_receipt']['path']),'gpu-incremental-build-receipt.json',build['build_receipt'])]
 if sorted(name for _,name,_ in sources)!=['gpu-incremental-build-receipt.json','onnxruntime_providers_cuda.dll','onnxruntime_providers_shared.dll']:raise ValueError('exact incremental pair/private child receipt roster differs')
 audit(output);output.mkdir();audit(output,True)
 receipt={'schema':'airk.private.gpu-provider-stage/3','status':'RUNNING','build_mode':'incremental-session-collector','qualification':False,'complete_cuda_runtime':False,'producer_proof_sha256':runtime.canonical(build),'aggregate_proof_sha256':runtime.canonical(aggregate),'output_policy':policy,'consumer_deadline_seconds':300,'files':[]}
 primary_error=None
 try:
  for source,name,row in sources:
   core.verify_file(core.plain(source,regular=True),row);target=output/name;audit(target)
   with source.open('rb') as src,target.open('xb') as dst:shutil.copyfileobj(src,dst,1024*1024)
   audit(target,True);core.verify_file(core.plain(target,regular=True),row);core.verify_file(core.plain(source,regular=True),row)
   receipt['files'].append({'path':name,'bytes':target.stat().st_size,'sha256':core.digest(target)})
  observed = owner.verify_incremental_bundle(
   destination, incremental_root, proposal, receipt_sha, repo_root, deadline=deadline)
  if observed != expected:
   raise ValueError('incremental provenance changed during provider staging')
  core.verify_file(core.plain(build['recipe']['path'],regular=True),build['recipe'])
  if time.monotonic()>=deadline:raise TimeoutError('incremental staging original consumer deadline expired')
  receipt['status']='PASS_PRIVATE_PROVIDER_COPY_ONLY_INCOMPLETE_UNAPPROVED'
 except BaseException as exc:
  primary_error=exc;receipt.update(status='FAILED_PRESERVED_NO_RETRY',error=type(exc).__name__+': '+str(exc));raise
 finally:
  try:
   target=output/'staging-receipt.json';audit(target)
   with target.open('xb') as stream:stream.write((json.dumps(receipt,indent=2)+'\n').encode())
   audit(target,True)
   if time.monotonic()>=deadline:raise TimeoutError('incremental staging original consumer deadline expired')
  except BaseException as retention_error:
   if primary_error is None:raise
   primary_error.add_note('Secondary staging receipt retention failure: '+type(retention_error).__name__+': '+str(retention_error))
 return receipt
