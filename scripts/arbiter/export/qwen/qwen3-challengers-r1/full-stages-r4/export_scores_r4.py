# SPDX-License-Identifier: Apache-2.0
"""Separately owned scores-only Qwen3 ONNX export; original six input names."""
import argparse,hashlib,importlib.metadata,json,os,sys,time
from pathlib import Path
sys.path.insert(0,str(Path(__file__).resolve().parent.parent))
import qwen3_reference_r2 as reference

def main():
 p=argparse.ArgumentParser(description=__doc__);p.add_argument('--release',type=Path,required=True);p.add_argument('--output',type=Path,required=True);p.add_argument('--assets',type=Path,required=True);a=p.parse_args()
 release=json.loads(a.release.read_bytes());output=a.output.resolve()
 if release.get('execution_authorized') is not True or release.get('authorized_stage')!='qwen3-scores-only-export' or str(output)!=release['fresh_output'] or output.exists():raise ValueError('fresh separately released scores-only export required')
 if os.environ.get('CUBLAS_WORKSPACE_CONFIG')!=':4096:8':raise ValueError('deterministic cuBLAS environment required')
 prior_path=Path(release['reference_receipt_path']);prior_raw=prior_path.read_bytes()
 if hashlib.sha256(prior_raw).hexdigest()!=release['reference_receipt_sha256']:raise ValueError('actual full reference receipt differs')
 prior=json.loads(prior_raw)
 if prior.get('stage')!='qwen3-full-reference' or prior.get('model_id')!=release['model_id'] or [(r['tokens'],r['options']) for r in prior['cases']]!=[(17,3),(128,8),(2048,8)]:raise ValueError('matching full selected-model 3-shape reference required')
 for name,pin in prior['artifact_sha256'].items():
  if Path(name).name!=name or reference.sha(prior_path.parent/name)!=pin:raise ValueError('actual reference arrays changed')
 import numpy as np
 import torch
 import onnxruntime as ort
 started=time.monotonic();reference.freeze_math();output.mkdir()
 receipt={'schema':'airk.private-qwen-scores-only-export/1','stage':'qwen3-scores-only-export','model_id':release['model_id'],'root_release_sha256':reference.sha(a.release),'reference_receipt_sha256':release['reference_receipt_sha256'],'production_abi_shape_only':True,'release_qualified':False,'rows':[],'complete':False}
 try:
  scorer,config=reference.load_authenticated(a.assets,release['model_id']);scorer=scorer.to('cuda').eval()
  class ScoresOnly(torch.nn.Module):
   def __init__(self):super().__init__();self.scorer=scorer
   def forward(self,input_ids,attention_mask,position_ids,decision_positions,candidate_positions,option_mask):
    return self.scorer(input_ids,attention_mask,position_ids,decision_positions,candidate_positions,option_mask)[0]
  model=ScoresOnly().eval();names=list(reference.INPUT_NAMES);cases=[]
  with torch.inference_mode():
   for i in range(3):
    source=np.load(prior_path.parent/('case-%d.npz'%i),allow_pickle=False)
    feed={n:source[n] for n in names};expected=source['reference_scores'];values=tuple(torch.from_numpy(feed[n]).to('cuda') for n in names)
    actual=model(*values).cpu().numpy()
    if not np.isfinite(actual).all() or not np.isfinite(expected).all() or np.max(np.abs(actual-expected))>1e-3:raise ValueError('fresh original scores differ from retained full reference')
    cases.append((feed,expected))
   values=tuple(torch.from_numpy(cases[0][0][n]).to('cuda') for n in names)
   torch.onnx.export(model,values,str(output/'model.onnx'),input_names=names,output_names=['scores'],opset_version=18,dynamo=False,do_constant_folding=True,external_data=True,dynamic_axes={'input_ids':{1:'tokens'},'attention_mask':{1:'tokens'},'position_ids':{1:'tokens'},'candidate_positions':{1:'options'},'option_mask':{1:'options'},'scores':{1:'options'}})
  del values,model,scorer;torch.cuda.empty_cache()
  options=ort.SessionOptions();options.intra_op_num_threads=1;options.inter_op_num_threads=1;options.enable_cpu_mem_arena=False;options.execution_mode=ort.ExecutionMode.ORT_SEQUENTIAL
  options.add_session_config_entry('session.intra_op.allow_spinning','0');options.add_session_config_entry('session.inter_op.allow_spinning','0')
  session=ort.InferenceSession(str(output/'model.onnx'),options,providers=['CPUExecutionProvider'])
  if [x.name for x in session.get_inputs()]!=names or [x.name for x in session.get_outputs()]!=['scores']:raise ValueError('scores-only native six-input graph ABI differs')
  for i,(feed,expected) in enumerate(cases):
   actual=session.run(['scores'],feed)[0];error=float(np.max(np.abs(actual-expected)))
   if actual.shape!=expected.shape or not np.isfinite(actual).all() or error>1e-3:raise ValueError('scores-only ORT parity exceeds fixed tolerance')
   np.savez_compressed(output/('case-%d.npz'%i),**feed,reference_scores=expected,ort_scores=actual)
   receipt['rows'].append({'tokens':int(feed['input_ids'].shape[1]),'options':int(feed['candidate_positions'].shape[1]),'maximum_absolute_error':error,'argmax_matches':bool(np.argmax(actual)==np.argmax(expected))})
  reference.authenticate(a.assets,release['model_id']);receipt['complete']=True
 except BaseException as error:
  receipt.update(error_type=type(error).__name__,error=str(error));raise
 finally:
  receipt.update(elapsed_seconds=time.monotonic()-started,peak_cuda_allocated=torch.cuda.max_memory_allocated(),peak_cuda_reserved=torch.cuda.max_memory_reserved(),versions={n:importlib.metadata.version(n) for n in ['torch','transformers','onnx','onnxruntime']},artifact_sha256={x.name:reference.sha(x) for x in output.iterdir() if x.is_file()})
  (output/'receipt.json').write_text(json.dumps(receipt,indent=2)+'\n',encoding='utf-8');print(json.dumps(receipt),flush=True)
if __name__=='__main__':main()
