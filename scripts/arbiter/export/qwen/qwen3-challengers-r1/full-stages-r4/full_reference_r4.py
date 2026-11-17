# SPDX-License-Identifier: Apache-2.0
"""Full pretrained synthetic reference with actual approved A-H vocabulary rows."""
import argparse,hashlib,importlib.metadata,json,os,sys,time
from pathlib import Path
sys.path.insert(0,str(Path(__file__).resolve().parent.parent))
import qwen3_reference_r2 as reference

def main():
 p=argparse.ArgumentParser();p.add_argument('--release',type=Path,required=True);p.add_argument('--output',type=Path,required=True);p.add_argument('--assets',type=Path,required=True);a=p.parse_args()
 review=json.loads(a.release.read_bytes());output=a.output.resolve()
 if review.get('execution_authorized') is not True or review.get('authorized_stage')!='qwen3-full-reference' or str(output)!=review['fresh_output'] or output.exists() or review['synthetic_shapes']!=[[17,3],[128,8],[2048,8]]:raise ValueError('exact full synthetic reference release required')
 if os.environ.get('CUBLAS_WORKSPACE_CONFIG')!=':4096:8':raise ValueError('deterministic cuBLAS required')
 for path,pin in review['source_sha256'].items():
  if reference.sha(path)!=pin:raise ValueError('released source differs')
 import torch
 import numpy as np
 started=time.monotonic();reference.freeze_math();scorer,config=reference.load_authenticated(a.assets,review['model_id']);scorer=scorer.to('cuda').eval();output.mkdir();rows=[]
 with torch.inference_mode():
  for i,(length,count) in enumerate(review['synthetic_shapes']):
   ids=(torch.arange(length,device='cuda')%(config['vocab_size']-1)+1)[None,:]
   values=(ids,torch.ones_like(ids),torch.arange(length,device='cuda')[None,:],torch.tensor([length-1],device='cuda'),torch.arange(32,count+32,device='cuda')[None,:],torch.ones(1,count,device='cuda',dtype=torch.bool))
   values[5][0,-1]=False
   actual,hidden=scorer(*values)
   official_hidden=scorer.text(input_ids=ids,attention_mask=values[1],position_ids=values[2],use_cache=False).last_hidden_state
   official_scores=torch.nn.functional.linear(official_hidden[:,-1],scorer.text.embed_tokens.weight)[...,values[4][0]].masked_fill(~values[5],-1e4)
   if any(not torch.isfinite(value).all() for value in [actual,hidden,official_hidden,official_scores]):raise ValueError('nonfinite full reference')
   hidden_error=float((hidden-official_hidden).abs().max());score_error=float((actual-official_scores).abs().max())
   if max(hidden_error,score_error)>1e-3:raise ValueError('full reference fixed parity tolerance exceeded')
   feed={name:value.cpu().numpy() for name,value in zip(reference.INPUT_NAMES,values)}
   np.savez_compressed(output/('case-%d.npz'%i),**feed,reference_scores=actual.cpu().numpy(),reference_hidden=hidden.cpu().numpy(),official_scores=official_scores.cpu().numpy())
   rows.append({'tokens':length,'options':count,'candidate_token_ids':values[4][0].tolist(),'option_mask':values[5][0].tolist(),'official_hidden_max_abs_error':hidden_error,'official_scores_max_abs_error':score_error})
 torch.cuda.synchronize();reference.authenticate(a.assets,review['model_id'])
 for path,pin in review['source_sha256'].items():
  if reference.sha(path)!=pin:raise ValueError('source changed during full reference')
 receipt={'stage':'qwen3-full-reference','model_id':review['model_id'],'release_sha256':reference.sha(a.release),'config':config,'cases':rows,'elapsed_seconds':time.monotonic()-started,'peak_cuda_allocated':torch.cuda.max_memory_allocated(),'peak_cuda_reserved':torch.cuda.max_memory_reserved(),'artifact_sha256':{p.name:reference.sha(p) for p in output.iterdir() if p.is_file()},'versions':{n:importlib.metadata.version(n) for n in ['torch','transformers','onnx','onnxruntime']},'complete':True,'scope':'Full pretrained synthetic architecture/readout reference only. Actual A-H rows32..39 and masked option; no task text, native compiler, quality, calibration or release qualification. Diagnostic hidden output is not production ABI.'}
 (output/'receipt.json').write_text(json.dumps(receipt,indent=2)+'\n',encoding='utf-8');print(json.dumps(receipt),flush=True)
if __name__=='__main__':main()
