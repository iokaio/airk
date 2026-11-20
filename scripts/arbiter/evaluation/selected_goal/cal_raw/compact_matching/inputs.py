# SPDX-License-Identifier: Apache-2.0
"""Keep original R5 compiler, remaining allowance and ordered cards for each CPU reference."""
import hashlib
import json
import math
from pathlib import Path
import sys

HERE = Path(__file__).resolve().parent
RAW = HERE.parent
sys.path.insert(0,str(RAW))
from phase_reference import AUDIT, CAMPAIGN, ORIGINAL, SCIENCE, checked, digest, phase_rows, require
import original_reference
import r5_chronological_inputs as chronology

CONTROL = CAMPAIGN/"cal100-phase00-ordinal000-original-publisher-reference-r1.json"
CONTROL_SHA = "29e3b76021122971c0303e981696e5776c39242b9b31c142456464d173e52945"
CONTROL_JOB = "b39ad07b620f16ef161cce50992ee10bdefe2de03c073efd2eb52393965094a3"
CONTROL_PHASE = "792e27acfe386f0255a1a7fa1bfd1212e979f86b45ed327e14d59008272033c0"
R5_SOURCE = "6da3b982fb8ef837b5e67b2312de6caf3b54dd72859e53abeced6f2a2d0de40c"
MATH_SHA = "9df92e6393079f61b6c2ccd13792afe243d4b3078fc2a6effc04c6651039e46f"
FULL_SHA = "5b12a98cee8525323ac16076fe82e3b3895471c534cdc48947f76f9785813595"
SUFFIX_SHA = "03f2170fa7112396bfb861ea486763371b2ca8707d4bb2e060d72af902d21e59"
STAGE_SHA = "4465463b434f6bb5e9798307c31e622dc3da4d67b6a5f6bf5793daf5c94beabf"
STAGE = CAMPAIGN/"compact-cutlass-protected-staged-r1/stage.json"


def ref(path,sha):
    return {"path":str(path),"sha256":sha}


def row_key(row):
    raw = json.dumps(row,sort_keys=True,separators=(",",":"),ensure_ascii=False,allow_nan=False).encode()
    return hashlib.sha256(raw).hexdigest()


def future_rows(input_ref):
    receipt = json.loads(checked(input_ref))
    require(receipt["source_sha256"] == digest(Path(chronology.__file__)) == R5_SOURCE and
            receipt["plan_sha256"] == chronology.PLAN and receipt["acquired_sha256"] == chronology.ACQUIRED and
            receipt["NN_invocations"] == receipt["gain_certificates"] == 0,"different R5 input-only producer")
    plan = chronology.read(chronology.reference(chronology.ROOT/"lifetime-plan.json",chronology.PLAN))
    reports = chronology.collect(plan,receipt["lifetimes"],receipt["terminal"])
    published = json.loads(checked(receipt["models"]["compact"]))
    phases = published["phases"]
    require(len(phases) == len(reports) and all(json.loads(checked(pin)) == report for pin,report in zip(phases,reports,strict=True)),
            "original independent phase evidence changed")
    data = chronology.projections(plan,reports,phases)["compact"]
    require(data == published and
            data["original_manifest"] == ORIGINAL and data["launch_audit_sha256"] == AUDIT and
            data["original_requests"] == receipt["original_requests"] in (56,100),
            "original Compact requests or complete source group changed")
    return data["rows"], "r5_original"


def original_control():
    original = json.loads(checked(ref(CONTROL,CONTROL_SHA)))
    require(original["reference_job"]["sha256"] == CONTROL_JOB and original["phase"]["sha256"] == CONTROL_PHASE and
            original_reference.extract(original["reference_job"],original["phase"],0) == original,
            "original publisher FP32 control did not reproduce from authenticated historical Job")
    rows = phase_rows(original["phase"],model="compact",retain_budget=True)
    row = rows[0]
    require(row["ordinal"] == 0 and row["status"] == "eligible" and row["compiled"] == original["compiled"] and
            row["roster"] == original["roster"] and row["captures"] == original["captures"] and
            len(row["compiled"]["input_ids"]) == 1408 and len(original["original_fp32"]) == len(row["roster"]["cards"]) == 7 and
            all(math.isfinite(x) for x in original["original_fp32"]), "T1408 control input or entire reference vector changed")
    return row, original


def selected(input_ref,first,count):
    require(type(first) is int and first >= 0 and type(count) is int and 1 <= count <= 8,
            "one to eight consecutive original ordinals required")
    if Path(input_ref["path"]) == CONTROL and input_ref["sha256"] == CONTROL_SHA:
        row, _ = original_control()
        rows, mode = [row], "r4_engineering_only"
    else:
        rows, mode = future_rows(input_ref)
    batch = rows[first:first+count]
    require(len(batch) == count and [r["ordinal"] for r in batch] == list(range(first,first+count)),
            "incomplete or reordered original ordinal range")
    for row in batch:
        require(row["status"] in ("eligible","no_native_assessment_request","compiler_refused_remaining_allowance"),
                "unknown native row status")
        if row["status"] == "eligible":
            compiled = row["compiled"]
            n = len(compiled["input_ids"])
            require(0 < n <= row["remaining_input_tokens"] <= 8192 and 0 < row["remaining_ms"] <= 120000 and
                    compiled["attention_mask"] == [1]*n and len(compiled["candidate_positions"]) == len(row["roster"]["cards"]) <= 8,
                    "original scope, roster, remaining time or tensor shape differs")
    return batch,mode
