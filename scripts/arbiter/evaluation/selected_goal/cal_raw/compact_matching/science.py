# SPDX-License-Identifier: Apache-2.0
"""Compute independent full-card Compact CPU references under the original compiled budget."""
import math
import os
from pathlib import Path
import runpy
import sys
import time

from inputs import (CAMPAIGN, FULL_SHA, HERE, MATH_SHA, ORIGINAL, SCIENCE, STAGE, STAGE_SHA, SUFFIX_SHA,
                    digest, original_control, require, row_key)


def owners():
    for name, sha in (("compact_cutlass_reference.py",MATH_SHA),("compact_full_projection_reference.py",FULL_SHA),
                      ("replay_compact_t1024_suffix.py",SUFFIX_SHA)):
        require(digest(SCIENCE/name) == sha,"protected reference source changed: "+name)
    full = runpy.run_path(str(SCIENCE/"compact_full_projection_reference.py"))
    protected = runpy.run_path(str(SCIENCE/"compact_cutlass_reference.py"))
    base = full["load_base"]()
    replay = base.owner.module("replay_layer0.py",base.owner.REPLAY_SHA)
    wider = base.owner.module("verify_projection_candidate_t32.py",replay.WIDER_REFERENCE_SHA)
    return protected,base,replay,wider


def score_batch(rows,mode,role,deadline,cal_pid):
    def check():
        require(time.monotonic() < deadline,"original CPU reference Job deadline expired")
    protected,base,replay,wider = owners()
    affinity = HERE.parents[3]/"analysis/development_margins/observe_cal_thread_affinity.py"
    affinity_sha = "7f37dcc3f9be734a964f561d9afc68b54ede19d036094b7fb7dab697f7d3f57f"
    require(digest(affinity) == affinity_sha,"physical-core sampler changed")
    placement = runpy.run_path(str(affinity))["place_optional_worker"](cal_pid or os.getpid(),2)
    legacy = replay.read_only_bootstrap()
    proof,basis = legacy.read_pinned_json(CAMPAIGN/"compact-fp16-qkv-fp32-wider-reference-r1"/wider.WIDER_LEAF,
                                          base.owner.REFERENCE_SHA,check)
    stage,stage_pin = legacy.read_pinned_json(STAGE,STAGE_SHA,check)
    require(stage["complete"] is True and stage["candidate_graph"]["sha256"] == protected["GRAPH"] and
            stage["graph_proof"]["causal_fp32_mha"] == 28 and stage["graph_proof"]["fp32_projection_count"] == 196,
            "protected candidate stage differs")
    for key in ("candidate_graph","candidate_weights"):
        require(replay.file_pin(Path(stage[key]["path"]),check) == stage[key],"protected candidate file changed")
    runtime,runtime_pin = legacy.read_pinned_json(replay.RUNTIME,replay.RUNTIME_SHA,check)
    for ref in proof["publisher_files"].values():
        require(replay.file_pin(Path(ref["path"]),check) == ref,"publisher file changed")
    sources = [replay.file_pin(path,check) for path in (
        Path(__file__).resolve(),HERE/"inputs.py",HERE/"run.py",SCIENCE/"compact_cutlass_reference.py",
        SCIENCE/"compact_full_projection_reference.py",SCIENCE/"representative_compact_reference.py",
        SCIENCE/"replay_compact_t1024_suffix.py",Path(affinity))]
    header = {"mode":mode,"role":role,"sources":sources,"original_manifest":ORIGINAL,
              "publisher_files":proof["publisher_files"],"runtime":runtime_pin,"reference_basis":basis,
              "candidate_graph":stage["candidate_graph"],"candidate_weights":stage["candidate_weights"],
              "protected_stage":stage_pin,"matching_recipe_sha256":protected["RECIPE_SHA"],"cpu_placement":placement,
              "candidate_forwards":0,"qualification":False}
    if not any(row["status"] == "eligible" for row in rows):
        return header | {"rows":[row | {"input_row_sha256":row_key(row)} for row in rows]}
    before = set(sys.modules)
    sys.path.extend(runtime["import_paths"])
    import torch
    torch.set_num_threads(2)
    torch.set_num_interop_threads(1)
    require(torch.get_num_threads() == 2 and len(placement["logical_cpus"]) == 2,"two quiet CPU cores required")
    half = base.owner.module("../arbiter-fp16-reference.py",legacy.REFERENCES[legacy.REFERENCE_OWNER])
    publisher = base.owner.module("../"+legacy.PUBLISHER_OWNER.removeprefix("scripts/"),legacy.REFERENCES[legacy.PUBLISHER_OWNER])
    half.freeze_reference_math(torch)
    original,config = publisher.load_authenticated(legacy.PUBLISHER_ROOT,"Qwen/Qwen3-1.7B")
    model = None
    if role == "protected_matching":
        model = half.HalfReference(torch,config,original.text.state_dict(),False,"cpu")
        del original
        q = torch.zeros(1,16,2,128)
        k = torch.zeros(1,8,2,128)
        v = torch.arange(8,dtype=torch.float32).view(1,8,1,1).expand(1,8,2,128).clone()
        v[:,:,1] += 2.
        expected = v.repeat_interleave(2,dim=1)
        expected[:,:,1] -= 1.
        require(torch.equal(protected["attention"](torch,q,k,v,[1,1],check),expected),
                "protected causal/grouped-head reference regression")
    else:
        require(role == "publisher_fp32","unknown independent CPU role")
    result = []
    with torch.inference_mode():
        for row in rows:
            out = dict(row)
            out["input_row_sha256"] = row_key(row)
            result.append(out)
            if row["status"] != "eligible":
                continue
            started = time.monotonic()
            check()
            if role == "publisher_fp32":
                scores = base.publisher_scores(torch,original,row["compiled"])
            else:
                scores = protected["score"](half,model,row["compiled"],check)
            check()

            require(len(scores) == len(row["roster"]["cards"]) and all(math.isfinite(x) for x in scores),
                    "incomplete or nonfinite independent output")
            if mode == "r4_engineering_only" and role == "publisher_fp32":
                _,known = original_control()
                require(max(abs(a-b) for a,b in zip(scores,known["original_fp32"],strict=True)) <= .001,
                        "R4 publisher T1408 control differs")
            out.update(scores=scores,seconds=time.monotonic()-started)
    require(not torch.cuda.is_initialized(),"CPU-only independent reference initialized CUDA")
    pins = dict(runtime["files"])
    for ref in proof["reference_sources"]:
        pins[ref["path"]] = ref
    for ref in sources:
        pins[ref["path"]] = ref
    legacy.loaded_runtime(pins,replay.file_pin,check,[sys.modules[n] for n in sys.modules.keys()-before])
    return header | {"rows":result}
