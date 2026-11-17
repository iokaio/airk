# SPDX-License-Identifier: Apache-2.0
import hashlib,json,sys
from pathlib import Path
import numpy as np,onnx,onnxruntime as ort
sys.path.insert(0,str(Path(__file__).resolve().parents[3]/'tools/retrieval-train'))
from signed_dynamic import quantize_signed_dynamic
root=Path(__file__).parent/'signed-dynamic-diagnostic-r1';root.mkdir(exist_ok=False)
weight=np.full((256,64),127,dtype=np.float32);h=onnx.helper
model=h.make_model(h.make_graph([h.make_node('MatMul',['a','w'],['y'])],'signed-producer-proof',[h.make_tensor_value_info('a',onnx.TensorProto.FLOAT,[1,256])],[h.make_tensor_value_info('y',onnx.TensorProto.FLOAT,[1,64])],[onnx.numpy_helper.from_array(weight,'w')]),opset_imports=[h.make_opsetid('',18)])
model.ir_version=10;onnx.save_model(model,str(root/'fp32.onnx'));quantize_signed_dynamic(root/'fp32.onnx',root/'signed.onnx')
opts=ort.SessionOptions();opts.intra_op_num_threads=1;opts.inter_op_num_threads=1
session=ort.InferenceSession(str(root/'signed.onnx'),opts,providers=['CPUExecutionProvider']);actual=session.run(None,{'a':np.full((1,256),127,dtype=np.float32)})[0]
expected=4129024;maximum=float(np.max(np.abs(actual-expected)));assert maximum==0
graph=onnx.load(str(root/'signed.onnx'));qlinear=[n for n in graph.graph.node if n.op_type=='QuantizeLinear'];assert qlinear
report={'expected_independent_scalar_sum':expected,'actual_unique':np.unique(actual).tolist(),'maximum_error':maximum,'graph_sha256':hashlib.sha256((root/'signed.onnx').read_bytes()).hexdigest(),'quantizer_source_sha256':hashlib.sha256((Path(__file__).resolve().parents[3]/'tools/retrieval-train/signed_dynamic.py').read_bytes()).hexdigest(),'quantize_nodes':len(qlinear),'types':'Signed activation and signed weight, symmetric fullrange[-127,127] avoids U8S8pair saturation; not reducedrange7bit'}
(root/'receipt.json').write_text(json.dumps(report,indent=2)+'\n');print(json.dumps(report))
