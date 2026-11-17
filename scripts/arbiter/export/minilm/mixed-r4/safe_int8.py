# SPDX-License-Identifier: Apache-2.0
"""Exact U8×S8 integer lowering for AVX2 CPUs with saturating pair products.

Split each unsigned activation and zero point into low7/high1 bits. Two bounded
MatMulInteger calls plus an exact int32 sum preserve the full8-bit mathematical
dot product without reducing weight precision or supplying a custom operator.
"""
import hashlib


def split_unsigned_activation_products(model,base_dir=''):
    import numpy as np
    import onnx
    graph=model.graph
    initializers={tensor.name:tensor for tensor in graph.initializer}
    types={value.name:value.type.tensor_type.elem_type for value in list(graph.input)+list(graph.value_info)+list(graph.output) if value.type.HasField('tensor_type')}
    types.update({name:tensor.data_type for name,tensor in initializers.items()})
    producers={name:node for node in graph.node for name in node.output}
    occupied={name for node in graph.node for name in list(node.input)+list(node.output)}|set(initializers)
    cache={};nodes=[];split_count=0
    for node in graph.node:
        if node.op_type!='MatMulInteger' or len(node.input)<4 or node.input[1] not in initializers or initializers[node.input[1]].data_type!=onnx.TensorProto.INT8:
            nodes.append(node);continue
        weight=initializers[node.input[1]]
        if len(weight.dims)!=2 or not 0<weight.dims[0]<=32768:raise ValueError('integer accumulation dimension exceeds exact int32 bound')
        if node.input[3] not in initializers or initializers[node.input[3]].data_type!=onnx.TensorProto.INT8:raise ValueError('signed weight zero-point type differs')
        zero=onnx.numpy_helper.to_array(initializers[node.input[3]],base_dir=str(base_dir))
        if zero.ndim>1 or zero.size not in (1,weight.dims[1]):raise ValueError('weight zero-point broadcasting differs')
        producer=producers.get(node.input[0])
        dynamic_pair=producer is not None and producer.op_type=='DynamicQuantizeLinear' and producer.output[0]==node.input[0] and producer.output[2]==node.input[2]
        if not dynamic_pair and (types.get(node.input[0])!=onnx.TensorProto.UINT8 or types.get(node.input[2])!=onnx.TensorProto.UINT8):raise ValueError('activation and zero-point must be verified UINT8')
        pair=(node.input[0],node.input[2])
        if pair not in cache:
            prefix='airk_u8s8_'+hashlib.sha256(('\0'.join(pair)).encode()).hexdigest()[:24]
            names=[prefix+suffix for suffix in ('_limit16','_factor32','_a16','_z16','_alo16','_ahi16','_zlo16','_zhi16','_alo','_ahi','_zlo','_zhi')]
            if set(names)&occupied:raise ValueError('activation split namespace collision')
            occupied.update(names)
            limit,factor,a16,z16,alo16,ahi16,zlo16,zhi16,alo,ahi,zlo,zhi=names
            graph.initializer.extend([onnx.numpy_helper.from_array(np.asarray(128,dtype=np.int16),limit),onnx.numpy_helper.from_array(np.asarray(128,dtype=np.int32),factor)])
            nodes.extend([onnx.helper.make_node('Cast',[pair[0]],[a16],to=onnx.TensorProto.INT16),onnx.helper.make_node('Cast',[pair[1]],[z16],to=onnx.TensorProto.INT16),
                onnx.helper.make_node('Mod',[a16,limit],[alo16],fmod=0),onnx.helper.make_node('Div',[a16,limit],[ahi16]),
                onnx.helper.make_node('Mod',[z16,limit],[zlo16],fmod=0),onnx.helper.make_node('Div',[z16,limit],[zhi16]),
                onnx.helper.make_node('Cast',[alo16],[alo],to=onnx.TensorProto.UINT8),onnx.helper.make_node('Cast',[ahi16],[ahi],to=onnx.TensorProto.UINT8),
                onnx.helper.make_node('Cast',[zlo16],[zlo],to=onnx.TensorProto.UINT8),onnx.helper.make_node('Cast',[zhi16],[zhi],to=onnx.TensorProto.UINT8)])
            cache[pair]=(alo,ahi,zlo,zhi,factor)
        alo,ahi,zlo,zhi,factor=cache[pair]
        prefix='airk_dot_'+hashlib.sha256(node.output[0].encode()).hexdigest()[:24]
        low,high,scaled=[prefix+suffix for suffix in ('_lo','_hi','_scaled')]
        if {low,high,scaled}&occupied:raise ValueError('integer dot namespace collision')
        occupied.update((low,high,scaled))
        nodes.extend([onnx.helper.make_node('MatMulInteger',[alo,node.input[1],zlo,node.input[3]],[low],name=prefix+'_low'),
            onnx.helper.make_node('MatMulInteger',[ahi,node.input[1],zhi,node.input[3]],[high],name=prefix+'_high'),
            onnx.helper.make_node('Mul',[high,factor],[scaled],name=prefix+'_shift'),
            onnx.helper.make_node('Add',[low,scaled],list(node.output),name=prefix+'_sum')])
        split_count+=1
    if not split_count:raise ValueError('no constant signed integer products to lower')
    graph.ClearField('node');graph.node.extend(nodes)
    return {'split_matmul_nodes':split_count,'shared_activation_splits':len(cache),'maximum_inner_dimension':32768,
        'identity':'(Alo-Zlo)(B-Zb)+128(Ahi-Zhi)(B-Zb)=(A-Za)(B-Zb); Alo,Zlo<=127, Ahi,Zhi<=1, signed weights/zero-points[-128,127]'}


def recode_unsigned_weights(model,base_dir=''):
    import numpy as np
    import onnx
    graph=model.graph;initializers={tensor.name:tensor for tensor in graph.initializer};converted=set()
    targets=[node for node in graph.node if node.op_type=='MatMulInteger' and len(node.input)>=4 and node.input[1] in initializers and initializers[node.input[1]].data_type==onnx.TensorProto.UINT8]
    for node in targets:
        for name in (node.input[1],node.input[3]):
            if name in converted:continue
            tensor=initializers[name]
            if tensor.data_type!=onnx.TensorProto.UINT8:raise ValueError('unsigned source weight parameter type differs')
            array=onnx.numpy_helper.to_array(tensor,base_dir=str(base_dir))
            signed=(array.astype(np.int16)-128).astype(np.int8)
            tensor.CopyFrom(onnx.numpy_helper.from_array(signed,name));converted.add(name)
    if not converted:raise ValueError('no unsigned weights to recode')
    for value in graph.value_info:
        if value.name in converted:value.type.tensor_type.elem_type=onnx.TensorProto.INT8
    report=split_unsigned_activation_products(model,base_dir=base_dir)
    report.update(converted_weight_parameters=len(converted),weight_identity='Qs=Qu-128; Zs=Zu-128; identical scale bytes, so S(Qs-Zs)=S(Qu-Zu) exactly')
    return report


def quantize_signed_safe(source,destination,protected=(),external=False):
    from pathlib import Path
    import onnx
    from onnxruntime.quantization import QuantType,quantize_dynamic
    quantize_dynamic(str(source),str(destination),weight_type=QuantType.QInt8,per_channel=True,reduce_range=False,
        nodes_to_exclude=list(protected),op_types_to_quantize=['MatMul'],use_external_data_format=external,
        extra_options={'MatMulConstBOnly':True,'WeightSymmetric':True})
    model=onnx.load(str(destination),load_external_data=False)
    report=split_unsigned_activation_products(model,base_dir=Path(destination).parent)
    onnx.save_model(model,str(destination))
    return report
