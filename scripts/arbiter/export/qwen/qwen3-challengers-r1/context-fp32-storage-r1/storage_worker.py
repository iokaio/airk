# SPDX-License-Identifier: Apache-2.0
"""New quantization worker using unchanged full-stage process/resource profiles."""
import argparse,hashlib,json,runpy,sys,types
from pathlib import Path
HERE=Path(__file__).resolve().parent
sys.path.insert(0,str(HERE.parent/'full-stages-r4'))
from full_worker_r4 import PROFILE_PINS

def sha(p):
 with Path(p).open('rb') as f:return hashlib.file_digest(f,'sha256').hexdigest()
def main():
 p=argparse.ArgumentParser();p.add_argument('--seal',type=Path,required=True);p.add_argument('--seal-sha256',required=True);a=p.parse_args()
 if sha(a.seal)!=a.seal_sha256:raise ValueError('reviewed quant seal changed')
 seal=json.loads(a.seal.read_bytes());stage=seal['qwen_full_stage'];release=Path(stage['release_path'])
 if sha(release)!=stage['release_sha256']:raise ValueError('reviewed quant release changed')
 review=json.loads(release.read_bytes());model=stage['model_id']
 if review.get('execution_authorized') is not True or review.get('authorized_stage')!='qwen3-context-fp32-storage' or review.get('model_id')!=model or review.get('limits')!=seal['limits']:raise ValueError('separate exact selected quantization release required')
 owner_path=Path(seal['file_roles']['execution_module']['path'])
 if sha(owner_path)!=PROFILE_PINS[model]:raise ValueError('proven resource owner changed')
 owner=types.ModuleType('_sealed_context_weight8_owner');owner.__file__=str(owner_path)
 exec(compile(owner_path.read_bytes(),str(owner_path),'exec'),owner.__dict__);owner.verify_seal(a.seal,a.seal_sha256,wait_for_gate=True)
 sys.argv=[str(HERE/'pack_onnx.py'),'--release',str(release),'--output',review['fresh_output']]
 runpy.run_path(str(HERE/'pack_onnx.py'),run_name='__main__');owner.verify_seal(a.seal,a.seal_sha256)
 print(json.dumps({'context_fp32_storage_worker_complete':True,'root_release_sha256':stage['release_sha256'],'all_runtime_pre_post_verified':True,'original_caps_and_process_algorithm':True}),flush=True)
if __name__=='__main__':main()
