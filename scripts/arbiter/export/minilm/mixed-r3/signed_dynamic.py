# SPDX-License-Identifier: Apache-2.0
"""Pinned ORT1.30 IntegerOps recipe with signed dynamic activations.

The public convenience function hard-codes unsigned activations. The underlying
pinned quantizer can emit standard signed QuantizeLinear/MatMulInteger instead;
this explicit experimental profile must independently pass all native gates.
"""
from pathlib import Path


def quantize_signed_dynamic(source,destination,protected=(),external=False):
    import numpy as np
    import onnx
    from onnxruntime.quantization import QuantType
    from onnxruntime.quantization.onnx_quantizer import ONNXQuantizer
    from onnxruntime.quantization.quant_utils import QuantizationMode,load_model_with_shape_infer
    model=load_model_with_shape_infer(Path(source))
    quantizer=ONNXQuantizer(model,True,False,QuantizationMode.IntegerOps,False,
        QuantType.QInt8,QuantType.QInt8,None,[],list(protected),['MatMul'],
        {'MatMulConstBOnly':True,'WeightSymmetric':True,'ActivationSymmetric':True})
    quantizer.quantize_model()
    graph=quantizer.model.model.graph
    # This low-level signed branch is not covered by ORT's public dynamic
    # convenience API. NumPy int8 subtraction in its range helper overflows
    # 127-(-128) to-1, emitting an invalid negative scale denominator. Supply
    # our explicitly defined symmetric[-127,127] range using Python integer
    # arithmetic, then guard the independently required zero-input case.
    matches=[tensor for tensor in graph.initializer if tensor.name=='fixed_quantization_range_int8']
    if len(matches)!=1:raise ValueError('signed range initializer inventory changed')
    observed=float(onnx.numpy_helper.to_array(matches[0]))
    if observed not in (-.5,127.,127.5):raise ValueError('unreviewed signed range calculation')
    matches[0].CopyFrom(onnx.numpy_helper.from_array(np.asarray(127.,dtype=np.float32),'fixed_quantization_range_int8'))
    minimum='airk_signed_minimum_scale'
    if any(t.name==minimum for t in graph.initializer):raise ValueError('scale guard collision')
    graph.initializer.append(onnx.numpy_helper.from_array(np.asarray(1e-8,dtype=np.float32),minimum))
    guards=[]
    for node in graph.node:
        if node.op_type=='Div' and len(node.input)==2 and node.input[1]=='fixed_quantization_range_int8':
            output=node.output[0];node.output[0]=output+'_unguarded'
            guards.append(onnx.helper.make_node('Max',[node.output[0],minimum],[output],name=output+'_airk_nonzero_guard'))
    if not guards:raise ValueError('signed scale computation inventory changed')
    graph.node.extend(guards)
    quantizer.model.save_model_to_file(str(destination),external)
