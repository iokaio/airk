# SPDX-License-Identifier: Apache-2.0
"""Independent CPU reference for compressed Compact learned weights and protected FP32 causal attention."""
import argparse
import hashlib
import json
import math
from pathlib import Path
import runpy
import sys
import time

HERE = Path(__file__).resolve().parent
FULL_SOURCE = "5b12a98cee8525323ac16076fe82e3b3895471c534cdc48947f76f9785813595"
GRAPH = "147be075a73ff81ca17b6c2fb965ec8f887e10908fbd33cf47a919d1694aa3d9"
RECIPE = {"learned_matrices":"original publisher state rounded to binary16 storage",
          "projections":"all 196 products with FP32 inputs, upcast half weights, and FP32 output",
          "attention":"FP32 Q/K after original RoPE; FP32 V; group each KV head twice; exact causal all-live support",
          "attention_output":"FP32; 32-query blocks, ascending 128-key tiles, tiled online exp2 softmax",
          "multiply":"big toward zero, residual half-ULP truncate; small-big + big-small + big-big; FP32 accumulation",
          "native_backend":"CU15 SM80 CUTLASS OpMultiplyAddFastF32; independently assessed against mathematical FP32"}
RECIPE_SHA = hashlib.sha256(json.dumps(RECIPE,sort_keys=True,separators=(",",":")).encode()).hexdigest()


def require(ok,message):
    if not ok:
        raise ValueError(message)


def split_fast_f32(torch, value):
    bits=value.contiguous().view(torch.int32)
    big=(bits & -8192).view(torch.float32)
    residual=(value-big).contiguous().view(torch.int32)
    small=((residual + 4096) & -8192).view(torch.float32)
    return big,small


def fast_f32_product(a_big,a_small,b_big,b_small):
    return ((a_small @ b_big) + (a_big @ b_small)) + (a_big @ b_big)


def attention(torch,q,k,v,mask,check):
    n=len(mask)
    require(0<n<=8192 and mask==[1]*n and q.shape==(1,16,n,128) and k.shape==v.shape==(1,8,n,128)
            and all(x.dtype==torch.float32 and x.device.type=="cpu" for x in (q,k,v)),"exact all-live FP32 operands required")
    positions=torch.arange(n)
    keys=k.repeat_interleave(2,dim=1)
    values=v.repeat_interleave(2,dim=1)
    # CU15 SM80 selects 32 query rows, ascending 128-key tiles and the 3xTF32 FastF32 operator.
    # The CPU BLAS summation order can differ from the MMA instruction's lane-level accumulation.
    kb,ks=split_fast_f32(torch,keys)
    vb,vs=split_fast_f32(torch,values)
    qb,qs=split_fast_f32(torch,q)
    out=torch.empty_like(q)
    scale=torch.tensor(128**-0.5,dtype=torch.float32)*torch.tensor(math.log2(math.e),dtype=torch.float32)
    for first in range(0,n,32):
        check()
        end=min(first+32,n)
        xbig,xsmall=qb[:,:,first:end],qs[:,:,first:end]
        highest=torch.full((*xbig.shape[:-1],1),-math.inf)
        total=torch.zeros_like(highest)
        value_sum=torch.zeros_like(xbig)
        for start in range(0,end,128):
            limit=min(start+128,end)
            big=kb[:,:,start:limit].transpose(-1,-2)
            small=ks[:,:,start:limit].transpose(-1,-2)
            logits=fast_f32_product(xbig,xsmall,big,small)*scale
            logits.masked_fill_(positions[start:limit].unsqueeze(0)>positions[first:end].unsqueeze(1),-math.inf)
            updated=torch.maximum(highest,logits.amax(-1,keepdim=True))
            rescale=(highest-updated).exp2()
            mass=(logits-updated).exp2()
            total=total*rescale+mass.sum(-1,keepdim=True)
            big,small=split_fast_f32(torch,mass)
            value_sum=value_sum*rescale+fast_f32_product(big,small,vb[:,:,start:limit],vs[:,:,start:limit])
            highest=updated
        out[:,:,first:end]=value_sum/total
    require(torch.isfinite(out).all().item(),"nonfinite attention reference")
    return out


def score(half,model,row,check):
    torch=model.torch
    calls=[]
    matrices=[]
    original=half.causal_half_attention
    def linear(value,name):
        check()
        weight=model.weights[name]
        require(value.dtype==torch.float32 and weight.dtype==torch.float16,"projection route or storage differs")
        matrices.append(name)
        return torch.nn.functional.linear(value,weight.float())
    def operands(hidden,position,root):
        normal=model._attention_norm(hidden,root)
        n=hidden.shape[1]
        q=linear(normal,root+"self_attn.q_proj.weight").reshape(1,n,16,128)
        k=linear(normal,root+"self_attn.k_proj.weight").reshape(1,n,8,128)
        v=linear(normal,root+"self_attn.v_proj.weight").reshape(1,n,8,128).transpose(1,2)
        q=half.rms(torch,q,model.weights[root+"self_attn.q_norm.weight"],model.eps).transpose(1,2)
        k=half.rms(torch,k,model.weights[root+"self_attn.k_norm.weight"],model.eps).transpose(1,2)
        q,k=half.rotary(torch,q,k,position,model.config["rope_theta"])
        return q,k,v
    def causal(torch,q,k,v,mask,*,context_flash_single_tile):
        require(not context_flash_single_tile,"wrong candidate attention mode")
        calls.append(len(mask))
        return attention(torch,q,k,v,mask,check)
    try:
        model.linear=linear
        model._attention_operands=operands
        half.causal_half_attention=causal
        scores=model.forward(row).flatten().tolist()
    finally:
        del model.linear
        del model._attention_operands
        half.causal_half_attention=original
    expected=[f"layers.{layer}.{name}.weight" for layer in range(28) for name in
              ("self_attn.q_proj","self_attn.k_proj","self_attn.v_proj","self_attn.o_proj","mlp.gate_proj","mlp.up_proj","mlp.down_proj")]
    require(matrices==expected and calls==[len(row["input_ids"])]*28 and all(map(math.isfinite,scores)),
            "original 196 projections, 28 attention calls and finite scores required")
    return scores


def bound_entry_pin(path, expected_sha, replay, check):
    require(path == (HERE / "compact_cutlass_bound_reference.py").resolve(), "unknown bound reference entry")
    entry = replay.file_pin(path, check)
    require(entry["sha256"] == expected_sha, "bound reference entry changed")
    return entry


def worker(args,base,replay,wider,*,bound_entry=False):
    def check():
        require(time.monotonic()<args.worker_deadline,"original CPU deadline expired")
    placement=None
    if args.cal_pid is not None:
        affinity=HERE.parent/"arbiter/analysis/development_margins/observe_cal_thread_affinity.py"
        require(replay.file_pin(affinity)["sha256"]=="7f37dcc3f9be734a964f561d9afc68b54ede19d036094b7fb7dab697f7d3f57f","affinity owner changed")
        placement=runpy.run_path(str(affinity))["place_optional_worker"](args.cal_pid,2)
        require(len(placement["logical_cpus"])==2,"two quiet physical cores required")
    legacy=replay.read_only_bootstrap()
    entry = None
    if bound_entry:
        main = sys.modules["__main__"]
        entry = bound_entry_pin(Path(main.__file__).resolve(), args.source_sha256, replay, check)
        legacy.loaded_runtime({entry["path"]: entry}, replay.file_pin, check, [main])
    case,census,capture,_,row=base.inputs(legacy,args.case,args.census_sha256,args.ordinal,check)
    stage,stage_pin=legacy.read_pinned_json(base.CAMPAIGN/"compact-cutlass-protected-staged-r1/stage.json",
        "4465463b434f6bb5e9798307c31e622dc3da4d67b6a5f6bf5793daf5c94beabf",check)
    require(stage["complete"] and stage["candidate_graph"]["sha256"]==GRAPH and stage["graph_proof"]["causal_fp32_mha"]==28,
            "wrong protected-attention stage identity")
    proof,source=legacy.read_pinned_json(base.CAMPAIGN/"compact-fp16-qkv-fp32-wider-reference-r1"/wider.WIDER_LEAF,
                                         base.owner.REFERENCE_SHA,check)
    runtime,runtime_pin=legacy.read_pinned_json(replay.RUNTIME,replay.RUNTIME_SHA,check)
    for pin in proof["publisher_files"].values():
        require(replay.file_pin(Path(pin["path"]),check)==pin,"original publisher weights changed")
    before=set(sys.modules)
    sys.path.extend(runtime["import_paths"])
    import torch
    torch.set_num_threads(2)
    torch.set_num_interop_threads(1)
    require(torch.get_num_threads()==2,"expected two CPU threads")
    half=base.owner.module("../arbiter-fp16-reference.py",legacy.REFERENCES[legacy.REFERENCE_OWNER])
    publisher=base.owner.module("../"+legacy.PUBLISHER_OWNER.removeprefix("scripts/"),legacy.REFERENCES[legacy.PUBLISHER_OWNER])
    half.freeze_reference_math(torch)
    # Exact one-key attention must return the corresponding V head; the second token averages both causal values.
    q=torch.zeros(1,16,2,128)
    k=torch.zeros(1,8,2,128)
    v=torch.arange(8,dtype=torch.float32).view(1,8,1,1).expand(1,8,2,128).clone()
    v[:,:,1]+=2.
    literal=attention(torch,q,k,v,[1,1],check)
    wanted=v.repeat_interleave(2,dim=1)
    wanted[:,:,1]-=1.
    require(torch.equal(literal,wanted),"original causal and adjacent grouped-head literal differs")
    original,config=publisher.load_authenticated(legacy.PUBLISHER_ROOT,"Qwen/Qwen3-1.7B")
    model=half.HalfReference(torch,config,original.text.state_dict(),False,"cpu")
    del original
    with torch.inference_mode():
        scores=score(half,model,row,check)
    require(not torch.cuda.is_initialized(),"independent reference initialized CUDA")
    pins=dict(runtime["files"])
    for item in proof["reference_sources"]:
        pins[item["path"]]=item
    for p in (Path(__file__).resolve(),HERE/"compact_full_projection_reference.py",HERE/"representative_compact_reference.py"):
        item=replay.file_pin(p,check)
        pins[item["path"]]=item
    if entry:
        require(replay.file_pin(Path(entry["path"]), check) == entry, "bound worker changed during scoring")
        pins[entry["path"]] = entry
    legacy.loaded_runtime(pins,replay.file_pin,check,[sys.modules[n] for n in sys.modules.keys()-before])
    return {"schema":"airk.private.compact-cutlass-publisher-reference/1","case":case,"census":census,"capture":capture,
            "compiled":row,"stage":stage_pin,"source":source,"runtime":runtime_pin,"recipe":RECIPE,"recipe_sha256":RECIPE_SHA,
            "candidate_graph_sha256":GRAPH,"matching_fp16":scores,"cpu_placement":placement,"threads":2,"qualification":False}


if __name__=="__main__":
    parser=argparse.ArgumentParser(description=__doc__)
    group=parser.add_mutually_exclusive_group(required=True)
    group.add_argument("--output",type=Path)
    group.add_argument("--worker-deadline",type=float)
    parser.add_argument("--case",required=True)
    parser.add_argument("--census-sha256",required=True)
    parser.add_argument("--ordinal",type=int,required=True)
    parser.add_argument("--source-sha256")
    parser.add_argument("--cal-pid",type=int)
    args=parser.parse_args()
    require(sys.platform=="win32" and sys.flags.isolated and sys.flags.no_site and sys.dont_write_bytecode,
            "checked Windows Python required")
    full=HERE/"compact_full_projection_reference.py"
    require(hashlib.sha256(full.read_bytes()).hexdigest()==FULL_SOURCE,"independent publisher reference owner changed")
    base=runpy.run_path(str(full))["load_base"]()
    replay=base.owner.module("replay_layer0.py",base.owner.REPLAY_SHA)
    wider=base.owner.module("verify_projection_candidate_t32.py",replay.WIDER_REFERENCE_SHA)
    if args.output:
        base.owner.run_owned(replay,wider,args.output,representative=(args.case,args.census_sha256,args.ordinal),
                             cutlass_projection=True,full_cuts_cal_pid=args.cal_pid)
    else:
        require(replay.file_pin(Path(__file__).resolve())["sha256"]==args.source_sha256
                and 0<args.worker_deadline-time.monotonic()<=600,"source or deadline changed")
        print(base.owner.MARKER+json.dumps(worker(args,base,replay,wider),allow_nan=False))
