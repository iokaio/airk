# SPDX-License-Identifier: Apache-2.0
"""Additive full-model reference/export worker under the reviewed same process fence."""
import argparse,hashlib,json,runpy,sys,types
from pathlib import Path
HERE=Path(__file__).resolve().parent
PROFILE_PINS={'Qwen/Qwen3-1.7B':'fc670b1fa1d5fc5fb94a1428751416cee162dffb1a5ca0c6feaa30c3aa591ad0','Qwen/Qwen3-4B':'44f6707a78e833212f7b9e231f09fd65437ab17294fc2f73b67c3a39e925d451'}
def sha(p):
 with Path(p).open('rb') as f:return hashlib.file_digest(f,'sha256').hexdigest()
def main():
 p=argparse.ArgumentParser();p.add_argument('--seal',type=Path,required=True);p.add_argument('--seal-sha256',required=True);a=p.parse_args()
 if sha(a.seal)!=a.seal_sha256:raise ValueError('Root full-stage seal changed')
 seal=json.loads(a.seal.read_bytes());stage=seal['qwen_full_stage'];release=Path(stage['release_path'])
 if sha(release)!=stage['release_sha256']:raise ValueError('Root full-stage release changed')
 review=json.loads(release.read_bytes());model=stage['model_id']
 if review.get('execution_authorized') is not True or review.get('model_id')!=model or review.get('authorized_stage') not in ['qwen3-full-reference','qwen3-scores-only-export'] or review.get('limits')!=seal['limits']:raise ValueError('separate exact full-stage release required')
 owner_path=Path(seal['file_roles']['execution_module']['path'])
 if sha(owner_path)!=PROFILE_PINS.get(model):raise ValueError('unreviewed full resource profile')
 owner=types.ModuleType('_sealed_qwen_full_owner');owner.__file__=str(owner_path)
 exec(compile(owner_path.read_bytes(),str(owner_path),'exec'),owner.__dict__)
 owner.verify_seal(a.seal,a.seal_sha256,wait_for_gate=True)
 import torch
 owner.verify_torch_runtime(torch,a.seal) # exact profile fraction BEFORE any full model/forward
 if not torch.cuda.is_available():raise ValueError('no CPU fallback')
 sys.path.insert(0,str(HERE.parent))
 pilot=stage['pilot_path']
 sys.argv=[pilot,'--release',str(release),'--output',review['fresh_output'],'--assets',stage['assets_root']]
 runpy.run_path(pilot,run_name='__main__')
 owner.verify_seal(a.seal,a.seal_sha256)
 print(json.dumps({'qwen_full_stage_complete':True,'root_release_sha256':stage['release_sha256'],'execution_seal_sha256':a.seal_sha256,'all_runtime_pre_post_verified':True,'allocator_cap_before_model':True,'pilot_receipt_sha256':sha(Path(review['fresh_output'])/'receipt.json')}),flush=True)
if __name__=='__main__':main()
