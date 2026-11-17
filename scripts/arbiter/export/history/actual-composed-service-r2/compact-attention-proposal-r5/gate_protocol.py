# SPDX-License-Identifier: Apache-2.0
"""Private orchestration controls; no neural imports, graph/session creation or DTOs."""
from pathlib import Path
import os,struct
from contextlib import contextmanager

def need(ok,message):
    if not ok:raise ValueError(message)

@contextmanager
def original_external_context(source,output_root):
    source=Path(source).resolve(strict=True); output_root=Path(output_root).resolve(strict=True)
    previous=Path.cwd()
    need(previous.resolve()==output_root,'Checker must enter from fresh output working directory')
    try:
        os.chdir(source.parent)
        yield
    finally:
        os.chdir(previous)

def cache_contract(inputs,outputs,attrs,known,tokens,ns):
    need(type(tokens) is int and tokens in [1,17,128,8192],'Unsupported exact shape case')
    need(len(inputs)==11 and not any(inputs[3:5]) and not any(inputs[7:10]),'No-past/no extra GQA inputs required')
    need(len(outputs)==3 and all(outputs) and len(set(outputs))==3,'Three distinct internal outputs required')
    need(attrs.get('num_heads')==16 and attrs.get('kv_num_heads')==8 and attrs.get('causal')==1
         and attrs.get('local_window_size')==-1 and attrs.get('do_rotary')==0
         and attrs.get('softcap')==0. and attrs.get('smooth_softmax')==0
         and struct.pack('<f',attrs.get('scale',0.)).hex()=='f304b53d','Exact pinned CPU GQA attributes required')
    need(inputs[5:7]==[ns+'seqlen32',ns+'total32'] and inputs[10]=='/scorer/Where_output_0','Original mask/full physical seqlen required')
    for name,expected in [(inputs[0],(1,[1,tokens,2048])),(inputs[1],(1,[1,tokens,1024])),
        (inputs[2],(1,[1,tokens,1024])),(inputs[10],(1,[1,1,tokens,tokens])),
        (inputs[5],(6,[1])),(inputs[6],(6,[1]))]:
        need(known.get(name)==expected,'Resolved native GQA input type/shape required: '+name)
    return {outputs[1]:(1,[1,8,tokens,128]),outputs[2]:(1,[1,8,tokens,128])}

def dispatch_with_cache(contract,stock,publish):
    caches=contract()  # Validate actual upstream values before inference extension.
    stock()           # Official context propagation remains authoritative.
    for name,(dtype,dims) in caches.items():publish(name,dtype,dims)

def infer_complete(engine):
    complete=False
    while engine.run_:complete=engine._infer_impl()
    engine._update_output_from_vi()
    need(complete,'Incomplete symbolic shape inference; unresolved shapes refused')
    return engine.out_mp_

def fresh_child_cwd(root,base):
    root=Path(root).resolve(strict=True);base=Path(base).resolve(strict=True)
    need(root!=base and root.parent==base.parent,'Fresh sibling output required')
    return root

def external_save_plan(graph):
    graph=Path(graph)
    need(graph.parent.is_dir() and not graph.exists(),'Fresh optimized model path required')
    external=graph.with_suffix('.weights.bin')
    need(not external.exists(),'Fresh optimized external data path required')
    return external,{'session.optimized_model_external_initializers_file_name':external.name,
        'session.optimized_model_external_initializers_min_size_in_bytes':'1024'}
def removable_attention_axis_constant(name,op,inputs,outputs,constant,users,removed):
    """Only exporter-global axis literals consumed by removed attention Unsqueeze."""
    import re
    need(re.fullmatch(r'Constant_[0-9]+',name) is not None and op=='Constant' and inputs==[]
        and len(outputs)==1 and constant==(7,[1],[0]),'Only exact exported Int64[1] zero-axis Constant allowed outside attention')
    need(len(users)==1,'Exact original single attention-axis consumer required')
    user=users[0]
    match=re.fullmatch(r'/scorer/layers\.([0-9]+)/self_attn/Unsqueeze(?:_[0-9]+)?',user['name'])
    need(match is not None and 0<=int(match.group(1))<28 and user['op']=='Unsqueeze'
        and len(user['inputs'])==2 and user['inputs'][1]==outputs[0]
        and user['name'] in removed,'Constant must be axis of removed attention-only Unsqueeze')
    return int(match.group(1))

def install_external_value_guard(engine,external_names):
    """Keep true tensor metadata; optional external values are unknown, never read."""
    external_names=frozenset(external_names)
    need(all(name in engine.initializers_ and name in engine.known_vi_ for name in external_names),
         'External weight roster must retain actual initializer and shape metadata')
    stock_get=engine._get_value;stock_try=engine._try_get_value
    observed={'external_optional_values_unknown':0,'external_required_values_refused':0}
    def get_value(node,index):
        name=node.input[index]
        if name in external_names:
            observed['external_required_values_refused']+=1
            raise ValueError('Shape inference requires unavailable external weight VALUE: '+name+' at '+node.name)
        return stock_get(node,index)
    def try_value(node,index):
        if index<len(node.input) and node.input[index] in external_names:
            observed['external_optional_values_unknown']+=1
            return None
        return stock_try(node,index)
    engine._get_value=get_value;engine._try_get_value=try_value
    return observed
