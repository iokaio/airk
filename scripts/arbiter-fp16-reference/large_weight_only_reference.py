# SPDX-License-Identifier: Apache-2.0
"""Large reference with half-stored linear weights, direct FP32 activations/results, and 32-head compensated MHA."""
import hashlib
import json
from pathlib import Path
import runpy

HERE=Path(__file__).resolve().parent
STAGE="c2ac1951a3c0b5817380c1c6164b25e4e2f3b679ba569b727a7ebbbbf1ea9a35"
GRAPH="c1b5776144c6d9af07d08552cae40e491d8f866e89c53bab567adeeb32ca75e4"
MHA_SOURCE="78752923dc8a50f7852015855398343cf62252371cc7defd46a836b9a568233f"
FAST_SOURCE="9df92e6393079f61b6c2ccd13792afe243d4b3078fc2a6effc04c6651039e46f"
BODY_SOURCE="7506f90814ad7edcb8851efb5b1647153a7da04aa82f273b86732b6aabda6ed9"

def require(ok,message):
    if not ok:
        raise ValueError(message)

def stage(campaign,pin,check):
    path=campaign/"large-direct-fp32-stage-r2/stage.json"
    receipt=pin(path,check)
    require(receipt["sha256"]==STAGE,"original direct-FP32 stage changed")
    doc=json.loads(path.read_bytes())
    v=doc["verification"]
    require(doc["graph"]["sha256"]==GRAPH and pin(path.with_name("model.onnx"),check)==doc["graph"]
            and doc["new_weight_bytes"]==0 and v["byte_exact_mha_inverse"] and
            (v["direct_fp32_inputs"],v["direct_fp32_results"],v["removed_casts"],v["mha_count"],v["query_heads"],v["kv_heads"],v["head_width"])
            ==(252,252,1008,36,32,8,128),"unreviewed direct-FP32 math or Large grouping")
    return receipt,doc

def score(model,row,check):
    torch=model.torch
    old_mha=HERE/"large_cutlass_reference.py"
    require(hashlib.sha256(old_mha.read_bytes()).hexdigest()==MHA_SOURCE,"Large grouped arithmetic changed")
    shared=runpy.run_path(str(old_mha))
    arithmetic=shared["verified"](HERE.parent/"arbiter-fp16-execution/compact_cutlass_reference.py",FAST_SOURCE)
    routes=shared["verified"](HERE/"large_body_reference.py",BODY_SOURCE)["ROUTES"]
    ordered=iter(f"layers.{i}.{site}.weight" for i in range(36) for site in routes)
    ns=model.forward.__func__.__globals__
    saved=ns["causal_half_attention"]
    calls=0
    torch.set_float32_matmul_precision("highest")
    def linear(value,name):
        check()
        w=model.weights[name]
        require(name==next(ordered,None) and value.device.type==w.device.type=="cpu"
                and value.dtype==torch.float32 and w.dtype==torch.float16,"wrong original weight, activation precision or order")
        return torch.nn.functional.linear(value,w.float())
    def operands(hidden,position,root):
        normal=model._attention_norm(hidden,root)
        n=hidden.shape[1]
        q=linear(normal,root+"self_attn.q_proj.weight").reshape(1,n,32,128)
        k=linear(normal,root+"self_attn.k_proj.weight").reshape(1,n,8,128)
        v=linear(normal,root+"self_attn.v_proj.weight").reshape(1,n,8,128).transpose(1,2)
        q=ns["rms"](torch,q,model.weights[root+"self_attn.q_norm.weight"],model.eps).transpose(1,2)
        k=ns["rms"](torch,k,model.weights[root+"self_attn.k_norm.weight"],model.eps).transpose(1,2)
        q,k=ns["rotary"](torch,q,k,position,model.config["rope_theta"])
        return q,k,v
    def attn(t,q,k,v,mask,*,context_flash_single_tile):
        nonlocal calls
        require(not context_flash_single_tile and q.dtype==k.dtype==v.dtype==t.float32,"unexpected Large attention boundary")
        calls+=1
        return shared["causal"](t,q,k,v,mask,check,arithmetic["attention"])
    model.linear=linear
    model._attention_operands=operands
    ns["causal_half_attention"]=attn
    try:
        result=model.forward(row)
    finally:
        ns["causal_half_attention"]=saved
        del model._attention_operands
        del model.linear
    require(calls==36 and next(ordered,None) is None,"incomplete original Large model")
    return result
