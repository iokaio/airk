# SPDX-License-Identifier: Apache-2.0
"""Extract only original publisher FP32 scores from an authenticated CAL CPU reference Job."""
import argparse
import json
import math
from pathlib import Path
import sys

sys.path.insert(0,str(Path(__file__).resolve().parent))
from phase_reference import ORIGINAL, checked, phase_rows, require
from assemble import settled

REVIEWED_SOURCE="3f1e555896de310f41872157cc062670d8653a676de9d27aede38297f38a9161"
REFERENCE_BASIS="f0d0e01fa28e7a876f1c57bc9894dff7e2a915ac8bf8150885b2aac2fdff2f14"


def extract(source_job, target_phase, ordinal):
    job=json.loads(checked(source_job))
    require(job["schema"]=="airk.private.compact-cal-cpu-phase/1" and job["complete"] is True and
            job["source"]["sha256"]==REVIEWED_SOURCE and settled(job["owned"]), "unreviewed original FP32 source")
    actual=job["result"]
    require(actual["candidate_forwards"]==0 and actual["reference_basis"]["sha256"]==REFERENCE_BASIS
            and actual["phase"]==job["phase"],"foreign original publisher reference basis")
    source,=actual["rows"]
    require(source["ordinal"]==ordinal and source["status"]=="eligible","wrong original ordinal")
    previous_rows=[r for r in phase_rows(job["phase"]) if r["ordinal"]==ordinal]
    current_rows=[r for r in phase_rows(target_phase) if r["ordinal"]==ordinal]
    require(len(previous_rows)==len(current_rows)==1,"original ordinal absent or duplicated in target phase")
    previous,current=previous_rows[0],current_rows[0]
    # Equality includes ordered capture pins. The request carries original scope, records,
    # complete roster/options and original combined budget in addition to compiled tensor bytes.
    require(previous==current and all(source[k]==current[k] for k in
                ("ordinal","id","status","captures","compiled","roster")), "different original request, scope, roster or tensors")
    scores=source["original_fp32"]
    require(len(scores)==len(current["roster"]["cards"]) and all(type(x) in (float,int) and math.isfinite(x) for x in scores),
            "original publisher output is incomplete")
    basis=json.loads(checked(actual["reference_basis"]))
    return {"schema":"airk.private.compact-cal-original-publisher-reference/1",
            "origin":"independent_publisher_cpu_fp32","original_model":"arbiter-context-qwen3-17b",
            "original_profile_manifest":ORIGINAL,"phase":target_phase,"ordinal":ordinal,"id":source["id"],
            "captures":source["captures"],"compiled":source["compiled"],"roster":source["roster"],"original_fp32":scores,
            "publisher_files":basis["publisher_files"],"runtime":actual["runtime"],"reference_basis":actual["reference_basis"],
            "reference_job":source_job,"producer_source":job["source"]}


if __name__=="__main__":
    p=argparse.ArgumentParser(description=__doc__)
    p.add_argument("--job",type=Path,required=True)
    p.add_argument("--job-sha256",required=True)
    p.add_argument("--phase",type=Path,required=True)
    p.add_argument("--phase-sha256",required=True)
    p.add_argument("--ordinal",type=int,required=True)
    a=p.parse_args()
    require(sys.flags.isolated and sys.flags.no_site and sys.dont_write_bytecode,"use checked Python -I -B -S")
    print(json.dumps(extract({"path":str(a.job),"sha256":a.job_sha256},{"path":str(a.phase),"sha256":a.phase_sha256},a.ordinal),
                     allow_nan=False,separators=(",",":")))
