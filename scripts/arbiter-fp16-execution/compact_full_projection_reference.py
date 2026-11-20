# SPDX-License-Identifier: Apache-2.0
"""Independent Compact projection bound with unchanged storage and efficient-attention ABI."""
import argparse
import hashlib
import importlib.util
import json
import math
from pathlib import Path
import sys
import time


HERE = Path(__file__).resolve().parent
BASE_SHA = "e574ed3d2b7dd3f736891bcf5e40c2eb45edc43f1fbb73bec902c330aded83e1"
CPU_REFERENCE = "89b5a289b66ce93ac314c583cadff2112fda185081152d3f6a7886bae8b8d169"
SITES = ("self_attn.q_proj", "self_attn.k_proj", "self_attn.v_proj", "self_attn.o_proj",
         "mlp.gate_proj", "mlp.up_proj", "mlp.down_proj")
ROUTES = tuple(f"layers.{layer}.{site}.weight" for layer in range(28) for site in SITES)
RECIPE = {"routes":ROUTES,"stored_projection_weights":"FP16 unchanged",
          "dense_input":"FP32 without intermediate half narrowing","dense_weight":"exact FP32 upcast of stored FP16",
          "dense_matmul":"FP32 without TF32","dense_output":"FP32",
          "efficient_attention":"only Q,K,V and attention output narrow to FP16; original GQA Flash",
          "reference_attention":"source-derived reverse 64-key tiles, half exponents for PV, FP32 accumulator; CPU approximation"}


def require(ok, message):
    if not ok:
        raise ValueError(message)


def load_base():
    path=HERE/"representative_compact_reference.py"
    require(hashlib.sha256(path.read_bytes()).hexdigest()==BASE_SHA,"sealed baseline reference changed")
    spec=importlib.util.spec_from_file_location("full_projection_base",path)
    module=importlib.util.module_from_spec(spec)
    spec.loader.exec_module(module)
    return module


def run_case(base,half,wider,model,row,check,mode):
    torch=model.torch
    trace={}
    weights=[]
    attention_calls=[]
    old_attention=half.causal_half_attention
    old_norm=model._attention_norm
    scale=half.f32(half.f32(128**-.5)*math.log2(math.e))
    decision=row["decision_position"]

    def capture(hidden,root):
        trace[root]=hidden[0,decision].clone()
        return old_norm(hidden,root)

    def linear(value,name):
        check()
        w=model.weights[name]
        require(value.dtype==torch.float32 and w.dtype==torch.float16,"full projection dtype or storage differs")
        weights.append(name)
        return torch.nn.functional.linear(value,w.float())

    def attention(torch,q,k,v,mask,*,context_flash_single_tile):
        require(q.dtype==k.dtype==v.dtype==torch.float16 and not context_flash_single_tile,"Flash boundary differs")
        attention_calls.append(len(mask))
        if mode=="dense_attention_control":
            return old_attention(torch,q,k,v,mask,context_flash_single_tile=False)
        return base.ragged_flash(torch,wider,q,k,v,mask,scale,check)

    model._attention_norm=capture
    start=time.monotonic()
    try:
        if mode=="sealed_qkv":
            scores=base.matching(half,wider,model,row,check)
        else:
            model.linear=linear
            half.causal_half_attention=attention
            raw=model.forward(row,context_flash_single_tile=False)
            require(raw.shape==(1,len(row["candidate_positions"])) and torch.isfinite(raw).all().item(),"invalid projection-bound scores")
            scores=raw[0].tolist()
    finally:
        del model._attention_norm
        if "linear" in vars(model):
            del model.linear
        half.causal_half_attention=old_attention
    require(list(trace)==[f"layers.{n}." for n in range(28)],"missing pre-block decision residual")
    if mode!="sealed_qkv":
        require(tuple(weights)==ROUTES and attention_calls==[len(row["input_ids"])]*28,"full-projection route census differs")
    return scores,trace,time.monotonic()-start


def compare_trace(a,b):
    rows=[]
    for layer in range(28):
        name=f"layers.{layer}."
        actual,reference=a[name].double(),b[name].double()
        error=actual-reference
        rows.append({"layer":layer,"maximum_abs":float(error.abs().max()),
                     "relative_l2":float(error.norm()/reference.norm()),"reference_l2":float(reference.norm())})
    return rows


def worker(args,base,replay,wider):
    def check():
        require(time.monotonic()<args.worker_deadline,"original CPU deadline expired")
    legacy=replay.read_only_bootstrap()
    case,census,capture,_,row=base.inputs(legacy,args.case,args.census_sha256,args.ordinal,check)
    require(case=="ENGINEERING_FIXED","exact original engineering case required")
    previous,cpu_pin=legacy.read_pinned_json(base.CAMPAIGN/"compact-development-engineering-publisher-reference-r1"/"execution.json",CPU_REFERENCE,check)
    require(previous["complete"] and previous["result"]["compiled"]==row and previous["result"]["capture"]==capture,
            "original CPU reference input differs")
    sealed,seal_pin=legacy.read_pinned_json(base.CAMPAIGN/"compact-fp16-qkv-fp32-wider-reference-r1"/wider.WIDER_LEAF,base.owner.REFERENCE_SHA,check)
    runtime,runtime_pin=legacy.read_pinned_json(replay.RUNTIME,replay.RUNTIME_SHA,check)
    for pin in sealed["publisher_files"].values():
        require(replay.file_pin(Path(pin["path"]),check)==pin,"publisher input changed")
    before=set(sys.modules)
    sys.path.extend(runtime["import_paths"])
    import torch
    torch.set_num_threads(wider.WIDER_CPU_THREADS)
    torch.set_num_interop_threads(1)
    half=base.owner.module("../arbiter-fp16-reference.py",legacy.REFERENCES[legacy.REFERENCE_OWNER])
    publisher=base.owner.module("../"+legacy.PUBLISHER_OWNER.removeprefix("scripts/"),legacy.REFERENCES[legacy.PUBLISHER_OWNER])
    half.freeze_reference_math(torch)
    original,config=publisher.load_authenticated(legacy.PUBLISHER_ROOT,"Qwen/Qwen3-1.7B")
    model=half.HalfReference(torch,config,original.text.state_dict(),False,"cpu")
    del original
    with torch.inference_mode():
        original,trace_original,t_original=run_case(base,half,wider,model,row,check,"sealed_qkv")
        require(original==previous["result"]["matching_fp16"],"historical reference score words changed")
        candidate,trace_candidate,t_candidate=run_case(base,half,wider,model,row,check,"full_projection")
        dense,trace_dense,t_dense=run_case(base,half,wider,model,row,check,"dense_attention_control")
    require(not torch.cuda.is_initialized(),"CPU control unexpectedly initialized CUDA")
    check()
    pins=dict(runtime["files"])
    for pin in sealed["reference_sources"]+[replay.file_pin(Path(__file__).resolve()),replay.file_pin(HERE/"representative_compact_reference.py")]:
        pins[pin["path"]]=pin
    legacy.loaded_runtime(pins,replay.file_pin,check,[sys.modules[n] for n in sys.modules.keys()-before])
    return {"schema":"airk.private.compact-full-projection-reference/1","case":case,"census":census,"capture":capture,
            "baseline_basis":seal_pin,"runtime":runtime_pin,"original_cpu":cpu_pin,"compiled":row,
            "roster":previous["result"]["roster"],"original_fp32":previous["result"]["original_fp32"],
            "recipe":RECIPE,"recipe_sha256":hashlib.sha256(json.dumps(RECIPE,sort_keys=True,separators=(",",":")).encode()).hexdigest(),
            "sealed_qkv":original,"matching_fp16":candidate,"dense_attention_control":dense,
            "dense_minus_reverse_per_card":[x-y for x,y in zip(dense,candidate,strict=True)],
            "residual_site":"FP32 pre-block decision row","projection_change":compare_trace(trace_candidate,trace_original),
            "attention_control_change":compare_trace(trace_dense,trace_candidate),
            "cpu_elapsed_seconds":{"baseline":t_original,"candidate":t_candidate,"dense_control":t_dense},
            "native_candidate_scoring":False,"qualification":False}


if __name__=="__main__":
    parser=argparse.ArgumentParser(description=__doc__)
    group=parser.add_mutually_exclusive_group(required=True)
    group.add_argument("--output",type=Path)
    group.add_argument("--worker-deadline",type=float)
    parser.add_argument("--source-sha256")
    parser.add_argument("--case",required=True)
    parser.add_argument("--census-sha256",required=True)
    parser.add_argument("--ordinal",type=int,required=True)
    args=parser.parse_args()
    require(sys.platform=="win32" and sys.flags.isolated and sys.flags.no_site and sys.dont_write_bytecode,
            "owned Windows Python required")
    base=load_base()
    replay=base.owner.module("replay_layer0.py",base.owner.REPLAY_SHA)
    wider=base.owner.module("verify_projection_candidate_t32.py",replay.WIDER_REFERENCE_SHA)
    if args.output:
        base.owner.run_owned(replay,wider,args.output,representative=(args.case,args.census_sha256,args.ordinal),full_projection=True)
    else:
        require(replay.file_pin(Path(__file__).resolve())["sha256"]==args.source_sha256
                and 0<args.worker_deadline-time.monotonic()<=600,"unexpected worker, source or deadline")
        print(base.owner.MARKER+json.dumps(worker(args,base,replay,wider),allow_nan=False))
