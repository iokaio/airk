# SPDX-License-Identifier: Apache-2.0
import hashlib,json
from pathlib import Path
import numpy as np,onnx,onnxruntime as ort
root=Path(__file__).parent;source=root/'minilm-exports-r2';out=root/'encoder-precision-diagnostic-r1';out.mkdir(exist_ok=False)
report=json.loads((source/'receipt.json').read_text());cases=report['profiles'][0]['numerical_cases'];sessions=[];output_names=None
for tag,file in [('fp32',source/'fp32.onnx'),('u8',source/'u8/model.onnx')]:
 model=onnx.load(str(file));names=[node.output[0] for node in model.graph.node if node.op_type=='LayerNormalization'];output_names=names
 old={value.name for value in model.graph.output}
 for name in names:
  if name not in old:model.graph.output.append(onnx.helper.make_tensor_value_info(name,onnx.TensorProto.FLOAT,[1,None,384]))
 path=out/(tag+'.onnx');onnx.save(model,str(path));opts=ort.SessionOptions();opts.intra_op_num_threads=1;opts.inter_op_num_threads=1;sessions.append(ort.InferenceSession(str(path),opts,providers=['CPUExecutionProvider']))
results=[]
for case in [cases[0],cases[6]]:
 feed={k:np.asarray(v,dtype=np.int64) for k,v in case['inputs'].items()};observed=[session.run(output_names,feed) for session in sessions];rows=[]
 for name,reference,actual in zip(output_names,*observed):
  pool=lambda h:(h*feed['attention_mask'][:,:,None]).sum(1)[0]/feed['attention_mask'].sum()
  a,b=pool(reference),pool(actual);a/=np.linalg.norm(a);b/=np.linalg.norm(b)
  rows.append({'layer':name,'cosine':float(a@b),'l2':float(np.linalg.norm(a-b)),'max_hidden_error':float(np.max(np.abs(reference-actual)))})
 results.append({'text':case['text'],'layers':rows})
(out/'receipt.json').write_text(json.dumps({'source_sha256':hashlib.sha256(Path(__file__).read_bytes()).hexdigest(),'cases':results},indent=2)+'\n');print(json.dumps(results))
