# SPDX-License-Identifier: Apache-2.0
"""Join independently completed phase reference vectors to the closed CAL100 original compile census."""
import argparse
import json
import math
from pathlib import Path
import sys

sys.path.insert(0, str(Path(__file__).resolve().parent))
from phase_reference import AUDIT, GRAPH, WEIGHTS, PROFILE, ORIGINAL, checked, phase_rows, require


def json_pin(ref):
    return json.loads(checked(ref))


def settled(job):
    return (type(job.get("exit_code")) is int and job["exit_code"] == 0
            and job.get("timed_out") is False and job.get("log_oversized") is False
            and job.get("child_assigned_before_resume") is True and job.get("owned_descendants_settled") is True
            and job.get("owned_job_forced_termination") is False and job.get("owned_job_final_active_processes") == 0
            and job.get("owned_job_final_member_pids") == [] and "cleanup_refusal" not in job)


def match_row(compiled, row):
    require(row["id"] == compiled["id"] and row["status"] == "eligible" and
            compiled["status"] == "compiled_original_input" and row["compiled"] == compiled["compiled"]
            and row["roster"] == compiled["roster"], "phase reference and complete original input differ")
    n = len(compiled["roster"]["cards"])
    require(1 <= n <= 8 and all(len(row[k]) == n and all(type(v) in (float,int) and math.isfinite(v) for v in row[k])
                              for k in ("matching_fp16","original_fp32")), "invalid independent reference cardinality")
    return {k: row[k] for k in ("id","compiled","roster","matching_fp16","original_fp32")}


def reference_archive(descriptor):
    census = json_pin(descriptor["original_compiled"])
    pre = json_pin(census["preflight"])
    require(census["schema"]=="airk.private.compact-cal-original-compiled-inputs/1" and census["complete"] is True
            and census["manifest"]==ORIGINAL and census["NN_invocations"]==0 and census["launch_audit"]["sha256"]==AUDIT
            and pre["complete"] is True and pre["launch_audit_sha256"]==AUDIT and pre["focus_tokens"]==1024
            and pre["capture_sha256"]==census["capture_sha256"], "incomplete original CAL input")
    require(len(pre["rows"])==len(census["rows"])==100 and len(descriptor["phases"])==13,"incomplete CAL100 census")
    original_ids=[r["id"] for r in census["rows"]]
    require(len(set(original_ids))==100 and original_ids==[r["id"] for r in pre["rows"]],"original order differs")
    basis="f0d0e01fa28e7a876f1c57bc9894dff7e2a915ac8bf8150885b2aac2fdff2f14"
    submitted=iter(descriptor["reference_jobs"])
    rows=[]
    for phase, ref in enumerate(descriptor["phases"]):
        observation=json_pin(ref)
        require(observation["phase"]==phase,"phase source out of order")
        for row in phase_rows(ref):
            ordinal=row["ordinal"]
            compiled=census["rows"][ordinal]
            require(row["id"]==compiled["id"],"phase/case ordinal differs")
            if row["status"]!="eligible":
                require(row["status"]==compiled["status"],"original refusal changed")
                continue
            job_ref=next(submitted,None)
            require(job_ref is not None,"missing independent reference")
            job=json_pin(job_ref)
            require(job["schema"]=="airk.private.compact-cal-cpu-phase/1" and job["complete"] is True
                    and settled(job["owned"]) and job["phase"]==ref
                    and job["source"]["sha256"]==descriptor["reference_source_sha256"],"unclosed or foreign reference job")
            result=job["result"]
            require(result["phase"]==ref and result["graph"]==GRAPH and result["weights"]==WEIGHTS and
                    result["profile"]==PROFILE and result["candidate_forwards"]==0 and result["reference_basis"]["sha256"]==basis, "reference candidate differs")
            scored,=result["rows"]
            require(scored["ordinal"]==ordinal and scored["captures"]==row["captures"],"foreign source capture")
            rows.append(match_row(compiled,scored))
    require(next(submitted,None) is None,"extra independent reference")
    return {"schema":"airk.private.compact-cal-independent-references/1","complete":True,"rows":rows,
            "candidate_profile_sha256":PROFILE,"matching_reference_basis_sha256":basis,"candidate_graph_sha256":GRAPH,"candidate_weights_sha256":WEIGHTS,
            "preflight_sha256":census["preflight"]["sha256"],"original_compiled_sha256":descriptor["original_compiled"]["sha256"],
            "reference_jobs":descriptor["reference_jobs"],"reference_source_sha256":descriptor["reference_source_sha256"],
            "phase_inputs":descriptor["phases"]}


if __name__=="__main__":
    p=argparse.ArgumentParser(description=__doc__)
    p.add_argument("--descriptor",type=Path,required=True)
    p.add_argument("--sha256",required=True)
    args=p.parse_args()
    require(sys.flags.isolated and sys.dont_write_bytecode and sys.flags.no_site,"use checked Python -I -B -S")
    print(json.dumps(reference_archive(json_pin({"path":str(args.descriptor),"sha256":args.sha256})),
                     allow_nan=False,separators=(",",":")))
