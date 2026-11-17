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
    by_name={n.name:n for n in model.graph.node}
    gqa=[n for n in model.graph.node if n.op_type=='GroupQueryAttention']
    need(len(gqa)==28 and [n.name for n in model.graph.output]==['scores'],'Exact internal GQA/scores-only boundary required')
    for i in range(28):
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
        # This official ORT tool owns GQA context shape propagation. Its no-past
        # cache outputs remain unresolved, so resolve them explicitly with the
        # pinned CPU Compute rule only AFTER every Q/K/V/type/attribute check.
        inferred=SymbolicShapeInference.infer_shapes(candidate,auto_merge=False,guess_output_rank=False)
        known={v.name:v for v in list(inferred.graph.input)+list(inferred.graph.value_info)+list(inferred.graph.output)}
        witnesses=[]
        for n in [v for v in inferred.graph.node if v.op_type=='GroupQueryAttention']:
            attrs={a.name:onnx.helper.get_attribute_value(a) for a in n.attribute}
            need(attrs.get('num_heads')==16 and attrs.get('kv_num_heads')==8 and attrs.get('causal')==1
                and attrs.get('local_window_size')==-1 and attrs.get('do_rotary')==0
                and attrs.get('softcap')==0. and attrs.get('smooth_softmax')==0
                and struct.pack('<f',attrs.get('scale',0.)).hex()=='f304b53d','Native GQA head/scale/causal/position contract differs')
            need(len(n.output)==3 and all(n.output) and len(n.input)==11 and not any(n.input[3:5]) and not any(n.input[7:10]),'Internal present outputs/no-past boundary required')
            need(n.input[5]==NS+'seqlen32' and n.input[6]==NS+'total32'
                and n.input[10]=='/scorer/Where_output_0','Original mask or full physical seqlen dependency changed')
            for value,expected in [(n.input[0],[1,tokens,2048]),(n.input[1],[1,tokens,1024]),
                (n.input[2],[1,tokens,1024]),(n.input[10],[1,1,tokens,tokens]),(n.output[0],[1,tokens,2048])]:
                need(value in known and shape(known[value])==(onnx.TensorProto.FLOAT,expected),'GQA type/shape unresolved or wrong: '+value)
            for value in n.input[5:7]:need(value in known and shape(known[value])==(onnx.TensorProto.INT32,[1]),'GQA sequence-length type/shape differs')
            # Actual CPU source: with no past, physical totalT sets present
            # shape [batch,kv_heads,totalT,head_size]. No guessed/unknown axis.
            cache=[1,8,tokens,128]
            witnesses.append({'node':n.name,'context':[1,tokens,2048],
                'cache_shapes_resolved_by_pinned_cpu_compute':[cache,cache],
                'actual_allocation_proof':'pending native profiling; annotations are not allocation'})
        need(shape(known['scores'])==(onnx.TensorProto.FLOAT,[1,options]),'Final trained readout shape unresolved/wrong')
        result.append({'tokens':tokens,'options':options,'all28_resolved':len(witnesses)==28,'witnesses':witnesses})
    return result
