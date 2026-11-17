# SPDX-License-Identifier: Apache-2.0
"""Original exact process owner with a stricter shared outer deadline."""
import argparse,hashlib,json,os,time,types
from pathlib import Path
OWNER_SHA='d363913b2424321822f9eee189f999eabb807a43c7d0c46fab567531dd1bd975'
def digest(p):
 with Path(p).open('rb') as f:return hashlib.file_digest(f,'sha256').hexdigest()
def main():
 start=time.monotonic();p=argparse.ArgumentParser();p.add_argument('--seal',type=Path,required=True);p.add_argument('--seal-sha256',required=True);p.add_argument('--log-dir',type=Path,required=True);a=p.parse_args()
 if digest(a.seal)!=a.seal_sha256:raise ValueError('Root reviewed seal changed')
 seal=json.loads(a.seal.read_bytes());owner_path=Path(seal['file_roles']['execution_module']['path'])
 if digest(owner_path)!=OWNER_SHA:raise ValueError('original owner changed')
 if seal['limits']!={'wall_seconds':600,'host_committed_bytes':8*1024**3,'cuda_allocator_bytes':4*1024**3,'minimum_gpu_free_mib':8192,'owned_process_slots':2}:raise ValueError('tiny limits changed')
 owner=types.ModuleType('_sealed_qwen_launch_owner');owner.__file__=str(owner_path)
 exec(compile(owner_path.read_bytes(),str(owner_path),'exec'),owner.__dict__)
 os.environ.update(seal['environment'])
 owner.verify_seal(a.seal,a.seal_sha256)
 original=owner.run_windows_owned_training
 def remaining_owned(*args,**kwargs):
  remaining=600-(time.monotonic()-start)
  if remaining<=5:raise ValueError('whole tiny deadline exhausted before child')
  values=list(args);values[3]=min(values[3],remaining)
  return original(*values,**kwargs)
 owner.run_windows_owned_training=remaining_owned
 owner.launch(a)
 owner.verify_seal(a.seal,a.seal_sha256)
 elapsed=time.monotonic()-start
 if elapsed>600:raise ValueError('whole tiny deadline exceeded including source checks')
 receipt={'schema':'airk.private-qwen-tiny-outer-closure/1','seal_sha256':a.seal_sha256,'root_release_sha256':seal['qwen_tiny_stage']['release_sha256'],'elapsed_seconds':elapsed,'whole_wall_seconds':600,'original_owner_sha256':OWNER_SHA,'all_runtime_sources_verified_before_after':True,'original_process_owner_natural_pass':True,'production_abi':False,'owned_receipt_sha256':digest(a.log_dir/'execution-receipt.json'),'pilot_receipt_sha256':digest(Path(json.loads(Path(seal['qwen_tiny_stage']['release_path']).read_bytes())['fresh_output'])/'receipt.json')}
 (a.log_dir/'outer-closure.json').write_text(json.dumps(receipt,indent=2)+'\n',encoding='utf-8');print(json.dumps(receipt),flush=True)
if __name__=='__main__':main()
