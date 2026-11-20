# SPDX-License-Identifier: Apache-2.0
"""Independent Compact down-projection sensitivity and matching candidate reference."""
import argparse
import hashlib
import importlib.util
import json
import math
from pathlib import Path
import struct
import sys
import time


HERE = Path(__file__).resolve().parent
BASE_SOURCE = "e574ed3d2b7dd3f736891bcf5e40c2eb45edc43f1fbb73bec902c330aded83e1"
BASE_REFERENCE = "89b5a289b66ce93ac314c583cadff2112fda185081152d3f6a7886bae8b8d169"
BASE_NATIVE = "1580381c761b10c56b80db0bb62bb0bd33d50c28c1eeaa62027520b0ec45474f"
ROUTES = tuple(f"layers.{i}.mlp.down_proj.weight" for i in range(28))
RECIPE = {"baseline": "Compact Q/K/V FP32 MatMul with preserved half output",
          "routes": ROUTES, "weights": "unchanged FP16 sidecar, exact upcast to FP32",
          "activation": "rounded to FP16 then exactly upcast to FP32",
          "matmul": "FP32 without TF32", "output": "FP32 direct to residual addition"}


def require(ok, message):
    if not ok:
        raise ValueError(message)


def load_base():
    path = HERE / "representative_compact_reference.py"
    require(hashlib.sha256(path.read_bytes()).hexdigest() == BASE_SOURCE, "baseline matching reference changed")
    spec = importlib.util.spec_from_file_location("sealed_compact_reference", path)
    base = importlib.util.module_from_spec(spec)
    spec.loader.exec_module(base)
    return base


def pair(current, previous):
    require(len(current) == len(previous) and 2 <= len(current) <= 8, "score cardinality differs")
    delta = [x-y for x, y in zip(current, previous, strict=True)]
    return {"per_card": delta, "maximum_abs": max(map(abs, delta))}


def variant(base, half, wider, model, row, check, routes):
    torch = model.torch
    ordinary = model._linear_half
    visited = []
    witnesses = []
    start = time.monotonic()

    def routed(value, name):
        if name not in routes:
            return ordinary(value, name)
        check()
        visited.append(name)
        weight = model.weights[name]
        require(weight.dtype == value.dtype == torch.float16
                and tuple(weight.shape) == (model.hidden, model.intermediate)
                and value.shape[-1] == model.intermediate, "MLP down operands differ")
        result = torch.nn.functional.linear(value.float(), weight.float())
        if name == ROUTES[-1]:
            decision = row["decision_position"]
            # Evaluate both paths on exactly the same complete T-by-intermediate operands.
            baseline = ordinary(value, name)[0, decision].float()
            protected = result[0, decision]
            difference = (protected-baseline).abs()
            out = int(difference.argmax())
            x = value[0, decision].tolist()
            w = weight[out].tolist()
            # Half products are exact in binary64. Exact witnesses also use the common
            # 2^-24 half unit, so the full contraction is summed as Python integers.
            exact_units = sum(int(a*(1<<24))*int(b*(1<<24)) for a,b in zip(x,w,strict=True))
            exact = math.ldexp(exact_units, -48)
            witnesses.append({"route": name, "output_channel": out, "input_width": len(x),
                              "input_sha256": half.tensor_digest(value[0,decision]),
                              "weight_row_sha256": half.tensor_digest(weight[out]),
                              "exact_units_times_2_pow_minus48": str(exact_units),
                              "exact": exact, "ordinary": float(baseline[out]), "protected": float(protected[out]),
                              "max_abs_on_same_operands": float(difference.max()),
                              "l2_on_same_operands": float(difference.norm())})
        return result

    model._linear_half = routed
    try:
        scores = base.matching(half, wider, model, row, check)
    finally:
        if "_linear_half" in vars(model):
            del model._linear_half
    require(tuple(visited) == tuple(routes) and len(witnesses) == int(bool(routes)), "wrong measured MLP routes")
    return {"scores": scores, "down_routes": visited, "decision_witnesses": witnesses,
            "cpu_seconds": time.monotonic()-start}


def worker(args, base, replay, wider):
    def check():
        require(time.monotonic() < args.worker_deadline, "original CPU deadline expired")
    owner = base.owner
    legacy = replay.read_only_bootstrap()
    case, census, capture, _, row = base.inputs(legacy,args.case,args.census_sha256,args.ordinal,check)
    require(case == "ENGINEERING_FIXED", "first controlled attribution requires the frozen engineering input")
    cp, cpu_pin = legacy.read_pinned_json(owner.CAMPAIGN/"compact-development-engineering-publisher-reference-r1"/"execution.json",BASE_REFERENCE,check)
    native, native_pin = legacy.read_pinned_json(owner.CAMPAIGN/"compact-fp16-qkv-observed-engineering-r1"/"execution.json",BASE_NATIVE,check)
    source, native_scores_pin = legacy.read_pinned_json(Path(native["outputs"]["native"]["path"]),native["outputs"]["native"]["sha256"],check)
    require(cp["complete"] and native["complete"] and source["complete"]
            and row == cp["result"]["compiled"] == source["rows"][0]["compiled"]
            and cp["result"]["capture"] == capture, "original baseline proofs differ")
    proof, proof_pin = legacy.read_pinned_json(owner.CAMPAIGN/"compact-fp16-qkv-fp32-wider-reference-r1"/wider.WIDER_LEAF,owner.REFERENCE_SHA,check)
    runtime, runtime_pin = legacy.read_pinned_json(replay.RUNTIME,replay.RUNTIME_SHA,check)
    for pin in proof["publisher_files"].values():
        require(replay.file_pin(Path(pin["path"]),check) == pin,"publisher source changed")
    before = set(sys.modules)
    sys.path.extend(runtime["import_paths"])
    import torch
    torch.set_num_threads(wider.WIDER_CPU_THREADS)
    torch.set_num_interop_threads(1)
    half = owner.module("../arbiter-fp16-reference.py",legacy.REFERENCES[legacy.REFERENCE_OWNER])
    publisher = owner.module("../"+legacy.PUBLISHER_OWNER.removeprefix("scripts/"),legacy.REFERENCES[legacy.PUBLISHER_OWNER])
    half.freeze_reference_math(torch)
    original, config = publisher.load_authenticated(legacy.PUBLISHER_ROOT,"Qwen/Qwen3-1.7B")
    model = half.HalfReference(torch,config,original.text.state_dict(),False,"cpu")
    del original
    with torch.inference_mode():
        ordinary = variant(base,half,wider,model,row,check,())
        n = len(row["candidate_positions"])
        require(struct.pack(f"<{n}f",*ordinary["scores"]) == struct.pack(f"<{n}f",*cp["result"]["matching_fp16"]),
                "complete original matching reference does not reproduce")
        last = variant(base,half,wider,model,row,check,ROUTES[-1:])
        all_down = variant(base,half,wider,model,row,check,ROUTES)
    check()
    require(not torch.cuda.is_initialized(),"CPU attribution initialized CUDA")
    pins = dict(runtime["files"])
    for pin in proof["reference_sources"]+[replay.file_pin(Path(__file__)),replay.file_pin(HERE/"representative_compact_reference.py")]:
        pins[pin["path"]] = pin
    legacy.loaded_runtime(pins,replay.file_pin,check,[sys.modules[n] for n in sys.modules.keys()-before])
    return {"schema":"airk.private.compact-mlp-down-projection-reference/1","recipe":RECIPE,
            "recipe_sha256":hashlib.sha256(json.dumps(RECIPE,sort_keys=True,separators=(",",":")).encode()).hexdigest(),
            "original_native":native_pin,"original_scores":native_scores_pin,"original_cpu":cpu_pin,
            "baseline_basis":proof_pin,"runtime":runtime_pin,"capture":capture,"census":census,
            "compiled":row,"case":case,"roster":cp["result"]["roster"],"original_fp32":cp["result"]["original_fp32"],
            "baseline":ordinary,"last_down_fp32":last,"all_down_fp32":all_down,
            "last_only_change":pair(last["scores"],ordinary["scores"]),
            "all_down_change":pair(all_down["scores"],ordinary["scores"]),
            "baseline_native_error":pair(source["rows"][0]["raw_scores"],ordinary["scores"]),
            "native_candidate_scoring":False,"qualification":False}


if __name__ == "__main__":
    parser = argparse.ArgumentParser(description=__doc__)
    group = parser.add_mutually_exclusive_group(required=True)
    group.add_argument("--output",type=Path)
    group.add_argument("--worker-deadline",type=float)
    parser.add_argument("--source-sha256")
    parser.add_argument("--case",required=True)
    parser.add_argument("--census-sha256",required=True)
    parser.add_argument("--ordinal",type=int,required=True)
    args=parser.parse_args()
    require(sys.platform=="win32" and sys.flags.isolated and sys.flags.no_site and sys.dont_write_bytecode,"owned Windows Python required")
    base=load_base()
    replay=base.owner.module("replay_layer0.py",base.owner.REPLAY_SHA)
    wider=base.owner.module("verify_projection_candidate_t32.py",replay.WIDER_REFERENCE_SHA)
    if args.output:
        base.owner.run_owned(replay,wider,args.output,representative=(args.case,args.census_sha256,args.ordinal),down_projection=True)
    else:
        require(replay.file_pin(Path(__file__).resolve())["sha256"]==args.source_sha256
                and 0<args.worker_deadline-time.monotonic()<=600,"worker source or deadline differs")
        print(base.owner.MARKER+json.dumps(worker(args,base,replay,wider),allow_nan=False))
