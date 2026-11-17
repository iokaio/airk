# SPDX-License-Identifier: Apache-2.0
"""Pre-Torch gated tiny worker; original reviewed owner stays unchanged."""
import argparse,hashlib,json,runpy,sys,types
from pathlib import Path
OWNER_SHA='d363913b2424321822f9eee189f999eabb807a43c7d0c46fab567531dd1bd975'
def digest(p):
 with Path(p).open('rb') as f:return hashlib.file_digest(f,'sha256').hexdigest()
def main():
 p=argparse.ArgumentParser();p.add_argument('--seal',type=Path,required=True);p.add_argument('--seal-sha256',required=True);a=p.parse_args()
 if digest(a.seal)!=a.seal_sha256:raise ValueError('tiny seal changed')
 seal=json.loads(a.seal.read_bytes());stage=seal['qwen_tiny_stage'];release=Path(stage['release_path'])
 if digest(release)!=stage['release_sha256']:raise ValueError('Root tiny release changed')
 review=json.loads(release.read_bytes())
 if review.get('execution_authorized') is not True or review['authorized_stage']!='qwen3-tiny-export' or review['model_id'] is not None:raise ValueError('tiny-only release required')
 owner_path=Path(seal['file_roles']['execution_module']['path'])
 if digest(owner_path)!=OWNER_SHA:raise ValueError('original owner changed')
 owner=types.ModuleType('_sealed_qwen_owner');owner.__file__=str(owner_path)
 exec(compile(owner_path.read_bytes(),str(owner_path),'exec'),owner.__dict__)
 owner.verify_seal(a.seal,a.seal_sha256,wait_for_gate=True)
 import torch
 owner.verify_torch_runtime(torch,a.seal) # sets allocator fraction BEFORE model/weight/forward
 if not torch.cuda.is_available():raise ValueError('no silent CPU fallback')
 if seal['limits']['cuda_allocator_bytes']!=4*1024**3:raise ValueError('original allocator cap required')
 sys.argv=[stage['pilot_path'],'--release',str(release),'--output',review['fresh_output'],'--assets',stage['assets_root']]
 runpy.run_path(stage['pilot_path'],run_name='__main__')
 owner.verify_seal(a.seal,a.seal_sha256)
 print(json.dumps({'qwen_tiny_worker_complete':True,'root_release_sha256':stage['release_sha256'],'execution_seal_sha256':a.seal_sha256,'full_runtime_pre_post_verified':True,'allocator_cap_before_model':True,'production_abi':False}),flush=True)
if __name__=='__main__':main()
