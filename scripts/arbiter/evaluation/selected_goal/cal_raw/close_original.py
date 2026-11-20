# SPDX-License-Identifier: Apache-2.0
"""Close existing original native CPU readouts against the independent complete CAL100 compiler census."""
import argparse
import json
from pathlib import Path
import sys

sys.path.insert(0,str(Path(__file__).resolve().parent))
from assemble import json_pin, settled
from close_scores import core, validate_readout
from phase_reference import AUDIT, ORIGINAL, require

PROFILE="13085b0cbb6ed15bb4d6fb0318aad59b60e9c0d9c9ddb3e19f5b9f14be52f620"


def close(d):
    compiled=json_pin(d["original_compiled"])
    raw=json_pin(d["raw"])
    pre=json_pin(compiled["preflight"])
    audit=json_pin(compiled["launch_audit"])
    job=json_pin(d["owned_job"])
    require(settled(job) and core(job["suspended_assignment"]["image"])==core(d["producer_binary"])
            and raw["test_binary_sha256"]==d["producer_binary"]["sha256"],"unclosed or foreign original scoring image")
    require(compiled["complete"] is True and compiled["NN_invocations"]==0 and compiled["launch_audit"]["sha256"]==AUDIT
            and compiled["manifest"]==ORIGINAL and compiled["original_profile"]["sha256"]==PROFILE
            and pre["complete"] is True and pre["focus_tokens"]==1024 and pre["launch_audit_sha256"]==AUDIT
            and raw["schema"]=="airk.arbiter.context-action.selected-cal-raw/1" and raw["complete"] is True
            and raw["profile"]==ORIGINAL and raw["model"]=="arbiter-context-qwen3-17b" and raw["profile_source_sha256"]==PROFILE
            and raw["capture_sha256"]==pre["capture_sha256"]==compiled["capture_sha256"]
            and raw["task_policy"]==pre["task_policy"] and raw["corpus_sha256"]==pre["corpus_sha256"]
            and raw["profiles_sha256"]==audit["inputs"]["inputs"]["sha256"] and raw["preflight_sha256"]==compiled["preflight"]["sha256"]
            and raw["release_qualified"] is raw["task_qualified"] is False,"original artifact, Focus epoch or input differs")
    ids=json_pin(audit["census"])["question_ids"]
    require(len(set(ids))==len(ids)==100 and all(ids==[r["id"] for r in a["rows"]] for a in (pre,compiled,raw)),
            "incomplete original CAL population")
    for i,(prepared,actual,source) in enumerate(zip(compiled["rows"],raw["rows"],pre["rows"],strict=True)):
        if prepared["status"]=="compiled_original_input":
            validate_readout(actual,prepared,source,i)
        else:
            require(prepared["status"] in ("no_native_assessment_request","compiler_refused_remaining_allowance")
                    and all(actual.get(k)==v for k,v in prepared.items()) and actual.get("scores") is None,
                    "original refusal status differs")
    return {"schema":"airk.private.compact-cal-raw-closure/1","complete":True,"origin":"native_original_compact_cpu",
            "raw_sha256":d["raw"]["sha256"],"profile_sha256":PROFILE,"preflight_sha256":compiled["preflight"]["sha256"],
            "test_binary_sha256":d["producer_binary"]["sha256"],"owned_jobs":[core(d["owned_job"])],"original_compiled":d["original_compiled"]}


if __name__=="__main__":
    p=argparse.ArgumentParser(description=__doc__)
    p.add_argument("--descriptor",type=Path,required=True)
    p.add_argument("--sha256",required=True)
    a=p.parse_args()
    require(sys.flags.isolated and sys.flags.no_site and sys.dont_write_bytecode,"use checked Python -I -B -S")
    print(json.dumps(close(json_pin({"path":str(a.descriptor),"sha256":a.sha256})),allow_nan=False,separators=(",",":")))
