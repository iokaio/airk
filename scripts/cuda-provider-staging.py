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
