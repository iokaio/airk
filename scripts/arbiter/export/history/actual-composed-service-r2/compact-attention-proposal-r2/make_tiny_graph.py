# SPDX-License-Identifier: Apache-2.0
"""Prepare one synthetic GQA allocation fixture only after explicit review."""
import argparse,hashlib,json,os,sys
from pathlib import Path

def main():
    p=argparse.ArgumentParser(description=__doc__);p.add_argument('--python-site',required=True)
    p.add_argument('--output',required=True);p.add_argument('--execute-fixture-creation',action='store_true');a=p.parse_args()
    if not a.execute_fixture_creation:
        print(json.dumps({'fixture_created':False,'model_weights':False,'kernel_execution':False}));return 0
    sys.path.insert(0,str(Path(a.python_site).resolve(strict=True)));import onnx
    h=onnx.helper;T=onnx.TensorProto
    inputs=[h.make_tensor_value_info('query',T.FLOAT,[1,'T',2048]),
        h.make_tensor_value_info('key',T.FLOAT,[1,'T',1024]),h.make_tensor_value_info('value',T.FLOAT,[1,'T',1024]),
        h.make_tensor_value_info('bias',T.FLOAT,[1,1,'T','T']),h.make_tensor_value_info('seqlens_k',T.INT32,[1]),
        h.make_tensor_value_info('total_sequence_length',T.INT32,[1])]
    node=h.make_node('GroupQueryAttention',['query','key','value','','','seqlens_k','total_sequence_length','','','','bias'],
        ['context','internal_present_k','internal_present_v'],name='synthetic_gqa',domain='com.microsoft',
        num_heads=16,kv_num_heads=8,causal=1,local_window_size=-1,scale=0.0883883461356163,
        do_rotary=0,softcap=0.,smooth_softmax=0)
    graph=h.make_graph([node],'synthetic_gqa_private_cache',inputs,[h.make_tensor_value_info('context',T.FLOAT,[1,'T',2048])])
    model=h.make_model(graph,opset_imports=[h.make_opsetid('',18),h.make_opsetid('com.microsoft',1)])
    model.ir_version=10;raw=model.SerializeToString(deterministic=True)
    output=Path(a.output).absolute()
    if output.exists() or not output.parent.is_dir():raise ValueError('fresh existing protected output parent required')
    with output.open('xb') as f:f.write(raw);f.flush();os.fsync(f.fileno())
    print(json.dumps({'path':str(output),'bytes':len(raw),'sha256':hashlib.sha256(raw).hexdigest(),
        'model_weights':False,'kernel_execution':False,'actual_allocations_unproven':True}))
    return 0
if __name__=='__main__':sys.exit(main())
