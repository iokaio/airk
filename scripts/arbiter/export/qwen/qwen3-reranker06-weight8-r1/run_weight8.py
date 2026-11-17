# SPDX-License-Identifier: Apache-2.0
"""One separate root-released static512 weight8 conversion and numeric control stage."""
import gc
import importlib.metadata
import json
import os
from pathlib import Path
import sys
import time

HERE=Path(__file__).resolve().parent
STATIC=HERE.parent/'qwen3-reranker06-static-export-r1'
sys.path.insert(0,str(STATIC))
from graph_closure import graph_closure,digest
sys.path.insert(0,str(HERE.parent/'qwen3-reranker06-experiment-r1'))
import run_focus_export as reference

def run(output, *, source):
    output=Path(output).resolve();source=Path(source).resolve()
    if not source.is_relative_to(reference.ROOT/'artifacts'):raise ValueError('Private explicit FP32 source required')
    if output.exists() or not output.is_relative_to(reference.ROOT/'artifacts'):raise ValueError('Fresh private weight8 output required')
    if os.environ.get('CUDA_VISIBLE_DEVICES')!='':raise ValueError('Weight8 stage must remain CPU-only')
    prior=json.loads((source/'receipt.json').read_bytes())
    if prior.get('complete') is not True or prior['phase']!='static512-fp32-reference-export' or len(prior['rows'])!=12:raise ValueError('Complete selected static512 numerical reference required')
    pins=dict(prior['input_pins']);pins[str(source/'receipt.json')]=digest(source/'receipt.json')
    for asset in prior['assets']:
        if Path(asset['name']).name!=asset['name']:raise ValueError('Input graph asset is not local')
        path=source/asset['name']
        if path.is_symlink() or path.stat().st_size!=asset['bytes'] or digest(path)!=asset['sha256']:raise ValueError('Complete FP32 graph/reference asset differs')
        pins[str(path)]=asset['sha256']
    def verify():
        if any(digest(path)!=expected for path,expected in pins.items()):raise ValueError('Pinned weight8 input changed')
    verify();output.mkdir();started=time.monotonic()
    receipt={'schema':1,'phase':'static512-weight8-numeric','complete':False,'qualification':False,'input_pins':pins,'rows':[],
             'precision':{'weights':'MatMulNBits8','block_size':32,'symmetric':True,'accuracy_level':0,'activations':'float32','embedding':'unchanged-float32'},
             'frozen_limits':{'raw_absolute':0.25,'relevance_absolute':0.05,'argmax_exact':True}}
    try:
        import numpy as np
        import onnx
        import onnxruntime as ort
        from onnxruntime.quantization.matmul_nbits_quantizer import MatMulNBitsQuantizer
        # Metadata-only closure first; do not silently ignore any source sidecar.
        if graph_closure(source,expected_extra=['reference512-cases.npz','receipt.json'])!=prior['graph_closure']:raise ValueError('Original FP32 external tensor closure differs')
        metadata=onnx.load_model(str(source/'model.onnx'),load_external_data=False)
        initializers={tensor.name for tensor in metadata.graph.initializer}
        projection_names={node.name for node in metadata.graph.node if node.op_type=='MatMul' and len(node.input)==2 and node.input[1]in initializers}
        if len(projection_names)!=197:raise ValueError('Full28layertrainedprojection/readout census differs')
        del metadata
        phase=time.monotonic()
        quantizer=MatMulNBitsQuantizer(str(source/'model.onnx'),bits=8,block_size=32,is_symmetric=True,accuracy_level=0,op_types_to_quantize=('MatMul',))
        quantizer.process()
        graph=quantizer.model.model
        converted=[node for node in graph.graph.node if node.op_type=='MatMulNBits']
        if len(converted)!=197:raise ValueError('Quantization did not convert every original trained projection/readout')
        for node in converted:
            attrs={attr.name:onnx.helper.get_attribute_value(attr)for attr in node.attribute}
            # The pinned quantizer intentionally omits optional accuracy_level=0.
            if node.domain!='com.microsoft' or len(node.input)!=3 or attrs.get('bits')!=8 or attrs.get('block_size')!=32 or attrs.get('accuracy_level',0)!=0:
                raise ValueError('Quantized projection uses a different kernel precision')
        remaining={tensor.name for tensor in graph.graph.initializer}
        if any(node.op_type=='MatMul' and len(node.input)==2 and node.input[1]in remaining for node in graph.graph.node):raise ValueError('Unconverted constant trained matrix remains')
        quantizer.model.save_model_to_file(str(output/'model.onnx'),True)
        del graph,converted,quantizer
        gc.collect();receipt['conversion_seconds']=time.monotonic()-phase
        closure=graph_closure(output)
        if [asset['name']for asset in closure['files']]!=['model.onnx','model.onnx.data']:raise ValueError('Production candidate must consolidate into one model and one weights asset')
        case_path=source/'reference512-cases.npz';cases=np.load(case_path)
        phase=time.monotonic();session=reference.ort_session(ort,output/'model.onnx',1);receipt['session_create_seconds']=time.monotonic()-phase
        for i,row in enumerate(prior['rows']):
            phase=time.monotonic()
            actual=session.run(['logits'],{name:cases[f'{name}_{i}']for name in ['input_ids','attention_mask','position_ids']})[0]
            elapsed=time.monotonic()-phase;expected=cases[f'expected_{i}']
            if actual.shape!=(1,2) or expected.shape!=(1,2) or not np.isfinite(actual).all() or not np.isfinite(expected).all():raise ValueError('Invalid finite two-logit weight8 readout')
            raw,score=reference.errors(np,actual,expected);argmax=bool(np.argmax(actual)==np.argmax(expected))
            receipt['rows'].append(dict(name=row['name'],actual_logits=actual.tolist(),publisher_logits=expected.tolist(),raw_max_error=raw,relevance_error=score,argmax_matches=argmax,forward_seconds=elapsed))
            if raw>0.25 or score>0.05 or not argmax:raise ValueError('Weight8 exceeds independently frozen numeric limits')
        del session
        if graph_closure(output)!=closure:raise ValueError('Quantized graph/sidecars changed during numerical controls')
        verify();(output/'reference512-cases.npz').write_bytes(case_path.read_bytes())
        receipt.update(complete=True,graph_closure=closure,quantized_projection_count=197,assets=closure['files']+[dict(name='reference512-cases.npz',bytes=(output/'reference512-cases.npz').stat().st_size,sha256=digest(output/'reference512-cases.npz'))])
    except BaseException as error:
        receipt.update(error_type=type(error).__name__,error=str(error));raise
    finally:
        receipt['elapsed_seconds']=time.monotonic()-started
        receipt['versions']={name:importlib.metadata.version(name)for name in ['onnx','onnxruntime','numpy']}
        (output/'receipt.json').write_text(json.dumps(receipt,indent=2)+'\n',encoding='utf8')
        print(json.dumps({'complete':receipt['complete'],'rows':len(receipt['rows']),'elapsed_seconds':receipt['elapsed_seconds'],'error':receipt.get('error')}),flush=True)


if __name__ == "__main__":
    import argparse
    parser = argparse.ArgumentParser(description="Owned Focus weight8 conversion; requires a fresh reviewed execution seal")
    parser.add_argument("--output", type=Path, required=True)
    parser.add_argument("--source", type=Path, required=True)
    args = parser.parse_args()
    run(args.output, source=args.source)
