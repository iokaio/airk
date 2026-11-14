# SPDX-License-Identifier: Apache-2.0
"""Produce an explicit development INT8-weight optional profile; no fallback."""
import argparse
import hashlib
import json
from pathlib import Path
import time

import qwen_export


def annotate_recurrence_outputs(model):
    import onnx
    existing={value.name:value for value in model.graph.value_info}
    count=0
    for node in model.graph.node:
        if node.domain!='com.microsoft' or node.op_type!='LinearAttention':continue
        for name,rank in zip(node.output,(3,4)):
            if not name:continue
            if name in existing:
                if existing[name].type.tensor_type.elem_type not in (0,onnx.TensorProto.FLOAT):raise ValueError('unexpected recurrence output precision')
                existing[name].type.tensor_type.elem_type=onnx.TensorProto.FLOAT
            else:model.graph.value_info.append(onnx.helper.make_tensor_value_info(name,onnx.TensorProto.FLOAT,[None]*rank))
            count+=1
    return count


def main():
    import numpy as np
    import onnx
    import onnxruntime as ort
    from onnxruntime.quantization import QuantType,quantize_dynamic
    parser=argparse.ArgumentParser(description=__doc__)
    parser.add_argument('--input',type=Path,required=True)
    parser.add_argument('--fixtures',type=Path,required=True)
    parser.add_argument('--output',type=Path,required=True)
    parser.add_argument('--precision',choices=('signed7','unsigned8','weight8'),default='signed7')
    parser.add_argument('--protect-ffn-output',action='store_true')
    args=parser.parse_args();output=args.output.resolve()
    if output.exists() or not output.is_relative_to(qwen_export.ROOT/'artifacts'):parser.error('fresh private output required')
    source=Path(__file__).read_bytes();fixture_bytes=args.fixtures.read_bytes();receipt_bytes=(args.input/'receipt.json').read_bytes()
    upstream=json.loads(receipt_bytes);seals={}
    for asset in upstream['assets']:
        path=args.input/asset['name']
        if path.stat().st_size!=asset['bytes'] or qwen_export.digest(path)!=asset['sha256']:raise ValueError('FP32 source profile changed')
        seals[str(path)]=asset['sha256']
    output.mkdir(parents=True);started=time.monotonic()
    prepared=output/'preprocessed-private';prepared.mkdir()
    graph=onnx.load(str(args.input/'model.onnx'),load_external_data=False)
    original_nodes=[node.SerializeToString() for node in graph.graph.node]
    original_initializers=[tensor.SerializeToString() for tensor in graph.graph.initializer]
    annotated=annotate_recurrence_outputs(graph)
    if not annotated:raise ValueError('expected known FP32 recurrence outputs')
    if original_nodes!=[node.SerializeToString() for node in graph.graph.node] or original_initializers!=[tensor.SerializeToString() for tensor in graph.graph.initializer]:raise ValueError('type annotation changed computation')
    inferred=onnx.shape_inference.infer_shapes(graph)
    if any(value.type.HasField('tensor_type') and value.type.tensor_type.elem_type==0 for value in inferred.graph.value_info):raise ValueError('undefined inferred tensor type remains')
    onnx.save_model(graph,str(prepared/'model.onnx'))
    # A private retained copy keeps generic shape inference/quantization from
    # reopening original qualification files. Verify streamed exact bytes.
    for asset in upstream['assets']:
        if asset['name']=='model.onnx':continue
        observed=hashlib.sha256()
        with (args.input/asset['name']).open('rb') as stream,(prepared/asset['name']).open('xb') as sink:
            while chunk:=stream.read(4194304):sink.write(chunk);observed.update(chunk)
        if observed.hexdigest()!=asset['sha256']:raise ValueError('staged source bytes differ')
    protected=[node.name for node in graph.graph.node if node.op_type=='MatMul' and '/mlp/down_proj/' in node.name] if args.protect_ffn_output else []
    if args.protect_ffn_output and not protected:raise ValueError('missing expected FFN output projection inventory')
    if args.precision=='weight8':
        from onnxruntime.quantization.matmul_nbits_quantizer import MatMulNBitsQuantizer
        import logging
        logging.getLogger('onnxruntime.quantization.matmul_nbits_quantizer').setLevel(logging.WARNING)
        quantizer=MatMulNBitsQuantizer(str(prepared/'model.onnx'),bits=8,block_size=32,is_symmetric=True,accuracy_level=0,nodes_to_exclude=protected)
        quantizer.process();quantizer.model.save_model_to_file(str(output/'model.onnx'),True)
        del quantizer
    else:
        quantize_dynamic(str(prepared/'model.onnx'),str(output/'model.onnx'),per_channel=True,reduce_range=args.precision=='signed7',nodes_to_exclude=protected,
            weight_type=QuantType.QInt8 if args.precision=='signed7' else QuantType.QUInt8,op_types_to_quantize=['MatMul'],use_external_data_format=True,
            extra_options={'MatMulConstBOnly':True})
    options=ort.SessionOptions();options.intra_op_num_threads=1;options.inter_op_num_threads=1;options.enable_cpu_mem_arena=False
    session=ort.InferenceSession(str(output/'model.onnx'),options,providers=['CPUExecutionProvider'])
    results=[]
    for case in json.loads(fixture_bytes)['cases']:
        feed={row.name:np.asarray(case[row.name],dtype=np.bool_ if row.type=='tensor(bool)' else np.int64) for row in session.get_inputs()}
        scores=session.run(['scores'],feed)[0];gold=np.asarray(case['raw_scores'])
        softmax=lambda x:np.exp(x-x.max(-1,keepdims=True))/np.exp(x-x.max(-1,keepdims=True)).sum(-1,keepdims=True)
        results.append({'name':case['name'],'reference_scores':gold.tolist(),'int8_scores':scores.tolist(),
            'maximum_raw_score_error':float(np.max(np.abs(scores-gold))),'maximum_softmax_error':float(np.max(np.abs(softmax(scores)-softmax(gold)))),
            'argmax_matches':np.argmax(scores).item()==np.argmax(gold).item()})
    assets=[{'name':p.name,'bytes':p.stat().st_size,'sha256':qwen_export.digest(p)} for p in sorted(output.iterdir()) if p.is_file()]
    if Path(__file__).read_bytes()!=source or args.fixtures.read_bytes()!=fixture_bytes or (args.input/'receipt.json').read_bytes()!=receipt_bytes or any(qwen_export.digest(p)!=sha for p,sha in seals.items()):raise ValueError('quantization input drift')
    precision='FP32 embeddings/norms/GDN with symmetric block32 INT8 constant MatMul weights, FP32 activations and built-in MatMulNBits accuracy_level0' if args.precision=='weight8' else 'FP32 embeddings/norms/GDN with '+('reduced-range signed7bit values stored asINT8' if args.precision=='signed7' else 'full-range unsigned8bit')+' per-output-channel constant MatMul weights and dynamic unsigned8bit activation quantization; explicit non-VNNI CPUprofile'
    report={'schema':'airk.development-int8-optional-profile/1','precision':precision,
        'source_sha256':hashlib.sha256(source).hexdigest(),'source_profile_receipt_sha256':hashlib.sha256(receipt_bytes).hexdigest(),'source_seals':seals,
        'fixtures_sha256':hashlib.sha256(fixture_bytes).hexdigest(),'cases':results,'assets':assets,'elapsed_seconds':time.monotonic()-started,
        'explicit_fp32_recurrence_output_annotations':annotated,
        'protected_fp32_nodes':protected,
        'scope':'Explicit alternate precision experiment. Not a silent fallback or quality/resource/native qualification. Frozen A0 final quality floors unchanged; smoke score/decision drift reported without hiding failures.'}
    (output/'runner.py').write_bytes(source);(output/'receipt.json').write_text(json.dumps(report,indent=2)+'\n')
    print(json.dumps({'assets':assets,'cases':results,'elapsed_seconds':report['elapsed_seconds']}),flush=True)


if __name__=='__main__':main()
