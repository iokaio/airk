# SPDX-License-Identifier: Apache-2.0
"""Root-owned CPU neural trained-row readout; no backbone or ORT execution."""
import argparse,hashlib,json,os
from pathlib import Path

def sha(path):
 with Path(path).open('rb') as f:return hashlib.file_digest(f,'sha256').hexdigest()
def validate_shape(ids,candidates,hidden,rows,tokens,options):
 if ids.shape!=(1,tokens) or candidates.shape!=(1,options) or hidden.shape!=(1,tokens,rows.shape[1]) or rows.shape[0]!=8:raise ValueError('exact original all-live readout shape differs')
def main():
 p=argparse.ArgumentParser();p.add_argument('--release',type=Path,required=True);p.add_argument('--output',type=Path,required=True);a=p.parse_args();review=json.loads(a.release.read_bytes());out=a.output.resolve()
 if review.get('execution_authorized') is not True or review.get('authorized_stage')!='qwen3-context-fp32-readout' or str(out)!=review['fresh_output'] or out.exists():raise ValueError('separate Root readout release and fresh output required')
 import numpy as np
 import torch
 from safetensors import safe_open
 if torch.get_num_threads()!=1 or torch.get_num_interop_threads()!=1 or os.environ.get('OMP_NUM_THREADS')!='1' or os.environ.get('MKL_NUM_THREADS')!='1':raise ValueError('frozen single-thread CPU math differs')
 pins=review['original_head_readout']
 for entry in pins.values():
  if sha(entry['path'])!=entry['sha256']:raise ValueError('pinned official hidden/row source changed')
 reference_path=Path(pins['reference_receipt']['path']);reference=json.loads(reference_path.read_bytes());source=Path(review['source_export']);producer=json.loads((source/'receipt.json').read_bytes())
 if sha(source/'receipt.json')!=review['source_export_receipt_sha256'] or reference.get('complete') is not True or producer.get('complete') is not True or reference.get('model_id')!=review['model_id'] or producer.get('model_id')!=review['model_id']:raise ValueError('exact completed same-model reference/export required')
 with safe_open(pins['embedding_shard']['path'],framework='pt',device='cpu') as tensors:trained=tensors.get_slice('model.embed_tokens.weight')[32:40].float()
 if not torch.isfinite(trained).all():raise ValueError('nonfinite original trained rows')
 rows=[]
 for i,(tokens,options) in enumerate([(17,3),(128,8),(2048,8)]):
  name='case-%d.npz'%i;hp=reference_path.parent/name;ep=source/name
  if sha(hp)!=reference['artifact_sha256'][name] or sha(ep)!=producer['artifact_sha256'][name]:raise ValueError('actual numerical input/hidden bytes changed')
  with np.load(hp,allow_pickle=False) as h,np.load(ep,allow_pickle=False) as e:
   names=['input_ids','attention_mask','position_ids','decision_positions','candidate_positions','option_mask']
   if any(not np.array_equal(h[n],e[n]) for n in names):raise ValueError('original hidden/export six tensors differ')
   hidden=h['reference_hidden'];ids=e['input_ids'];candidates=e['candidate_positions'];live=e['option_mask'];decision=int(e['decision_positions'][0]);known=e['reference_scores']
   validate_shape(ids,candidates,hidden,trained,tokens,options)
   if hidden.dtype!=np.float32 or not np.isfinite(hidden).all() or not 0<=decision<tokens or not np.array_equal(candidates[0],np.arange(32,32+options)) or live.dtype!=np.bool_ or live.shape!=(1,options):raise ValueError('exact finite decision/trained A-H binding differs')
   with torch.inference_mode():all_live=torch.nn.functional.linear(torch.from_numpy(hidden[:,decision,:]),trained[:options]).numpy()
   error=float(np.max(np.abs(all_live[live]-known[live])))
   if not np.isfinite(all_live).all() or not np.isfinite(known).all() or error>0.001:raise ValueError('known-live actual same-artifact 1e-3 control failed')
   rows.append({'id':'synthetic-%d-by-%d'%(tokens,options),'input':{'input_ids':ids[0].tolist(),'attention_mask':e['attention_mask'][0].tolist(),'position_ids':e['position_ids'][0].tolist(),'decision_position':decision,'candidate_positions':candidates[0].tolist(),'option_mask':[True]*options},'original_fp32_all_live':all_live[0].tolist(),'python_fp32_known_live':known[0].tolist(),'known_live_mask':live[0].tolist(),'original_hidden_sha256':sha(hp),'same_artifact_reference_sha256':sha(ep),'known_live_head_reference_max_error':error})
 out.mkdir();fixture={'schema':'airk.private-context-native-fp32-probes/1','model_id':review['model_id'],'rows':rows,'original_head_source_sha256':pins['embedding_shard']['sha256'],'original_reference_receipt_sha256':pins['reference_receipt']['sha256'],'same_artifact_export_receipt_sha256':review['source_export_receipt_sha256'],'scope':'Synthetic numeric tokens only; original official full backbone hidden plus trained A-H readout, independently checked against actual FP32 export known-live references. No private task/source/gold/live paths.'}
 f=out/'numeric-fixture.json';f.write_text(json.dumps(fixture,indent=2)+'\n');receipt={'complete':True,'stage':'qwen3-context-fp32-readout','model_id':review['model_id'],'root_release_sha256':sha(a.release),'NN_scope':'CPU trained-row readout only; no backbone/ORT/CUDA execution','artifact_sha256':{f.name:sha(f)},'quality_or_resource_or_CAL_qualified':False};(out/'receipt.json').write_text(json.dumps(receipt,indent=2)+'\n');print(json.dumps(receipt))
if __name__=='__main__':main()
