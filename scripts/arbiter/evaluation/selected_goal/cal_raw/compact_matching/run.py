# SPDX-License-Identifier: Apache-2.0
"""Own a guarded CPU publisher or CUTLASS-matching batch for retained original cards."""
import argparse
import json
import os
from pathlib import Path
import sys
import time

sys.path.insert(0,str(Path(__file__).resolve().parent))
from inputs import (CAMPAIGN, CONTROL, CONTROL_SHA, HERE, RAW, SCIENCE, checked, digest, original_control,
                    ref, require, row_key, selected)
from phase_reference import ROOT
from science import owners, score_batch

MARKER = "AIRK_COMPACT_CAL_REFERENCE "
LIMIT = 22 << 30


def source_set():
    return {str(p):digest(p) for p in (Path(__file__).resolve(),HERE/"inputs.py",HERE/"science.py",
            RAW/"original_reference.py",RAW/"phase_reference.py",RAW/"r5_chronological_inputs.py",
            SCIENCE/"compact_cutlass_reference.py",SCIENCE/"compact_full_projection_reference.py",
            SCIENCE/"representative_compact_reference.py",SCIENCE/"replay_compact_t1024_suffix.py")}


def verify_result(result,rows,mode,role):
    import math
    require(result["mode"] == mode and result["role"] == role and result["candidate_forwards"] == 0 and
            len(result["rows"]) == len(rows) and result["matching_recipe_sha256"] ==
            "7efd4fc72f11de95131c532467803a6a2c82c71cf84ac0bd23cabbcf3662e394",
            "reference family, population or protected arithmetic differs")
    for actual,original in zip(result["rows"],rows,strict=True):
        require(all(actual[k] == v for k,v in original.items()) and actual["input_row_sha256"] == row_key(original),
                "original ordinal, tensor, scope, allowance or card order changed")
        if original["status"] == "eligible":
            require(len(actual["scores"]) == len(original["roster"]["cards"]) and
                    all(type(x) in (int,float) and math.isfinite(x) for x in actual["scores"]) and
                    0 < actual["seconds"] <= 600,"invalid full-vector or oracle duration")
        else:
            require("scores" not in actual and "seconds" not in actual,"refused original case received scores")


def owned(args,input_ref,rows,mode):
    _,base,replay,wider = owners()
    legacy,_ = wider.load_base()
    deadline = time.monotonic()+legacy.MAX_WALL_SECONDS
    def check():
        require(time.monotonic() < deadline,"original scientific Job time expired")
    plan,_ = legacy.read_pinned_json(wider.BASE_ARGUMENTS["conversion_plan"],wider.BASE_ARGUMENTS["conversion_plan_sha256"],check)
    wrapper = Path(plan["wrapper"]["path"])
    if replay.file_pin(wrapper) != plan["wrapper"]:
        wrapper = CAMPAIGN/"compact-conversion-owner-311d13cc"/"arbiter-fp16-execution.py"
    require(digest(wrapper) == plan["wrapper"]["sha256"],"conversion or Windows guard changed")
    conversion = legacy.load_module(wrapper,"compact_cal_cpu_guard")
    token = conversion.windows_owner()
    require(replay.file_pin(Path(sys.executable)) == plan["python"] and token["owner_sid"] == plan["human_sid"],
            "unexpected Python executable or Windows owner")
    helper = ROOT/"scripts/cuda-native-execution/helpers"
    for leaf,sha in conversion.HELPERS.items():
        require(digest(helper/leaf) == sha,"suspended owner helper changed")
    sys.path.insert(0,str(helper))
    parent = legacy.load_module(helper/"reference_parent_no_site_r2.py","compact_cal_cpu_parent")
    job = legacy.load_module(helper/"reference_owned.py","compact_cal_cpu_job")
    out = args.output
    require(out.is_absolute() and out == out.resolve() and out.parent == CAMPAIGN and not out.exists(),
            "fresh original campaign directory required")
    cache = out/"temp"
    for path in (out,cache):
        conversion.guard(plan,path,deadline=deadline)
        path.mkdir()
        conversion.guard(plan,path,True,deadline=deadline)
    log,execution = out/"worker.log",out/"execution.json"
    for path in (log,execution):
        conversion.guard(plan,path,deadline=deadline)
    env = {k:v for k,v in os.environ.items() if not k.upper().startswith(("AIRK_","ORT_","PYTHON","CUDA_","CUDNN_","TORCH_","HF_","TRITON_"))}
    env.update(CUDA_VISIBLE_DEVICES="",HF_HUB_OFFLINE="1",TRANSFORMERS_OFFLINE="1",TOKENIZERS_PARALLELISM="false",
               TORCH_HOME=str(cache),TORCHINDUCTOR_CACHE_DIR=str(cache),TRITON_CACHE_DIR=str(cache),
               TEMP=str(cache),TMP=str(cache),TMPDIR=str(cache))
    sources = source_set()
    script = Path(__file__).resolve()
    argv = [sys.executable,"-I","-B","-S","-X","pycache_prefix="+plan["cache_prefix"],str(script),
            "--input",input_ref["path"],"--input-sha256",input_ref["sha256"],"--first",str(args.first),
            "--count",str(args.count),"--role",args.role,"--worker-deadline",repr(deadline),
            "--source-set",json.dumps(sources,sort_keys=True,separators=(",",":"))]
    if args.cal_pid:
        argv.extend(["--cal-pid",str(args.cal_pid)])
    def suspended(api,handle,info,child_deadline):
        evidence = parent.suspended_authentication(api,info,deadline,child_deadline,conversion.SID,plan["python"],job.image_proof)
        require(conversion.windows_owner(process=info.process) == token and job.job_snapshot(api,handle) == ([info.pid],1,1),
                "unverified suspended process or Job")
        return evidence
    with open(os.devnull,"rb") as stdin,log.open("xb") as stream:
        conversion.guard(plan,log,True,deadline=deadline)
        observation = job.run_windows_owned_training(argv,ROOT,stream,deadline-time.monotonic(),env,stdin,LIMIT,
                                                     before_resume=suspended,absolute_deadline=deadline,cleanup_seconds=5)
    for path in [out,cache,log,*cache.rglob("*")]:
        conversion.guard(plan,path,True,deadline=time.monotonic()+10)
    require(log.stat().st_size <= 16<<20,"CPU reference log exceeds original bound")
    lines = [line[len(MARKER):] for line in log.read_text("utf-8").splitlines() if line.startswith(MARKER)]
    complete = parent.settled(observation) and len(lines) == 1 and sources == source_set()
    result = json.loads(lines[0]) if complete else None
    error = None
    if complete:
        try:
            verify_result(result,rows,mode,args.role)
            checked(input_ref)
        except (ValueError,KeyError,OSError) as exception:
            complete = False
            error = str(exception)
    with execution.open("x",encoding="utf-8") as stream:
        json.dump({"schema":"airk.private.compact-protected-cal-cpu-batch/1","complete":complete,
                   "role":args.role,"input":input_ref,"first":args.first,"count":args.count,"sources":sources,
                   "command":argv,"owned":observation,"error":error,"result":result if complete else None},stream,allow_nan=False)
    conversion.guard(plan,execution,True,deadline=time.monotonic()+10)
    require(complete,"incomplete original CPU reference batch; see retained original Job and log")


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    mode = parser.add_mutually_exclusive_group(required=True)
    mode.add_argument("--check-only",action="store_true")
    mode.add_argument("--output",type=Path)
    mode.add_argument("--worker-deadline",type=float)
    parser.add_argument("--input",type=Path,default=CONTROL)
    parser.add_argument("--input-sha256",default=CONTROL_SHA)
    parser.add_argument("--first",type=int,required=True)
    parser.add_argument("--count",type=int,required=True)
    parser.add_argument("--role",choices=("protected_matching","publisher_fp32"),required=True)
    parser.add_argument("--cal-pid",type=int)
    parser.add_argument("--source-set")
    args = parser.parse_args()
    require(sys.platform == "win32" and sys.flags.isolated and sys.flags.no_site and sys.dont_write_bytecode,
            "original checked Windows Python -I -B -S required")
    input_ref = ref(args.input,args.input_sha256)
    rows,epoch = selected(input_ref,args.first,args.count)
    if args.check_only:
        math_owner,_,_,_ = owners()
        print(json.dumps({"mode":epoch,"role":args.role,"rows":[{"ordinal":r["ordinal"],"status":r["status"],
                         "input_row_sha256":row_key(r),"remaining_ms":r.get("remaining_ms"),
                         "width":len(r.get("compiled",{}).get("input_ids",[]))} for r in rows],
                         "matching_recipe":math_owner["RECIPE_SHA"],"model_forwards":0},sort_keys=True))
    elif args.output:
        owned(args,input_ref,rows,epoch)
    else:
        require(0 < args.worker_deadline-time.monotonic() <= 600 and source_set() == json.loads(args.source_set),
                "foreign parent provenance or scientific deadline")
        result = score_batch(rows,epoch,args.role,args.worker_deadline,args.cal_pid)
        verify_result(result,rows,epoch,args.role)
        print(MARKER+json.dumps(result,separators=(",",":"),allow_nan=False))


if __name__ == "__main__":
    main()
