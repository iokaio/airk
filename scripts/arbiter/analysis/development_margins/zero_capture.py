# SPDX-License-Identifier: Apache-2.0
"""Classify an observed development budget stop with no Context assessment."""


def zero_disposition(answer, request, binding, admission_dir):
    expected = {"source": "named", "index_ids": [binding["index_id"]]}
    if (answer["question"] != request["question"] or not answer["request_id"]
            or answer["read_scope"] != expected or answer["scope_observation"] != binding["scope_observation"]
            or answer["assessment"]["scope"]["publication"] != binding["publication"]):
        raise ValueError("request or source binding changed")
    if answer["assessment_history"] or any(admission_dir.iterdir()):
        raise ValueError("expected zero Context assessments and zero observer records")
    if (answer["stop"] != "budget" or answer["assessment"]["termination"] != "budget"
            or not any(answer["work"][dimension] == request["budget"][dimension]
                       for dimension in ("calls", "fanout"))
            or any(answer["work"][dimension] > request["budget"][dimension] for dimension in ("calls", "fanout"))):
        raise ValueError("unverified budget stop")
    focus = [row["canonical_result"]["focus"] for row in answer["records"] if row["operation"] == "search"]
    return {"kind": "observed_budget_stop_without_context_admission", "request_id": answer["request_id"],
            "stop": answer["stop"], "work": answer["work"], "focus": focus,
            "canonical_records": len(answer["records"]), "canonical_candidate_count": answer["candidate_count"],
            "canonical_omitted_count": answer["omitted_count"], "native_admissions": 0, "compiler_rows": 0}


def verify_empty_journal(summary, dispatches):
    if (summary.get("complete") is not True or summary.get("error") or dispatches
            or any(summary[field] != 0 for field in ("records", "bytes", "last_sequence", "dropped", "writer_failed"))):
        raise ValueError("unverified empty recorder")
