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