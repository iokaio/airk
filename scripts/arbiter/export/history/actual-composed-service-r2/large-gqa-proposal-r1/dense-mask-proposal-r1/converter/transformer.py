# SPDX-License-Identifier: Apache-2.0
"""Pinned Large-only explicit-mask dense candidate; no execution without reviewed flag."""
from pathlib import Path
import argparse,copy,hashlib,json,os,struct,sys,time
SOURCE_SHA='e4cd63d26c1e5b8bbfd28c65cb04f5ab3946f57e5511d2cd09e125cdf46c4aca'
ORIGINAL_SHA='f8ba5d3ea8a4269739ec5e517e61e4ec1f2f94560ea60014a4f5a9476c32fd94'
PRIOR_PROOF_SHA='6a9cc23d3d9172fab09fe370207361830f33003ec61bded61f7d3adddc41e42b'
NS='/airk_large_gqa/'
MASK='/scorer/Where_output_0'
BIAS_SHA='8bf893388fce7d661bc12b012aa8fb35d25cf23c8c4d226209304f5622387f22'
def need(v,message):
    if not v:raise ValueError(message)
def digest(v):return hashlib.sha256(v).hexdigest()
def bias_closure(model):
    produced={v:n for n in model.graph.node for v in n.output};initial={v.name:v for v in model.graph.initializer};inputs={v.name:v for v in model.graph.input};closed={};stack=[MASK]
    while stack:
        value=stack.pop()
        if value in produced:
            node=produced[value];key='node:'+node.name
            if key in closed:continue
            closed[key]=node.SerializeToString(deterministic=True);stack.extend(v for v in node.input if v)
        elif value in initial:closed['initializer:'+value]=initial[value].SerializeToString(deterministic=True)
        else:
            need(value in inputs,'Unresolved original bias ancestor');closed['input:'+value]=inputs[value].SerializeToString(deterministic=True)
    return closed
def closure_digest(closed):
    h=hashlib.sha256()
    for key,value in sorted(closed.items()):
        k=key.encode();h.update(len(k).to_bytes(8,'little'));h.update(k);h.update(len(value).to_bytes(8,'little'));h.update(value)
    return h.hexdigest()
def validate_prior_node(attrs,inputs,outputs,domain):
    need(attrs.get('num_heads')==32 and attrs.get('kv_num_heads')==8 and attrs.get('causal')==1 and attrs.get('local_window_size')==-1 and attrs.get('do_rotary')==0 and attrs.get('softcap')==0. and attrs.get('smooth_softmax')==0 and struct.pack('<f',attrs.get('scale',0.)).hex()=='f304b53d','Prior exact Large attention attributes differ')
    need(domain=='com.microsoft' and len(inputs)==11 and inputs[10]==MASK and not any(inputs[3:5]) and not any(inputs[7:10]) and len(outputs)==3,'Original full bias/private-cache/no-past boundary required')
def rewrite(prior,original,onnx):
    need(closure_digest(bias_closure(original))==BIAS_SHA,'Original causal/padding expression differs')
    need(bias_closure(prior)==bias_closure(original),'Prior graph changed original full bias expression')
    result=copy.deepcopy(prior);selected=[n for n in result.graph.node if n.op_type=='GroupQueryAttention']
    need(len(selected)==36,'Complete36 Large GQA population required')
    for index,node in enumerate(selected):
        attrs={a.name:onnx.helper.get_attribute_value(a) for a in node.attribute}
        validate_prior_node(attrs,list(node.input),list(node.output),node.domain)
        causal=[v for v in node.attribute if v.name=='causal'];need(len(causal)==1 and causal[0].type==onnx.AttributeProto.INT,'Exactly one integer causal attribute required');causal[0].i=0
    reverted=copy.deepcopy(result)
    for node in reverted.graph.node:
        if node.op_type=='GroupQueryAttention':next(a for a in node.attribute if a.name=='causal').i=1
    need(reverted.SerializeToString(deterministic=True)==prior.SerializeToString(deterministic=True),'Any prior graph byte except causal attributes changed')
    need(bias_closure(result)==bias_closure(original),'Candidate changed original bias expression')
    return result
def main():
    started=time.monotonic();p=argparse.ArgumentParser(description=__doc__)
    for name in ['source','source-sha256','original-source','original-source-sha256','prior-proof','prior-proof-sha256','python-site']:p.add_argument('--'+name,required=True)
    for name in ['output','cpu-shape-source','cpu-shape-source-sha256','execution-plan','execution-plan-sha256']:p.add_argument('--'+name)
    p.add_argument('--execute-graph-only',action='store_true');a=p.parse_args()
    raw=Path(a.source).read_bytes();original_raw=Path(a.original_source).read_bytes();proof_raw=Path(a.prior_proof).read_bytes()
    need(digest(raw)==a.source_sha256==SOURCE_SHA and digest(original_raw)==a.original_source_sha256==ORIGINAL_SHA and digest(proof_raw)==a.prior_proof_sha256==PRIOR_PROOF_SHA,'Exact source/ancestor/proof pins required')
    sys.path.insert(0,str(Path(a.python_site).resolve(strict=True)));import onnx
    prior=onnx.ModelProto();prior.ParseFromString(raw);original=onnx.ModelProto();original.ParseFromString(original_raw)
    if not a.execute_graph_only:print(json.dumps({'source_sha256':SOURCE_SHA,'candidate_created':False,'model_forward':False}));return 0
    output=Path(a.output).absolute();need(not output.exists() and output.parent.is_dir(),'Fresh protected output required')
    plan_raw=Path(a.execution_plan).read_bytes();need(digest(plan_raw)==a.execution_plan_sha256,'Frozen graph plan drift');plan=json.loads(plan_raw)
    sys.path.insert(0,plan['owner_root']);import reference_worker;reference_worker.verify_loaded_runtime(plan)
    proof=json.loads(proof_raw);need(proof['source_sha256']==ORIGINAL_SHA and proof['new_graph_sha256']==SOURCE_SHA,'Prior actual graph ancestry differs')
    from gate_protocol import original_external_context
    with original_external_context(a.original_source,output.parent):onnx.checker.check_model(prior,full_check=False,check_custom_domain=False)
    result=rewrite(prior,original,onnx)
    with original_external_context(a.original_source,output.parent):onnx.checker.check_model(result,full_check=False,check_custom_domain=False)
    from check_shapes import check
    proof['custom_shape_witnesses']=check(result,onnx,{'path':a.cpu_shape_source,'sha256':a.cpu_shape_source_sha256})
    data=result.SerializeToString(deterministic=True)
    proof.update(source_sha256=ORIGINAL_SHA,rewrite_input_sha256=SOURCE_SHA,prior_shape_proof_sha256=PRIOR_PROOF_SHA,new_graph_sha256=digest(data),bytes=len(data),attention_mode='large_explicit_mask_dense/1',causal_attribute=0,global_flash_setting='0',only_prior_graph_causal_attributes_changed=True,original_bias_name=MASK,original_bias_closure_sha256=BIAS_SHA,original_bias_closure_entries=29,original_bias_preserved_verbatim=True,model_forward=False,custom_shape_gate_pending=False,native_kernel_gate_pending=True)
    reference_worker.verify_loaded_runtime(plan);need(time.monotonic()-started<30,'Original graph-only30s cap exceeded')
    with output.open('xb') as f:f.write(data);f.flush();os.fsync(f.fileno())
    with output.with_suffix('.proof.json').open('x',encoding='utf-8') as f:json.dump(proof,f,indent=2);f.write('\n')
    print(json.dumps(proof));return 0
if __name__=='__main__':raise SystemExit(main())
