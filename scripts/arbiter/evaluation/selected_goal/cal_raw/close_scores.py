# SPDX-License-Identifier: Apache-2.0
"""Verify every original ordinal and assemble the native GPU raw stream for the scientific fit."""
import argparse
import hashlib
import json
import math
from pathlib import Path
import sys

sys.path.insert(0,str(Path(__file__).resolve().parent))
from assemble import json_pin, settled
from phase_reference import AUDIT, GRAPH, ORIGINAL, PROFILE, WEIGHTS, checked, require

MANIFEST = "dabbc7e062f4f5a350ed486c43c4af2a5cfe5db9dd6c1693739c03a70a5603cf"


def core(pin):
    return {"path":pin["path"],"sha256":pin["sha256"]}


def validate_readout(actual, compiled, original, ordinal):
    require(actual["id"]==original["id"]==compiled["id"] and actual["source_group"]==original["source_group"]
            and actual["split"]=="calibration" and actual["status"]=="raw_scores_only", "foreign CAL score")
    require(actual["compiled"]==compiled["compiled"] and actual["roster"]==compiled["roster"],
            "original compiled prompt/card order changed")
    n=len(compiled["roster"]["cards"])
    require(1<=n<=8 and len(actual["scores"])==n and all(type(x) in (float,int) and math.isfinite(x) for x in actual["scores"])
            and all(type(actual[k]) is int and actual[k]>=0 for k in ("compile_us","inference_us"))
            and actual["compile_us"]+actual["inference_us"]<=compiled["remaining_ms"]*1000,
            f"incomplete or late original CAL row {ordinal}")


def close(descriptor):
    census=json_pin(descriptor["original_compiled"])
    preflight=census["preflight"]
    pre=json_pin(preflight)
    refs=json_pin(descriptor["references"])
    image=descriptor["producer_binary"]
    require(census["schema"]=="airk.private.compact-cal-original-compiled-inputs/1"
            and census["complete"] is True and census["NN_invocations"]==0 and census["manifest"]==ORIGINAL
            and pre["complete"] is True and pre["focus_tokens"]==1024 and pre["launch_audit_sha256"]==AUDIT
            and pre["capture_sha256"]==census["capture_sha256"] and len(pre["rows"])==len(census["rows"])==100,
            "complete closed CAL100 required")
    audit=json_pin(census["launch_audit"])
    require(census["launch_audit"]["sha256"]==AUDIT and refs["complete"] is True
            and refs["candidate_profile_sha256"]==PROFILE and refs["candidate_graph_sha256"]==GRAPH
            and refs["candidate_weights_sha256"]==WEIGHTS and refs["original_compiled_sha256"]==descriptor["original_compiled"]["sha256"]
            and refs["preflight_sha256"]==preflight["sha256"],"foreign independent evidence")
    ids=json_pin(audit["census"])["question_ids"]
    require(ids==[r["id"] for r in pre["rows"]]==[r["id"] for r in census["rows"]] and len(set(ids))==100,
            "changed CAL population")
    jobs=iter(descriptor["score_jobs"])
    references=iter(refs["rows"])
    raw_rows=[]
    owned_jobs=[]
    for ordinal,(prepared,original) in enumerate(zip(census["rows"],pre["rows"],strict=True)):
        if prepared["status"] != "compiled_original_input":
            require(prepared["status"] in {"compiler_refused_remaining_allowance","no_native_assessment_request"},
                    "unrecognized original refusal")
            raw_rows.append(prepared)
            continue
        ref=next(references,None)
        require(ref is not None and ref["id"]==prepared["id"] and ref["compiled"]==prepared["compiled"]
                and ref["roster"]==prepared["roster"],"reference row missing or changed")
        pin=next(jobs,None)
        require(pin is not None,"missing native GPU Job")
        job=json_pin(pin)
        require(job["complete"] is True and job["mode"]=="qkv-scientific-cal" and settled(job["owned"]) and
                core(job["image"])==core(image) and job["projection_assets"]["candidate_graph"]["sha256"]==GRAPH
                and job["projection_assets"]["candidate_weights"]["sha256"]==WEIGHTS, "foreign or unsettled GPU Job")
        native=json_pin(job["outputs"]["native"])
        require(native["schema"]=="airk.private.compact-qkv-scientific-cal-row/1" and native["complete"] is True
                and native["profile"]==MANIFEST and native["native_binary_sha256"]==image["sha256"] and
                native["scientific_cal"]["ordinal"]==ordinal and native["scientific_cal"]["candidate_profile"]["sha256"]==PROFILE
                and native["scientific_cal"]["preflight"]==core(preflight)
                and native["scientific_cal"]["original_compiled"]==core(descriptor["original_compiled"])
                and native["scientific_cal"]["independent_references"]==core(descriptor["references"])
                and native["native_forward_returned"] is native["forward_synchronized"] is True and
                native["session_teardown_observed"] is native["device_teardown_observed"] is native["native_host_charge_release_observed"] is True
                and native["process_cuda_poisoned"] is False and native["numeric_forward_attempts"]==1,
                "missing native scoring/settlement evidence")
        actual=native["cal_row"]
        validate_readout(actual,prepared,original,ordinal)
        raw_rows.append(actual)
        owned_jobs.append(job["owned"])
    require(next(jobs,None) is None and next(references,None) is None and owned_jobs,"extra jobs/reference rows or zero actual calls")
    raw={"schema":"airk.arbiter.context-action.selected-cal-raw/1","complete":True,"model":"arbiter-context-qwen3-17b",
         "profile":MANIFEST,"profile_source_sha256":PROFILE,"profiles_sha256":audit["inputs"]["inputs"]["sha256"],
         "preflight_sha256":preflight["sha256"],"capture_sha256":pre["capture_sha256"],"corpus_sha256":pre["corpus_sha256"],
         "task_policy":pre["task_policy"],"test_binary_sha256":image["sha256"],"task_qualified":False,"release_qualified":False,
         "human_semantic_gold_count":0,"rows":raw_rows}
    return raw,owned_jobs


if __name__=="__main__":
    p=argparse.ArgumentParser(description=__doc__)
    p.add_argument("--descriptor",type=Path,required=True)
    p.add_argument("--sha256",required=True)
    p.add_argument("--output",type=Path,required=True)
    args=p.parse_args()
    require(sys.flags.isolated and sys.flags.no_site and sys.dont_write_bytecode,"use checked Python -I -B -S")
    d=json.loads(checked({"path":str(args.descriptor),"sha256":args.sha256}))
    raw,jobs=close(d)
    from phase_reference import CAMPAIGN
    out=args.output
    require(out.resolve()==out and out.parent==CAMPAIGN and out.is_dir() and not any(out.iterdir()),"fresh prechecked output required")
    def emit(name,value):
        path=out/name
        data=(json.dumps(value,allow_nan=False,separators=(",",":"))+"\n").encode()
        with path.open("xb") as stream:
            stream.write(data)
        return {"path":str(path),"sha256":hashlib.sha256(data).hexdigest()}
    raw_pin=emit("raw.private.json",raw)
    receipts=[emit(f"owned-{i:03}.json",job) for i,job in enumerate(jobs)]
    closure=emit("scoring-closure.private.json",{"schema":"airk.private.compact-cal-raw-closure/1","complete":True,
        "origin":"native_compact_qkv_cuda","raw_sha256":raw_pin["sha256"],"profile_sha256":PROFILE,
        "preflight_sha256":raw["preflight_sha256"],"test_binary_sha256":raw["test_binary_sha256"],
        "returned_synchronized":True,"independent_references":core(d["references"]),"owned_jobs":receipts,
        "original_gpu_jobs":d["score_jobs"],"original_compiled":d["original_compiled"]})
    print(json.dumps({"raw":raw_pin,"scoring_closure":closure,"original_ordinals":len(raw["rows"]),"actual_calls":len(jobs)}))
