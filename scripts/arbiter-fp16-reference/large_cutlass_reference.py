# SPDX-License-Identifier: Apache-2.0
"""Independent 32-query/four-to-one-head FP32 causal MHA reference for Large's unchanged 252 half-boundary products."""
import hashlib
import json
import runpy
from pathlib import Path

HERE=Path(__file__).resolve().parent
STAGE="e6a6227446662e33c47ea006e1c0f6e2710b6434245bb23f4205ace8299d794c"
GRAPH="a9badf775c6b4e2be5dd7ffa015c37d7aeace0c56ec943f55bc1be23e0854157"
FAST_SOURCE="9df92e6393079f61b6c2ccd13792afe243d4b3078fc2a6effc04c6651039e46f"
BODY_SOURCE="7506f90814ad7edcb8851efb5b1647153a7da04aa82f273b86732b6aabda6ed9"

def require(ok,message):
    if not ok:
        raise ValueError(message)

def verified(path,digest):
    require(hashlib.sha256(path.read_bytes()).hexdigest()==digest,"independent reference source changed")
    return runpy.run_path(str(path))

def stage(campaign,pin,check):
    path=campaign/"large-cutlass-body-stage-r1/stage.json"
    p=pin(path,check)
    require(p["sha256"]==STAGE,"Large FP32 MHA descriptor changed")
    spec=json.loads(path.read_bytes())
    v=spec["verification"]
    require(spec["graph"]["sha256"]==GRAPH and pin(path.with_name("model.onnx"),check)==spec["graph"]
            and (v["projection_count"],v["mha_count"],v["query_heads"],v["kv_heads"],v["group_repeat"],v["head_width"])
            ==(252,36,32,8,4,128) and v["byte_exact_body_inverse"] and spec["new_weight_bytes"]==0,
            "Large graph, original weights or grouped MHA geometry differs")
    return p,spec

def causal(torch,q,k,v,mask,check,attention):
    n=len(mask)
    require(q.shape==(1,32,n,128) and k.shape==v.shape==(1,8,n,128),"32 query and 8 KV heads required")
    # Attention factorizes independently by head. Each subgroup feeds two consecutive query
    # heads from every four-head Large group to the same eight original keys and values.
    qs=q.reshape(1,8,4,n,128)
    output=torch.empty_like(qs)
    for offset in (0,2):
        part=qs[:,:,offset:offset+2].reshape(1,16,n,128)
        output[:,:,offset:offset+2]=attention(torch,part,k,v,mask,check).reshape(1,8,2,n,128)
    return output.reshape(q.shape)

def score(model,row,check):
    torch=model.torch
    arithmetic=verified(HERE.parent/"arbiter-fp16-execution/compact_cutlass_reference.py",FAST_SOURCE)
    products=verified(HERE/"large_body_reference.py",BODY_SOURCE)
    ns=model.forward.__func__.__globals__
    old=ns["causal_half_attention"]
    calls=0
    routes=iter(f"layers.{layer}.{site}.weight" for layer in range(36) for site in products["ROUTES"])
    torch.set_float32_matmul_precision("highest")
    def product(value,name):
        weight=model.weights[name]
        require(name==next(routes,None) and value.device.type==weight.device.type=="cpu"
                and value.dtype==weight.dtype==torch.float16,"Large original half product order or boundary differs")
        check()
        return torch.nn.functional.linear(value.float(),weight.float()).half()
    def operands(hidden,position,root):
        normal=model._attention_norm(hidden,root)
        n=hidden.shape[1]
        q=model.linear(normal,root+"self_attn.q_proj.weight").reshape(1,n,32,128)
        k=model.linear(normal,root+"self_attn.k_proj.weight").reshape(1,n,8,128)
        v=model.linear(normal,root+"self_attn.v_proj.weight").reshape(1,n,8,128).transpose(1,2)
        q=ns["rms"](torch,q,model.weights[root+"self_attn.q_norm.weight"],model.eps).transpose(1,2)
        k=ns["rms"](torch,k,model.weights[root+"self_attn.k_norm.weight"],model.eps).transpose(1,2)
        q,k=ns["rotary"](torch,q,k,position,model.config["rope_theta"])
        return q,k,v
    def mha(t,q,k,v,mask,*,context_flash_single_tile):
        nonlocal calls
        calls+=1
        require(q.dtype==k.dtype==v.dtype==t.float32 and not context_flash_single_tile,"unexpected MHA entry")
        return causal(t,q,k,v,mask,check,arithmetic["attention"])
    model._linear_half=product
    model._attention_operands=operands
    ns["causal_half_attention"]=mha
    try:
        result=model.forward(row,context_flash_single_tile=False)
    finally:
        del model._linear_half
        del model._attention_operands
        ns["causal_half_attention"]=old
    require(calls==36 and next(routes,None) is None,"expected 252 Large protected products and 36 MHA calls")
    return result
