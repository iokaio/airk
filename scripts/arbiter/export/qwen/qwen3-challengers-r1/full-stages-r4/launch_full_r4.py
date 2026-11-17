# SPDX-License-Identifier: Apache-2.0
"""Reviewed profile owner; preserve original job/gate/images, tighten whole deadline."""
import argparse,hashlib,json,os,time,types
from pathlib import Path
from full_worker_r4 import PROFILE_PINS
LIMITS={'Qwen/Qwen3-1.7B':{'wall_seconds':900,'host_committed_bytes':32*1024**3,'cuda_allocator_bytes':16*1024**3,'minimum_gpu_free_mib':22*1024,'owned_process_slots':2},'Qwen/Qwen3-4B':{'wall_seconds':1200,'host_committed_bytes':48*1024**3,'cuda_allocator_bytes':24*1024**3,'minimum_gpu_free_mib':30*1024,'owned_process_slots':2}}
def sha(p):
 with Path(p).open('rb') as f:return hashlib.file_digest(f,'sha256').hexdigest()
def main():
 start=time.monotonic();p=argparse.ArgumentParser();p.add_argument('--seal',type=Path,required=True);p.add_argument('--seal-sha256',required=True);p.add_argument('--log-dir',type=Path,required=True);a=p.parse_args()
 if sha(a.seal)!=a.seal_sha256:raise ValueError('Root reviewed seal changed')
 seal=json.loads(a.seal.read_bytes());stage=seal['qwen_full_stage'];model=stage['model_id'];limits=LIMITS[model];owner_path=Path(seal['file_roles']['execution_module']['path'])
 if sha(owner_path)!=PROFILE_PINS[model] or seal['limits']!=limits:raise ValueError('unreviewed resource profile')
 owner=types.ModuleType('_sealed_qwen_full_launch');owner.__file__=str(owner_path)
 exec(compile(owner_path.read_bytes(),str(owner_path),'exec'),owner.__dict__)
 os.environ.update(seal['environment']);owner.verify_seal(a.seal,a.seal_sha256)
 original=owner.run_windows_owned_training
 def remaining_owned(*args,**kwargs):
  remaining=limits['wall_seconds']-(time.monotonic()-start)
  if remaining<=5:raise ValueError('whole full-stage deadline exhausted before child')
  values=list(args);values[3]=min(values[3],remaining)
  return original(*values,**kwargs)
 owner.run_windows_owned_training=remaining_owned;owner.launch(a);owner.verify_seal(a.seal,a.seal_sha256)
 elapsed=time.monotonic()-start
 if elapsed>limits['wall_seconds']:raise ValueError('whole full-stage deadline exceeded including source checks')
 release=json.loads(Path(stage['release_path']).read_bytes())
 receipt={'schema':'airk.private-qwen-full-stage-outer-closure/1','stage':release['authorized_stage'],'model_id':model,'seal_sha256':a.seal_sha256,'root_release_sha256':stage['release_sha256'],'elapsed_seconds':elapsed,'whole_wall_seconds':limits['wall_seconds'],'owner_profile_sha256':PROFILE_PINS[model],'all_runtime_sources_pre_post_verified':True,'original_process_algorithm_natural_pass':True,'owned_receipt_sha256':sha(a.log_dir/'execution-receipt.json'),'pilot_receipt_sha256':sha(Path(release['fresh_output'])/'receipt.json')}
 (a.log_dir/'outer-closure.json').write_text(json.dumps(receipt,indent=2)+'\n',encoding='utf-8');print(json.dumps(receipt),flush=True)
if __name__=='__main__':main()
