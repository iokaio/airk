# SPDX-License-Identifier: Apache-2.0
"""Retain actual numeric-only quant probes; no Profile construction or inference."""
import argparse,hashlib,json
from pathlib import Path

def sha(p):
 with Path(p).open('rb') as f:return hashlib.file_digest(f,'sha256').hexdigest()
def main():
 p=argparse.ArgumentParser();p.add_argument('--quant-result',type=Path,required=True);p.add_argument('--receipt-sha256',required=True);p.add_argument('--output',type=Path,required=True);a=p.parse_args();root=a.quant_result.resolve()
 if a.output.exists() or sha(root/'receipt.json')!=a.receipt_sha256:raise ValueError('actual completed producer and fresh numeric fixture required')
 receipt=json.loads((root/'receipt.json').read_bytes())
 if receipt.get('complete') is not True or receipt.get('stage')!='qwen3-context-weight8':raise ValueError('actual complete selected weight8 producer required')
 import numpy as np
 rows=[]
 for i,(tokens,options) in enumerate([(17,3),(128,8),(2048,8)]):
  path=root/('case-%d.npz'%i)
  if sha(path)!=receipt['artifact_sha256'][path.name]:raise ValueError('actual numeric array identity changed')
  values=np.load(path,allow_pickle=False)
  ids=values['input_ids'];mask=values['attention_mask'];positions=values['position_ids'];decision=values['decision_positions'];candidates=values['candidate_positions'];live=np.ones((1,options),dtype=np.bool_)
  original=values['original_trained_rows_all_live_scores'];quantized=values['weight8_all_live_scores']
  if ids.shape!=(1,tokens) or candidates.shape!=(1,options) or original.shape!=(1,options) or quantized.shape!=original.shape or not np.isfinite(original).all() or not np.isfinite(quantized).all():raise ValueError('all-live numeric fixture shape/finite contract differs')
  rows.append({'id':'synthetic-%d-by-%d'%(tokens,options),'input':{'input_ids':ids[0].tolist(),'attention_mask':mask[0].tolist(),'position_ids':positions[0].tolist(),'decision_position':int(decision[0]),'candidate_positions':candidates[0].tolist(),'option_mask':live[0].tolist()},'original_fp32_all_live':original[0].tolist(),'python_weight8_all_live':quantized[0].tolist(),'raw_array_sha256':sha(path)})
 result={'schema':'airk.private-context-native-numeric-probes/1','model_id':receipt['model_id'],'producer_receipt_sha256':a.receipt_sha256,'storage_proof_sha256':receipt['native_storage_proof_sha256'],'rows':rows,'scope':'Synthetic numeric tokens/readouts only, all-liveH; no prompts, source documents, CAL/confirmation/gold/livepaths, task labels or semantic qualification.'}
 a.output.write_text(json.dumps(result,indent=2)+'\n',encoding='utf-8');print(json.dumps({'fixture':str(a.output),'sha256':sha(a.output),'rows':3,'NN_executed':False}))
if __name__=='__main__':main()
