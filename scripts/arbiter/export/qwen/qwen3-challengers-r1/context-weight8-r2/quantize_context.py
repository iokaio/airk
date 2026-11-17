# SPDX-License-Identifier: Apache-2.0
"""Selected Context weight8 artifact; distinct approximate numeric gate, no training."""
import argparse,hashlib,json,os,time,gc
from pathlib import Path

def sha(p):
 with Path(p).open('rb') as f:return hashlib.file_digest(f,'sha256').hexdigest()
def main():
 p=argparse.ArgumentParser();p.add_argument('--release',type=Path,required=True);p.add_argument('--output',type=Path,required=True);a=p.parse_args()
 release=json.loads(a.release.read_bytes());out=a.output.resolve()
 if release.get('execution_authorized') is not True or release.get('authorized_stage')!='qwen3-context-weight8' or out.exists() or str(out)!=release['fresh_output']:raise ValueError('fresh separately released Context weight8 stage required')
 if os.environ.get('CUBLAS_WORKSPACE_CONFIG')!=':4096:8':raise ValueError('frozen environment required')
 source=Path(release['source_export']);prior_path=source/'receipt.json'
 if sha(prior_path)!=release['source_export_receipt_sha256']:raise ValueError('actual FP32 producer receipt changed')
 prior=json.loads(prior_path.read_bytes())
 if prior.get('complete') is not True or prior.get('stage')!='qwen3-scores-only-export' or prior.get('model_id')!=release['model_id']:raise ValueError('matching complete selected FP32 graph required')
 for name,pin in prior['artifact_sha256'].items():
  if Path(name).name!=name or sha(source/name)!=pin:raise ValueError('actual FP32 tensor/fixture source changed')
 head=release['original_head_readout']
 for item in [head['reference_receipt'],head['embedding_index'],head['embedding_shard']]:
  if sha(item['path'])!=item['sha256']:raise ValueError('original head/hidden producer pin differs')
 full_reference=json.loads(Path(head['reference_receipt']['path']).read_bytes())
 if full_reference.get('complete') is not True or full_reference.get('model_id')!=release['model_id'] or full_reference.get('stage')!='qwen3-full-reference':raise ValueError('matching actual original hidden producer required')
 import numpy as np
 import torch
 from safetensors import safe_open
 with safe_open(head['embedding_shard']['path'],framework='pt',device='cpu') as tensors:
  trained_rows=tensors.get_slice('model.embed_tokens.weight')[32:40].float()
 if torch.get_num_threads()!=1 or torch.get_num_interop_threads()!=1 or os.environ.get('OMP_NUM_THREADS')!='1' or os.environ.get('MKL_NUM_THREADS')!='1':raise ValueError('frozen one-thread CPU/head-math state differs')
 if trained_rows.shape[0]!=8 or not torch.isfinite(trained_rows).all():raise ValueError('invalid original A-H trained rows')
 import onnxruntime as ort
 from onnxruntime.quantization.matmul_nbits_quantizer import MatMulNBitsQuantizer
 import onnx
 from graph_closure import graph_closure
 from projection_census import census
 np.random.seed(1908)
 started=time.monotonic();out.mkdir();result={'schema':'airk.private-context-weight8-numeric/1','stage':'qwen3-context-weight8','model_id':release['model_id'],'root_release_sha256':sha(a.release),'source_export_receipt_sha256':release['source_export_receipt_sha256'],'complete':False,'rows':[],'quality_or_calibration_or_native_qualified':False}
 try:
  layers=28 if release['model_id']=='Qwen/Qwen3-1.7B' else 36
  original=onnx.load_model(str(source/'model.onnx'),load_external_data=False);result['original_projection_census']=census(original,layers,False);del original;gc.collect()
  quantizer=MatMulNBitsQuantizer(str(source/'model.onnx'),bits=8,block_size=32,is_symmetric=True,accuracy_level=0,op_types_to_quantize=('MatMul',))
  quantizer.process();result['weight8_projection_census']=census(quantizer.model.model,layers,True)
  kept={t.name:t.data_type for t in quantizer.model.model.graph.initializer}
  if any(kept.get(name)==1 for name in result['original_projection_census']['constant_projection_initializers']):raise ValueError('unused original FP32 projection coefficients retained')
  quantizer.model.save_model_to_file(str(out/'model.onnx'),True);del quantizer;gc.collect()
  before=graph_closure(out);result['graph_closure_before_ORT']=before
  options=ort.SessionOptions();options.intra_op_num_threads=1;options.inter_op_num_threads=1;options.execution_mode=ort.ExecutionMode.ORT_SEQUENTIAL;options.enable_cpu_mem_arena=False
  options.add_session_config_entry('session.intra_op.allow_spinning','0');options.add_session_config_entry('session.inter_op.allow_spinning','0')
  session=ort.InferenceSession(str(out/'model.onnx'),options,providers=['CPUExecutionProvider'])
  names=['input_ids','attention_mask','position_ids','decision_positions','candidate_positions','option_mask']
  if [x.name for x in session.get_inputs()]!=names or [x.name for x in session.get_outputs()]!=['scores']:raise ValueError('weight8 six-input scores-only ABI differs')
  for i in range(3):
   arrays=np.load(source/('case-%d.npz'%i),allow_pickle=False);feed={name:arrays[name] for name in names};expected=arrays['reference_scores'];actual=session.run(['scores'],feed)[0]
   hidden_path=Path(head['reference_receipt']['path']).parent/('case-%d.npz'%i)
   if sha(hidden_path)!=full_reference['artifact_sha256'][hidden_path.name]:raise ValueError('actual hidden tensor bytes changed')
   hidden_case=np.load(hidden_path,allow_pickle=False)
   if any(not np.array_equal(hidden_case[name],feed[name]) for name in names):raise ValueError('original hidden and scored six-tensor inputs differ')
   hidden=hidden_case['reference_hidden'];count=feed['candidate_positions'].shape[1];decision=int(feed['decision_positions'][0])
   if hidden.dtype!=np.float32 or hidden.shape!=(1,feed['input_ids'].shape[1],trained_rows.shape[1]) or not np.array_equal(feed['candidate_positions'][0],np.arange(32,32+count)):raise ValueError('original readout shape or A-H binding differs')
   with torch.inference_mode():expected_all_live=torch.nn.functional.linear(torch.from_numpy(hidden[:,decision,:]),trained_rows[:count]).numpy()
   if not np.isfinite(expected_all_live).all() or np.max(np.abs(expected_all_live[feed['option_mask']]-expected[feed['option_mask']]))>0.001:raise ValueError('original-row known-live control exceeds original1e-3 tolerance')
   if actual.shape!=expected.shape or not np.isfinite(actual).all() or not np.isfinite(expected).all():raise ValueError('invalid weight8 readout')
   def probability(scores):
    exp=np.exp(scores-np.max(scores,axis=-1,keepdims=True));return exp/np.sum(exp,axis=-1,keepdims=True)
   raw=float(np.max(np.abs(actual-expected)));normalized=float(np.max(np.abs(probability(actual)-probability(expected))));argmax=bool(np.argmax(actual)==np.argmax(expected))
   # Actual native Context inputs use all-true masks. Toggle only this output
   # mask: existing active scores must be unchanged and H must become finite/live.
   all_live=dict(feed);all_live['option_mask']=np.ones_like(feed['option_mask']);unmasked=session.run(['scores'],all_live)[0]
   unchanged=float(np.max(np.abs(unmasked[feed['option_mask']]-actual[feed['option_mask']])))
   live_raw=float(np.max(np.abs(unmasked-expected_all_live)));live_normalized=float(np.max(np.abs(probability(unmasked)-probability(expected_all_live))));live_argmax=bool(np.argmax(unmasked)==np.argmax(expected_all_live))
   if not np.isfinite(unmasked).all() or unchanged>1e-6 or np.any(unmasked[~feed['option_mask']]==-1e4):raise ValueError('actual all-live mask control differs')
   row={'tokens':int(feed['input_ids'].shape[1]),'options':int(feed['candidate_positions'].shape[1]),'raw_max_error':raw,'normalized_mass_max_error':normalized,'argmax_matches':argmax,'reference_order':np.argsort(-expected[0],kind='stable').tolist(),'weight8_order':np.argsort(-actual[0],kind='stable').tolist(),'all_live_active_scores_max_error':unchanged,'all_live_original_row_max_error':live_raw,'all_live_normalized_mass_max_error':live_normalized,'all_live_argmax_matches':live_argmax}
   result['rows'].append(row);np.savez_compressed(out/('case-%d.npz'%i),**feed,reference_scores=expected,weight8_scores=actual,weight8_all_live_scores=unmasked,original_trained_rows_all_live_scores=expected_all_live)
   if raw>0.25 or normalized>0.05 or not argmax or live_raw>0.25 or live_normalized>0.05 or not live_argmax:raise ValueError('predeclared separate weight8 numeric tolerance exceeded')
  del session;gc.collect()
  after=graph_closure(out,expected_extra=('case-0.npz','case-1.npz','case-2.npz'))
  if after!=before:raise ValueError('graph/full external file ranges changed during ORT')
  result['graph_closure_after_ORT']=after
  for name,pin in prior['artifact_sha256'].items():
   if sha(source/name)!=pin:raise ValueError('source drift during quantization')
  result['complete']=True
 finally:
  result['elapsed_seconds']=time.monotonic()-started;result['artifact_sha256']={x.name:sha(x) for x in out.iterdir() if x.is_file()};result['numeric_scope']='Approximate selected weight8 artifact only; FP32 exporter retained its original1e-3 gate. Includes original trained-row neural readout on pinned hidden vectors, not zero-NN. No task labels, fitted calibration, native CPU fit or platform/publication qualification.'
  (out/'receipt.json').write_text(json.dumps(result,indent=2)+'\n',encoding='utf-8');print(json.dumps({k:v for k,v in result.items() if not k.startswith('graph_closure')}),flush=True)
if __name__=='__main__':main()
