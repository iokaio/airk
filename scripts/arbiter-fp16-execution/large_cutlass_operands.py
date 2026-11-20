# SPDX-License-Identifier: Apache-2.0
"""Build the Large native MHA operand control from real layer-17 Q/K/V and compute both CPU controls."""
import argparse
import hashlib
import json
import os
import runpy
import sys
import time
from pathlib import Path

HERE=Path(__file__).resolve().parent
ROOT=HERE.parents[1]
E=ROOT/"artifacts/v190/final-plan/exec-20261116T052942Z"
ATTENTION="27d5bb3a99d2288be5ce85584c6086a8d8e3dfa87f4217d38815aa5fd81db95a"
SOURCE="78752923dc8a50f7852015855398343cf62252371cc7defd46a836b9a568233f"
PLAN="d09ee0900ae7b13f251272170ef27d5de3181ea6b7ca16612f33e364a77ece0b"

def require(ok,msg):
    if not ok:
        raise ValueError(msg)

def pin(p):
    p=Path(p)
    with p.open("rb") as f:
        return {"path":str(p),"bytes":p.stat().st_size,"sha256":hashlib.file_digest(f,"sha256").hexdigest()}

def read(p,digest):
    require(pin(p)["sha256"]==digest,"changed "+str(p))
    return json.loads(Path(p).read_bytes())

def prepare(out):
    require(out.parent==E and out.is_dir() and sys.flags.isolated and sys.flags.no_site and sys.dont_write_bytecode
            and not any(out.iterdir()) and os.environ.get("CUDA_VISIBLE_DEVICES")=="","fresh owned CPU-only output required")
    inp=E/"windows-large-protected-qkv-attention17-t32-r1/execution.json"
    prior=read(inp,ATTENTION)
    require(prior["complete"] and prior["owned"]["exit_code"]==prior["owned"]["owned_job_final_active_processes"]==0,
            "actual isolated Large input capture required")
    native=read(prior["native"]["path"],prior["native"]["sha256"])
    plan=read(E/"large-fp16-conversion-plan-r5/large.json",PLAN)
    env=read(plan["runtime_manifest"]["path"],plan["runtime_manifest"]["sha256"])
    guard_path=ROOT/"scripts/arbiter-fp16-execution.py"
    require(pin(sys.executable)==plan["python"] and pin(guard_path)==plan["wrapper"],"original bootstrap required")
    owner=runpy.run_path(str(guard_path))
    ref=HERE.parent/"arbiter-fp16-reference/large_cutlass_reference.py"
    require(pin(ref)["sha256"]==SOURCE,"Large arithmetic differs")
    calc=runpy.run_path(str(ref))
    fast=calc["verified"](HERE/"compact_cutlass_reference.py",calc["FAST_SOURCE"])
    assets={name:out/name for name in ("attention.onnx","operands.fp16","reference.fp32")}
    end=time.monotonic()+120
    for p in assets.values():
        owner["guard"](plan,p,deadline=end)
    sys.addaudithook(lambda event,args:owner["output_access"](event,args,out))
    sys.path.extend(env["import_paths"])
    import numpy as np
    import onnx
    import torch
    torch.set_num_threads(2)
    torch.set_num_interop_threads(1)
    torch.set_float32_matmul_precision("highest")
    taps={r["name"]:r for r in native["layer17_taps"]}
    raw=[]
    words=[]
    for key,h in (("q",32),("k",8),("v",8)):
        r=taps[key]
        require((r["tokens"],r["width"],len(r["bits"]))==(32,128*h,32*128*h),"wrong original tensor dimensions")
        b=np.asarray(r["bits"],dtype="<u2")
        raw.append(b.tobytes())
        words.append(torch.from_numpy(b.view("<f2").reshape(1,32,h,128)).float().transpose(1,2).contiguous())
    q,k,v=words
    correct=calc["causal"](torch,q,k,v,[1]*32,lambda:None,fast["attention"])
    def exact(keys,values):
        logit=(q.double() @ keys.double().transpose(-1,-2))*(128**-0.5)
        logit.masked_fill_(torch.ones((32,32),dtype=torch.bool).triu(1),float("-inf"))
        return logit.softmax(-1) @ values.double()
    ideal=exact(k.repeat_interleave(4,dim=1),v.repeat_interleave(4,dim=1))
    wrong=exact(k.repeat(1,4,1,1),v.repeat(1,4,1,1))
    grouping_delta=float((wrong-ideal).abs().max())
    row0=(correct[:,:,0,:]-v.repeat_interleave(4,dim=1)[:,:,0,:]).abs().max().item()
    require(grouping_delta>0.01 and row0<1e-5 and torch.isfinite(correct).all(),"original operand fixture fails grouping/causality control")
    helper=onnx.helper
    def value(name):
        return helper.make_tensor_value_info(name,onnx.TensorProto.FLOAT,[1,"tokens",4096])
    node=helper.make_node("MultiHeadAttention",["q","k","v"],["attention"],name="airk_large_cutlass_t32_operand",
                          domain="com.microsoft",num_heads=32,unidirectional=1,scale=128**-0.5)
    g=helper.make_graph([node],"airk_large_cutlass_t32_operand",[value(k) for k in ("q","k","v")],[value("attention")])
    model=helper.make_model(g,ir_version=9,opset_imports=[helper.make_opsetid("",17),helper.make_opsetid("com.microsoft",1)])
    onnx.checker.check_model(model)
    for name,data in (("attention.onnx",model.SerializeToString()),("operands.fp16",b"".join(raw)),
                      ("reference.fp32",correct.transpose(1,2).contiguous().numpy().astype("<f4").tobytes())):
        with assets[name].open("xb") as f:f.write(data)
        owner["windows_owner"](path=assets[name])
    return {"schema":"airk.private.large-fp32-mha-original-operand-control/1","qualification":False,"source":pin(Path(__file__).resolve()),
            "reference_source":pin(ref),"original_capture":pin(inp),"original_native":prior["native"],
            "assets":{key:pin(p) for key,p in assets.items()},"shape":[1,32,32,128],"kv_heads":8,
            "compensated_vs_fp64_max":float((correct.double()-ideal).abs().max()),"invalid_alternating_order_max":grouping_delta,
            "first_causal_row_vs_original_value_max":row0}

if __name__=="__main__":
    p=argparse.ArgumentParser(description=__doc__)
    p.add_argument("output",type=Path)
    print(json.dumps(prepare(p.parse_args().output),allow_nan=False))
