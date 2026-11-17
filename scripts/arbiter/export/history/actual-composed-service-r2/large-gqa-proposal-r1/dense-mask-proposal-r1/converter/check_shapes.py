# SPDX-License-Identifier: Apache-2.0
"""No-session custom GQA shape gate; generic ONNX inference is not kernel proof."""
import copy,hashlib,json,struct
from pathlib import Path

def need(ok,message):
    if not ok:raise ValueError(message)
def shape(vi):
    tensor=vi.type.tensor_type
    dims=[]
    for d in tensor.shape.dim:
        need(d.HasField('dim_value') and d.dim_value>0,'Unresolved/nonpositive dimension: '+vi.name)
        dims.append(d.dim_value)
    return tensor.elem_type,dims

def check(model,onnx,pinned_cpu_shape_source):
    # Caller independently pins these actual native sources against its build
    # inventory. No declared/cache annotation is treated as actual allocation.
    need(hashlib.sha256(Path(pinned_cpu_shape_source['path']).read_bytes()).hexdigest()==pinned_cpu_shape_source['sha256'],
        'Actual CPU GQA shape source differs')
    from onnxruntime.tools.symbolic_shape_infer import SymbolicShapeInference
    from transformer import NS
    original=model
    # Pinned Large metadata: external tensors are learned Float weights,
    # never shape-value constants. Keep dtype/dims, prohibit external value IO.
    external_names={v.name for v in original.graph.initializer if v.data_location==onnx.TensorProto.EXTERNAL}
    need(len(external_names)==326 and all(v.data_type==onnx.TensorProto.FLOAT and all(d>0 for d in v.dims)
        for v in original.graph.initializer if v.name in external_names),'Exact external learned-weight metadata required')
    need(not any(a.HasField('t') and a.t.data_location==onnx.TensorProto.EXTERNAL
        for n in original.graph.node for a in n.attribute),'External Constant attribute values not supported')
    by_name={n.name:n for n in model.graph.node}
    gqa=[n for n in model.graph.node if n.op_type=='GroupQueryAttention']
    need(len(gqa)==36 and [n.name for n in model.graph.output]==['scores'],'Exact internal GQA/scores-only boundary required')
    for i in range(36):
        prefix=NS+str(i)+'/'
        for name,source in [('q','/scorer/layers.'+str(i)+'/self_attn/Add_output_0'),
                            ('k','/scorer/layers.'+str(i)+'/self_attn/Add_1_output_0')]:
            trans=by_name[prefix+name+'_transpose'];flat=by_name[prefix+name+'_flatten']
            need(trans.op_type=='Transpose' and list(trans.input)==[source], 'Wrong original post-RoPE boundary')
            need(list(onnx.helper.get_attribute_value(trans.attribute[0]))==[0,2,1,3], 'Direct reshape/head-token reorder refused')
            need(flat.op_type=='Reshape' and flat.input[0]==trans.output[0], 'Missing required transpose/flatten adapter')
    result=[]
    for tokens,options in [(1,1),(17,3),(128,8),(8192,8)]:
        candidate=copy.deepcopy(original)
        for value in candidate.graph.input:
            dims=value.type.tensor_type.shape.dim
            if value.name in ['input_ids','attention_mask','position_ids']:dims[1].dim_value=tokens
            if value.name in ['candidate_positions','option_mask']:dims[1].dim_value=options
        # Extend the instance dispatcher only on an inference copy. The pinned
        # official no-past handler omits cache types; resolve them BEFORE its
        # enclosing global all-output completeness check, never after failure.
        from gate_protocol import cache_contract,dispatch_with_cache,infer_complete,install_external_value_guard
        engine=SymbolicShapeInference(2**31-1,False,False,0)
        stock=engine.dispatcher_['GroupQueryAttention']
        def infer_gqa(node):
            attrs={a.name:onnx.helper.get_attribute_value(a) for a in node.attribute}
            required=[node.input[i] for i in [0,1,2,5,6,10]]
            def contract():
                actual={name:shape(engine.known_vi_[name]) for name in required}
                return cache_contract(list(node.input),list(node.output),attrs,actual,tokens,NS)
            def publish(name,dtype,dims):
                engine.known_vi_[name].CopyFrom(onnx.helper.make_tensor_value_info(name,dtype,dims))
            dispatch_with_cache(contract,lambda:stock(node),publish)
        engine.dispatcher_['GroupQueryAttention']=infer_gqa
        engine._preprocess(candidate)
        value_policy=install_external_value_guard(engine,external_names)
        inferred=infer_complete(engine)
        known={v.name:v for v in list(inferred.graph.input)+list(inferred.graph.value_info)+list(inferred.graph.output)}
        witnesses=[]
        for n in [v for v in inferred.graph.node if v.op_type=='GroupQueryAttention']:
            attrs={a.name:onnx.helper.get_attribute_value(a) for a in n.attribute}
            need(attrs.get('num_heads')==32 and attrs.get('kv_num_heads')==8 and attrs.get('causal')==0
                and attrs.get('local_window_size')==-1 and attrs.get('do_rotary')==0
                and attrs.get('softcap')==0. and attrs.get('smooth_softmax')==0
                and struct.pack('<f',attrs.get('scale',0.)).hex()=='f304b53d','Native GQA head/scale/causal/position contract differs')
            need(len(n.output)==3 and all(n.output) and len(n.input)==11 and not any(n.input[3:5]) and not any(n.input[7:10]),'Internal present outputs/no-past boundary required')
            need(n.input[5]==NS+'seqlen32' and n.input[6]==NS+'total32'
                and n.input[10]=='/scorer/Where_output_0','Original mask or full physical seqlen dependency changed')
            for value,expected in [(n.input[0],[1,tokens,4096]),(n.input[1],[1,tokens,1024]),
                (n.input[2],[1,tokens,1024]),(n.input[10],[1,1,tokens,tokens]),(n.output[0],[1,tokens,4096])]:
                need(value in known and shape(known[value])==(onnx.TensorProto.FLOAT,expected),'GQA type/shape unresolved or wrong: '+value)
            for value in n.input[5:7]:need(value in known and shape(known[value])==(onnx.TensorProto.INT32,[1]),'GQA sequence-length type/shape differs')
            # Actual CPU source: with no past, physical totalT sets present
            # shape [batch,kv_heads,totalT,head_size]. No guessed/unknown axis.
            cache=[1,8,tokens,128]
            for name in n.output[1:]:need(name in known and shape(known[name])==(onnx.TensorProto.FLOAT,cache),'Pinned no-past cache resolution failed')
            witnesses.append({'node':n.name,'context':[1,tokens,4096],
                'cache_shapes_resolved_by_pinned_cpu_compute':[cache,cache],
                'actual_allocation_proof':'pending native profiling; annotations are not allocation'})
        need(shape(known['scores'])==(onnx.TensorProto.FLOAT,[1,options]),'Final trained readout shape unresolved/wrong')
        result.append({'tokens':tokens,'options':options,'all36_resolved':len(witnesses)==36,'external_weight_value_policy':value_policy,'witnesses':witnesses})
    return result
