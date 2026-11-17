# SPDX-License-Identifier: Apache-2.0
import hashlib,json,time
from pathlib import Path
import numpy as np,onnx,onnxruntime as ort
root=Path(__file__).parent/'integer-kernel-diagnostic-r1';root.mkdir(exist_ok=False);results=[]
for signed,value in [(True,127),(True,63),(False,255)]:
 a=np.full((1,256),255,dtype=np.uint8);b=np.full((256,64),value,dtype=np.int8 if signed else np.uint8)
 vi=onnx.helper.make_tensor_value_info
 model=onnx.helper.make_model(onnx.helper.make_graph([onnx.helper.make_node('MatMulInteger',['a','b'],['y'])],'independentintegerprobe',
 [vi('a',onnx.TensorProto.UINT8,[1,256]),vi('b',onnx.TensorProto.INT8 if signed else onnx.TensorProto.UINT8,[256,64])],[vi('y',onnx.TensorProto.INT32,[1,64])]),opset_imports=[onnx.helper.make_opsetid('',18)])
 model.ir_version=10;raw=model.SerializeToString();options=ort.SessionOptions();options.intra_op_num_threads=1;options.inter_op_num_threads=1
 session=ort.InferenceSession(raw,options,providers=['CPUExecutionProvider']);actual=session.run(None,{'a':a,'b':b})[0];expected=255*value*256
 results.append({'signed_weight':signed,'weight_value':value,'expected_integer_sum':expected,'actual_unique':np.unique(actual).tolist(),'maximum_error':int(np.max(np.abs(actual.astype(np.int64)-expected))),'graph_sha256':hashlib.sha256(raw).hexdigest()})
 (root/('signed' if signed else 'unsigned')+str(value)+'.onnx') if False else None
 (root/(('signed' if signed else 'unsigned')+str(value)+'.onnx')).write_bytes(raw)
(root/'receipt.json').write_text(json.dumps({'ort':ort.__version__,'source_sha256':hashlib.sha256(Path(__file__).read_bytes()).hexdigest(),'cases':results},indent=2)+'\n');print(json.dumps(results,indent=2))
