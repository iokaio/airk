# SPDX-License-Identifier: Apache-2.0
"""Summarize the full frozen TRAIN cohort, including absences and unqualified runs."""
import hashlib
import json
from pathlib import Path
import runpy

HERE = Path(__file__).resolve().parent
inputs = runpy.run_path(str(HERE / "capture_inputs.py"))
focus = runpy.run_path(str(HERE / "focus1024.py"))
ROSTER_SHA = "586f4d2d22c2cc4daef0c6b71bef5035f8165f9f8bde546a093d08d474049d67"
require = inputs["require"]


def observed(path):
    require(path.resolve(strict=True) == path and path.is_file(), "canonical regular receipt required")
    raw = path.read_bytes()
    return json.loads(raw), hashlib.sha256(raw).hexdigest()


def collect(row):
    case = row["case"]
    run = inputs["CAMPAIGN"] / ("compact-development-" + case + "-focus1024-run-r1")
    out = {"case": case, "original_descriptor_sha256": row["descriptor_sha256"]}
    if not (run / "owned.json").is_file():
        return dict(out, state="started_unsettled" if run.exists() else "unobserved")
    owner, out["owned_sha256"] = observed(run / "owned.json")
    out["elapsed_seconds"] = owner["elapsed_seconds"]
    require(owner["focus_width"] == 1024, "unexpected focus width")
    if (owner["exit_code"] != 0 or owner["timed_out"] or owner["owned_job_forced_termination"]
            or not owner["owned_descendants_settled"] or not owner["source_seals_unchanged"]
            or owner["owned_job_final_active_processes"] != 0 or owner["owned_job_final_member_pids"] != []
            or not (run / "capture.private.json").is_file()):
        return dict(out, state="unqualified_capture", exit_code=owner["exit_code"])
    original = inputs["load"](inputs["INPUTS"] / case / "pilot.private.json", row["descriptor_sha256"])
    expected = inputs["digest"](inputs["encode"](focus["bind"](original, row["descriptor_sha256"])))
    capture, out["capture_sha256"] = observed(run / "capture.private.json")
    require(capture["case_id"] == case and capture["input_sha256"] == owner["capture_descriptor_sha256"] == expected
            and capture["ordered_source_documents"] == 50 and capture["judged"] is False, "case provenance differs")
    out.update(active_descriptor_sha256=expected, publication=capture["publication"])
    admissions = []
    for item in capture["native_observations"]:
        require(Path(item["file"]).name == item["file"], "foreign admission path")
        record = inputs["load"](run / "fixture/context-cal-admissions" / item["file"], item["sha256"])
        require(record["actual_Focus_profile"] == focus["FOCUS"] and not record["Context_NN_dispatch"]
                and record["request"]["remaining_input_tokens"] == item["original_remaining_tokens"], "admission identity differs")
        admissions.append(dict(item, remaining_ns=record["remaining_ns_at_observation"], ordinal=record["request"]["ordinal"]))
    if admissions:
        require("zero_disposition" not in capture, "both positive and zero admissions")
        return dict(out, state="admitted", admissions=admissions)
    zero = capture["zero_disposition"]
    require(zero["kind"] == "observed_budget_stop_without_context_admission" and zero["native_admissions"] == 0
            and zero["compiler_rows"] == 0, "unexpected zero disposition")
    return dict(out, state="budget_stop_zero", zero_disposition=zero)


if __name__ == "__main__":
    roster = inputs["load"](inputs["INPUTS"] / "roster.private.json", ROSTER_SHA)
    require(len(roster) == len({row["case"] for row in roster}) == 15, "frozen TRAIN membership differs")
    rows = [collect(row) for row in roster]
    counts = {state: sum(row["state"] == state for row in rows) for state in sorted({row["state"] for row in rows})}
    print(json.dumps({"schema": "airk.private.techqa-train-focus1024-capture-census/1", "roster_sha256": ROSTER_SHA,
                      "active_focus_manifest": focus["FOCUS"], "counts": counts, "rows": rows,
                      "context_model_scored": False, "task_quality_measured": False}, sort_keys=True))
