# SPDX-License-Identifier: Apache-2.0
"""Reconcile the original DEV_Q002 call when no native assessment occurred."""

import json
from pathlib import Path
import runpy

HERE = Path(__file__).resolve().parent
pilot = runpy.run_path(str(HERE / "run_techqa_predispatch_pilot.py"))
require, read, pin = (pilot[k] for k in ("require", "read", "pin"))
RUN = pilot["CAMPAIGN"] / "techqa-predispatch-pilot-run-r2"
PROOFS = {
    "owned.json": "70d8a03feed44ff634abd90898c2879b030abc2c0d4955df115e9988afc79b74",
    "commands/answer.stdout": "9997a58787a9eed1e845b3d1941c499c63ad105549b0e80e377999cea9dbeaa0",
    "commands/request-scope.stdout": "02daf57ad5883edc415cae0a40a0e7de1bd6f22060e8d3c1bb87674fa40ea098",
    "commands/pipeline.stdout": "9d03d255cd4ade2aef421043406b0c1ac539815e73f17ee77671e63864fb6cd9",
    "commands/ready-stages.stdout": "e3a19e410fbe11f4b41401a7a1d635367b18f45604652229ba223ed6be88c80e",
    "fixture/closed.json": "b408c1147af0d6485bf3c33d64e8e41b0ec8e06c112bc05d3ea9599801e34028",
    "fixture/composed-observer.closed.json": "16326c02f9f6a2ed9a322a4ddb6e8c88b6374ff7f85d63552058b3b5996d8471",
    "outer.log": "a054cd721184140bebb9c6fe06544f55029b1a809a27728d459c561567932520",
    "fixture/warm-completions.closed.json": "0881cdd21c443fed2aa48086e6d9c324e7dca35465eb061942c753a742534f77",
    "commands/answer.command.json": "c8e100701911de7bf168947f276df8f1a2b7aee2461bd051ce6174a0c2b79304",
}


def reconcile():
    descriptor = pilot["validate_inputs"]()
    for path, digest in PROOFS.items():
        pin(RUN / path, digest)
    request = read(pilot["INPUT"] / "request.private.json")
    def body(label):
        envelope = read(RUN / "commands" / (label + ".stdout"))
        require(envelope["isError"] is False, "canonical call failed")
        return envelope["structuredContent"]

    answer, scope, search = (body(label) for label in ("answer", "request-scope", "pipeline"))
    full = search["publication"]
    require(answer["question"] == request["question"] and answer["read_scope"] == scope["read_scope"]
            and answer["scope_observation"] == scope["scope_observation"] == search["scope_observation"]
            and answer["assessment"]["scope"]["publication"] == full and scope["publication"]["token"] == full["token"]
            and scope["files"] == 50 and search["semantic_readiness"]["pipeline_digest"] == descriptor["required_minilm_pipeline"]
            and search["execution"]["served_mode"] == "hybrid" and not search["execution"]["incomplete"], "source binding differs")
    ready = read(RUN / "commands/ready-stages.stdout")
    for stage, profile in (("focus", descriptor["cpu_focus_manifest"]), ("context", descriptor["cpu_context_bootstrap_manifest"])):
        require(ready[stage]["active_profile"] == ready[stage]["selected_profile"] == profile
                and ready[stage]["availability"] == "ready", "original profile unavailable")
    require(not any((RUN / "fixture/context-cal-admissions").iterdir()) and answer["assessment_history"] == []
            and answer["assessment"]["termination"] == answer["stop"] == "budget"
            and all(answer["work"][key] == request["budget"][key] == 8 for key in ("calls", "fanout")),
            "zero-admission budget disposition differs")
    focus = [row["canonical_result"]["focus"] for row in answer["records"] if row["operation"] == "search"]
    require(len(focus) == 5 and focus[0]["kind"] == "receipt" and not focus[0]["receipt"]["applied"]
            and focus[0]["receipt"]["availability"] == "failed" and focus[0]["receipt"]["observed_input_tokens"] is None
            and all(row["kind"] == "not_applied" and row["availability"] == "constrained" and row["work_unknown"] is False
                    and row["reason"].startswith("Prior optional work is unsettled;") for row in focus[1:]), "failed/unknown Focus census differs")
    worker = read(RUN / "owned.json")
    closed = read(RUN / "fixture/closed.json")
    journal = read(RUN / "fixture/composed-observer.closed.json")
    warm = read(RUN / "fixture/warm-completions.closed.json")
    command = read(RUN / "commands/answer.command.json")
    require(worker["exit_code"] == 1 and worker["source_seals_unchanged"] and worker["owned_descendants_settled"]
            and not worker["owned_job_forced_termination"] and not worker["owned_job_final_member_pids"]
            and worker["owned_job_final_active_processes"] == 0 and not worker["timed_out"] and command["exit_code"] == 0
            and command["elapsed_ns"] <= request["budget"]["deadline_ms"] * 1000000
            and closed["shutdown_completed"] and closed["service_lease_reacquired"]
            and warm["complete"] and warm["records"] == 2 and warm["dropped"] == 0
            and journal["complete"] and not journal["error"] and journal["records"] == journal["last_sequence"] ==
            journal["bytes"] == journal["dropped"] == journal["writer_failed"] == (RUN / "fixture/composed-observer.jsonl").stat().st_size == 0,
            "original command, owner or fixture settlement differs")
    return {"schema": "airk.private.techqa-zero-native-admission-census/1", "case_id": descriptor["question_id"],
            "original_input": pilot["DESCRIPTOR"], "original_run": str(RUN), "original_evidence": PROOFS,
            "ordered_source_documents": 50, "source_publication": full, "canonical_records": len(answer["records"]),
            "canonical_candidates": answer["candidate_count"], "canonical_omissions": answer["omitted_count"],
            "canonical_stop": answer["stop"], "canonical_work": answer["work"], "native_admissions": 0,
            "observed_offered_menus": 0, "observed_compiler_prompts": 0, "context_dispatches": 0,
            "first_focus_disposition": "failed; actual work unknown", "subsequent_focus_fallbacks": len(focus) - 1,
            "canonical_elapsed_ns": command["elapsed_ns"], "original_positive_capture_worker_exit": 1,
            "fixture_settled": True, "judged": False}


if __name__ == "__main__":
    result = reconcile()
    output = pilot["CAMPAIGN"] / "techqa-predispatch-pilot-census-r1" / "census.private.json"
    with output.open("x", encoding="utf-8", newline="\n") as stream:
        json.dump(result, stream, sort_keys=True, indent=2)
        stream.write("\n")
