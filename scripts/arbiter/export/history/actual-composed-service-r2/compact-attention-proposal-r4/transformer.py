# SPDX-License-Identifier: Apache-2.0
"""Guarded Compact-only protobuf attention rewrite. Default is read-only inspection.
No Torch/model load/ORT session is used. Root must review --execute-graph-only.
"""
import argparse,copy,hashlib,heapq,json,os,struct,sys,time
from pathlib import Path
SOURCE_SHA='c79c672513b874ab773ee7df8d4518e110d1e6196ae45076ce718f5b5dc6f7b4'
NS='/airk_compact_gqa/'

def need(ok,message):
    if not ok:raise ValueError(message)
def digest(raw):return hashlib.sha256(raw).hexdigest()
def topo_and_reachable(nodes,outputs,known):
    by_output={};by_name={}
    for node in nodes:
        need(node.name and node.name not in by_name,'Duplicate/unnamed node')
        by_name[node.name]=node
        for output in node.output:
            if output:need(output not in by_output,'Duplicate producer');by_output[output]=node.name
    required=set();queue=list(outputs)
    while queue:
        output=queue.pop()
        if output in known:continue
        need(output in by_output,'Unresolved tensor producer: '+output)
        name=by_output[output]
        if name in required:continue
        required.add(name);queue.extend(value for value in by_name[name].input if value)
    dependencies={name:{by_output[value] for value in by_name[name].input if value in by_output} for name in required}
    children={name:[] for name in required}
    for name,parents in dependencies.items():
        for parent in parents:children[parent].append(name)
    ready=[name for name,parents in dependencies.items() if not parents];heapq.heapify(ready);ordered=[]
    while ready:
        name=heapq.heappop(ready);ordered.append(by_name[name])
        for child in children[name]:
            dependencies[child].remove(name)
            if not dependencies[child]:heapq.heappush(ready,child)
    need(len(ordered)==len(required),'Graph cycle')
    return ordered

def rewrite(model,onnx):
    original=copy.deepcopy(model);graph=model.graph
    need([x.name for x in graph.input]==['input_ids','attention_mask','position_ids','decision_positions','candidate_positions','option_mask'],'Original six-input ABI differs')
    need([x.name for x in graph.output]==['scores'],'Original scores-only output differs')
    need(not model.functions,'Unreviewed local ONNX functions')
    nodes={n.name:n for n in graph.node};producers={o:n for n in graph.node for o in n.output}
    need(len(nodes)==len(graph.node),'Duplicate source node names')
    need(not any(n.name.startswith(NS) for n in graph.node),'Candidate namespace collision')
    initial={x.name:x.SerializeToString(deterministic=True) for x in graph.initializer}
    need(len(initial)==len(graph.initializer),'Duplicate original initializer')
    h=onnx.helper;T=onnx.TensorProto
    small=[h.make_tensor(NS+'qshape',T.INT64,[3],[0,0,2048]),h.make_tensor(NS+'kshape',T.INT64,[3],[0,0,1024]),
        h.make_tensor(NS+'axis1',T.INT64,[1],[1]),h.make_tensor(NS+'one',T.INT64,[1],[1])]
    added=[h.make_node('Shape',['attention_mask'],[NS+'mask_shape'],name=NS+'shape'),
        h.make_node('Gather',[NS+'mask_shape',NS+'axis1'],[NS+'total64'],axis=0,name=NS+'physical_T'),
        h.make_node('Sub',[NS+'total64',NS+'one'],[NS+'seqlen64'],name=NS+'physical_T_minus1'),
        h.make_node('Cast',[NS+'total64'],[NS+'total32'],to=T.INT32,name=NS+'total32'),
        h.make_node('Cast',[NS+'seqlen64'],[NS+'seqlen32'],to=T.INT32,name=NS+'seqlen32')]
    removed_sinks=set();scales=[]
    for index in range(28):
        prefix=f'/scorer/layers.{index}/self_attn/';name=NS+str(index)+'/'
        qk=nodes[prefix+'MatMul'];mul=nodes[prefix+'Mul_8'];masked=nodes[prefix+'Add_2'];softmax=nodes[prefix+'Softmax'];av=nodes[prefix+'MatMul_1'];sink=nodes[prefix+'Reshape_7']
        need(qk.op_type=='MatMul' and mul.op_type=='Mul' and mul.input[0]==qk.output[0],'Original post-QK scale chain differs')
        constant=producers[mul.input[1]];need(constant.op_type=='Constant','Original scale is not a constant')
        tensor=next(a.t for a in constant.attribute if a.name=='value')
        value=onnx.numpy_helper.to_array(tensor)
        need(tensor.data_type==T.FLOAT and value.size==1,'Original scale is not one float32')
        scale=float(value.reshape(-1)[0]);scales.append(struct.pack('<f',scale).hex())
        need(masked.op_type=='Add' and list(masked.input)==[mul.output[0],'/scorer/Where_output_0'],'Original shared mask was changed')
        need(softmax.op_type=='Softmax' and list(softmax.input)==list(masked.output)
            and h.get_attribute_value(softmax.attribute[0])==-1,'Original softmax axis differs')
        need(qk.input[0]==prefix+'Add_output_0' and nodes[prefix+'Add_1'].op_type=='Add','Original post-RoPE boundaries differ')
        need(av.op_type=='MatMul' and nodes[prefix+'v_proj/MatMul'].op_type=='MatMul','Original V boundary differs')
        need(sink.op_type=='Reshape' and list(sink.output)==[prefix+'Reshape_7_output_0'],'Original output boundary differs')
        added.extend([
            h.make_node('Transpose',[prefix+'Add_output_0'],[name+'q_btnh'],perm=[0,2,1,3],name=name+'q_transpose'),
            h.make_node('Reshape',[name+'q_btnh',NS+'qshape'],[name+'q_bth'],allowzero=0,name=name+'q_flatten'),
            h.make_node('Transpose',[prefix+'Add_1_output_0'],[name+'k_btkh'],perm=[0,2,1,3],name=name+'k_transpose'),
            h.make_node('Reshape',[name+'k_btkh',NS+'kshape'],[name+'k_bth'],allowzero=0,name=name+'k_flatten'),
            h.make_node('GroupQueryAttention',[name+'q_bth',name+'k_bth',prefix+'v_proj/MatMul_output_0','','',
                NS+'seqlen32',NS+'total32','','','','/scorer/Where_output_0'],
                [sink.output[0],name+'present_k',name+'present_v'],domain='com.microsoft',
                num_heads=16,kv_num_heads=8,causal=1,local_window_size=-1,scale=scale,do_rotary=0,softcap=0.,smooth_softmax=0,name=name+'GQA')])
        removed_sinks.add(sink.name)
    need(len(set(scales))==1,'Per-layer original scale differs')
    known={i.name for i in graph.input}|set(initial)|{i.name for i in small}
    selected=topo_and_reachable([n for n in graph.node if n.name not in removed_sinks]+added,[x.name for x in graph.output],known)
    retained={n.name for n in selected};removed=[n for n in original.graph.node if n.name not in retained]
    # Original exporter-global Unsqueeze axis constants are outside attention
    # namespaces but become unused only when their exact attention consumers
    # disappear. Refuse every other non-attention removal.
    from gate_protocol import removable_attention_axis_constant
    consumers={}
    for n in original.graph.node:
        for value in n.input:consumers.setdefault(value,[]).append(n)
    removed_names={n.name for n in removed};axis_constants=[];per_layer=[0]*28
    for node in removed:
        if not any(node.name.startswith(f'/scorer/layers.{i}/self_attn/') for i in range(28)):
            constant=None
            if len(node.attribute)==1 and node.attribute[0].name=='value':
                tensor=node.attribute[0].t
                constant=(tensor.data_type,list(tensor.dims),onnx.numpy_helper.to_array(tensor).tolist())
            users=[n for value in node.output for n in consumers.get(value,[])]
            layer=removable_attention_axis_constant(node.name,node.op_type,list(node.input),list(node.output),constant,
                [{'name':n.name,'op':n.op_type,'inputs':list(n.input)} for n in users],removed_names)
            axis_constants.append(node.name);per_layer[layer]+=1
        need(not any(part in node.name for part in ['/q_proj/','/k_proj/','/v_proj/','/o_proj/','/q_norm/','/k_norm/']),'Rewrite removed original projection or Q/K norm')
    need(len(axis_constants)==336 and per_layer==[12]*28,'Exact source-pinned336 attention-axis constants/12 per layer required')
    del graph.node[:];graph.node.extend(selected);graph.initializer.extend(small)
    if not any(x.domain=='com.microsoft' for x in model.opset_import):model.opset_import.append(h.make_opsetid('com.microsoft',1))
    original_nodes={n.name:n.SerializeToString(deterministic=True) for n in original.graph.node}
    for node in graph.node:
        if not node.name.startswith(NS):need(node.SerializeToString(deterministic=True)==original_nodes[node.name],'An original retained node changed')
    now={x.name:x.SerializeToString(deterministic=True) for x in graph.initializer}
    need(all(now.get(name)==value for name,value in initial.items()),'An original initializer/external-range changed')
    need(sum(n.op_type=='GroupQueryAttention' for n in graph.node)==28,'Incomplete GQA fusion')
    need(sum(n.op_type=='Softmax' for n in graph.node)==0,'Unfused attention softmax retained')
    need(sum(n.op_type=='MatMul' for n in graph.node)==197,'Original projections/rotary MatMul census differs')
    need([x.SerializeToString(deterministic=True) for x in graph.input]==[x.SerializeToString(deterministic=True) for x in original.graph.input],'Input ABI changed')
    need([x.SerializeToString(deterministic=True) for x in graph.output]==[x.SerializeToString(deterministic=True) for x in original.graph.output],'Output/cache ABI changed')
    live={o for n in graph.node for o in n.output}|known
    valid_vi=[x for x in graph.value_info if x.name in live];del graph.value_info[:];graph.value_info.extend(valid_vi)
    return model,{'gqa_nodes':28,'removed_global_axis_constants':axis_constants,'axis_constants_per_layer':per_layer,'removed_original_nodes':[n.name for n in removed],'original_scale_float32_bits':scales[0],
        'original_initializers':len(initial),'new_small_shape_initializers':len(small),'original_nodes_preserved_verbatim':True,
        'original_initializers_and_external_ranges_preserved_verbatim':True,'all6inputs_scores_only':True,
        'cache_outputs_internal_only':True,'actual_allocations_unproven_until_native_profile':True}

def main():
    start=time.monotonic();p=argparse.ArgumentParser(description=__doc__)
    p.add_argument('--source',required=True);p.add_argument('--source-sha256',required=True)
    p.add_argument('--python-site',required=True);p.add_argument('--output');p.add_argument('--execute-graph-only',action='store_true')
    p.add_argument('--cpu-shape-source');p.add_argument('--cpu-shape-source-sha256')
    p.add_argument('--execution-plan');p.add_argument('--execution-plan-sha256')
    a=p.parse_args();raw=Path(a.source).read_bytes();need(digest(raw)==a.source_sha256==SOURCE_SHA,'Only exact reviewed original Compact graph admitted')
    sys.path.insert(0,str(Path(a.python_site).resolve(strict=True)));import onnx
    model=onnx.ModelProto();model.ParseFromString(raw)
    if not a.execute_graph_only:
        print(json.dumps({'source_sha256':SOURCE_SHA,'nodes':len(model.graph.node),'outputs':[x.name for x in model.graph.output],
            'graph_created':False,'model_forward':False}));return 0
    need(a.output is not None,'Fresh output path required')
    output=Path(a.output).absolute();need(not output.exists() and output.parent.is_dir(),'Create-new graph output required')
    need(a.cpu_shape_source is not None and a.cpu_shape_source_sha256 is not None,'Pinned native custom shape owner required')
    need(a.execution_plan is not None and a.execution_plan_sha256 is not None,'Frozen private graph-owner plan required')
    plan_raw=Path(a.execution_plan).read_bytes();need(digest(plan_raw)==a.execution_plan_sha256,'Graph plan drift')
    plan=json.loads(plan_raw);sys.path.insert(0,plan['owner_root']);import reference_worker
    reference_worker.verify_loaded_runtime(plan)
    from gate_protocol import original_external_context
    # ModelProto checker resolves relative external ranges against CWD. Enter
    # the real original asset directory for this read-only checker only; never
    # relocate/fabricate weights. Restore fresh output CWD even on rejection.
    with original_external_context(a.source,output.parent):
        onnx.checker.check_model(model,full_check=False,check_custom_domain=False)
    result,proof=rewrite(model,onnx)
    with original_external_context(a.source,output.parent):
        onnx.checker.check_model(result,full_check=False,check_custom_domain=False)
    proof['onnx_checker']='original and candidate; real original external-data context; custom kernel ownership separate'

    from check_shapes import check
    proof['custom_shape_witnesses']=check(result,onnx,{'path':a.cpu_shape_source,'sha256':a.cpu_shape_source_sha256})
    reference_worker.verify_loaded_runtime(plan)
    need(time.monotonic()-start<30,'Original graph-only30s cap exceeded')
    data=result.SerializeToString(deterministic=True)
    # Full custom-op shape/source checks are a separate required owner gate,
    # not inferred from these annotations or from a generic ONNX checker.
    with output.open('xb') as f:f.write(data);f.flush();os.fsync(f.fileno())
    proof.update(source_sha256=SOURCE_SHA,new_graph_sha256=digest(data),bytes=len(data),
        model_forward=False,custom_shape_gate_pending=False,native_kernel_gate_pending=True)
    with output.with_suffix('.proof.json').open('x',encoding='utf-8') as f:json.dump(proof,f,indent=2);f.write('\n')
    print(json.dumps(proof));return 0
if __name__=='__main__':sys.exit(main())
