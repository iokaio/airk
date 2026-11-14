# SPDX-License-Identifier: Apache-2.0
"""Standard ONNX per-token U8 quantization and exact zero-point corrections."""
import hashlib


def per_token_unsigned(model):
    import numpy as np
    import onnx
    graph=model.graph;initializers={value.name:value for value in graph.initializer}
    occupied={name for node in graph.node for name in list(node.input)+list(node.output)}|set(initializers)
    nodes=[];pairs={};count=0
    def names(key,suffixes):
        prefix='airk_row_'+hashlib.sha256(key.encode()).hexdigest()[:24]
        values=[prefix+'_'+suffix for suffix in suffixes]
        if set(values)&occupied:raise ValueError('per-token namespace collision')
        occupied.update(values)
        return values
    def constant(name,array):graph.initializer.append(onnx.numpy_helper.from_array(array,name))
    def node(kind,inputs,outputs,**attributes):return onnx.helper.make_node(kind,inputs,outputs,**attributes)
    for original in graph.node:
        if original.op_type=='DynamicQuantizeLinear':
            if len(original.input)!=1 or len(original.output)!=3:raise ValueError('unexpected dynamic quantization signature')
            quantized,scale,zero=original.output
            axes,f0,f255,f1,minimum,maximum,low,high,width,rawscale,iszero,negative,zerofloat,zeroround,zeroclip,scaled,rounded,shifted,clipped,qi,zi,qsum=names(quantized,('axes','f0','f255','f1','min','max','lo','hi','width','scale','iszero','negative','zerofloat','zeroround','zeroclip','scaled','rounded','shifted','clipped','qi','zi','qsum'))
            for name,array in ((axes,np.asarray([-1],dtype=np.int64)),(f0,np.asarray(0,dtype=np.float32)),(f255,np.asarray(255,dtype=np.float32)),(f1,np.asarray(1,dtype=np.float32))):constant(name,array)
            a=original.input[0]
            nodes.extend([node('ReduceMin',[a,axes],[minimum],keepdims=1),node('ReduceMax',[a,axes],[maximum],keepdims=1),
                node('Min',[minimum,f0],[low]),node('Max',[maximum,f0],[high]),node('Sub',[high,low],[width]),
                node('Div',[width,f255],[rawscale]),node('Equal',[width,f0],[iszero]),node('Where',[iszero,f1,rawscale],[scale]),
                node('Neg',[low],[negative]),node('Div',[negative,scale],[zerofloat]),node('Round',[zerofloat],[zeroround]),
                node('Clip',[zeroround,f0,f255],[zeroclip]),node('Cast',[zeroclip],[zero],to=onnx.TensorProto.UINT8),
                node('Div',[a,scale],[scaled]),node('Round',[scaled],[rounded]),node('Add',[rounded,zeroclip],[shifted]),
                node('Clip',[shifted,f0,f255],[clipped]),node('Cast',[clipped],[quantized],to=onnx.TensorProto.UINT8),
                node('Cast',[quantized],[qi],to=onnx.TensorProto.INT32),node('Cast',[zero],[zi],to=onnx.TensorProto.INT32),
                node('ReduceSum',[qi,axes],[qsum],keepdims=1)])
            pairs[quantized]=(zero,zi,qsum)
            continue
        if original.op_type!='MatMulInteger':nodes.append(original);continue
        if len(original.input)!=4 or original.input[0] not in pairs:raise ValueError('unreviewed integer product')
        a,b,za,zb=original.input;zero,zi,qsum=pairs[a]
        if za!=zero or b not in initializers or zb not in initializers:raise ValueError('integer product zero-point provenance differs')
        weight=initializers[b];zero_b=initializers[zb]
        if weight.data_type!=onnx.TensorProto.UINT8 or zero_b.data_type!=onnx.TensorProto.UINT8 or len(weight.dims)!=2 or not 0<weight.dims[0]<=32768:raise ValueError('unreviewed unsigned weight/range')
        array=onnx.numpy_helper.to_array(weight);offset=onnx.numpy_helper.to_array(zero_b)
        if offset.ndim>1 or offset.size not in (1,weight.dims[1]):raise ValueError('weight zero-point shape differs')
        k,bsum,bzero,raw,za_bsum,centered_a,za_k,delta,correction=names(original.output[0],('k','bsum','bzero','raw','za_bsum','centered_a','za_k','delta','correction'))
        for name,value in ((k,np.asarray(weight.dims[0],dtype=np.int32)),(bsum,array.astype(np.int64).sum(axis=0).astype(np.int32)),(bzero,offset.astype(np.int32))):constant(name,value)
        nodes.extend([node('MatMulInteger',[a,b],[raw]),node('Mul',[zi,bsum],[za_bsum]),node('Sub',[raw,za_bsum],[centered_a]),
            node('Mul',[zi,k],[za_k]),node('Sub',[za_k,qsum],[delta]),node('Mul',[delta,bzero],[correction]),
            node('Add',[centered_a,correction],list(original.output))])
        count+=1
    if not count:raise ValueError('no unsigned products lowered')
    changed={name for pair in pairs.values() for name in pair[:1]}|{output for original in graph.node if original.op_type=='DynamicQuantizeLinear' for output in original.output[1:]}
    values=[value for value in graph.value_info if value.name not in changed]
    graph.ClearField('value_info');graph.value_info.extend(values)
    graph.ClearField('node');graph.node.extend(nodes)
    return {'per_token_quantizers':len(pairs),'corrected_matmuls':count,'maximum_inner_dimension':32768,
        'identity':'AqBq-Za*sum(Bq)+(K*Za-sum(Aq))*Zb=(Aq-Za)(Bq-Zb); every int32 intermediate and final bounded by K*65025<=2130739200',
        'activation':'Per-last-dimension asymmetric U8, include zero in range, nearest-even Round, zero-width scale1; original weight quantization/scale bytes unchanged'}
