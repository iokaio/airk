# SPDX-License-Identifier: Apache-2.0
"""Prepare review metadata only. No subprocess, native DTO or model execution."""
import argparse
import json
from pathlib import Path
from closure import checked_json, checked_bytes, require, resolve_sources

INPUT_ROLES = frozenset({"census", "corpus", "source_manifest", "rubric", "task_policy",
                         "profiles", "native_image_receipt", "process_owner_contract"})
SOURCE_ROLES = frozenset({"cal_prepare", "cal_controller", "cal_capture", "cal_support", "cal_aggregate",
    "cal_contract", "cal_native_client", "cal_native_session", "certificate_prepare", "certificate_run",
    "certificate_producer", "raw_prepare", "raw_run", "a6_run", "a6_native_client", "a6_native_session",
    "audit_sources", "aggregate_native_retrieval", "goal_sources", "goal_trial", "goal_rubric",
    "grade_native_retrieval", "selected_stage_observations", "selected_cal_refusals", "eval_agent_smoke", "qwen_export"})


def partitions(ids):
    require(isinstance(ids, list) and len(ids) == 100 and all(isinstance(x, str) and x for x in ids)
            and len(set(ids)) == 100, "exact100 unique ordered IDs required")
    return [{"chunk_index": index, "question_start": start, "question_end": min(start + 8, 100),
             "questions": ids[start:min(start + 8, 100)]} for index, start in enumerate(range(0, 100, 8))]


def prepare(request, repository):
    require(set(request) == {"schema", "source_maps", "source_selections", "inputs"} and
            request["schema"] == "airk.selected-goal-preparation-request/1", "preparation request shape")
    require(set(request["source_selections"]) == SOURCE_ROLES, "complete literal helper-role roster required")
    catalogue = json.loads(Path(__file__).with_name("source-selections.json").read_bytes())
    require(request["source_selections"] == catalogue["selections"], "selected Section7 source version differs")
    require(set(request["inputs"]) == INPUT_ROLES, "complete immutable input roster required")
    repository = Path(repository).resolve()
    source_maps = request["source_maps"]
    expected_paths = [repository / path for path in catalogue["source_maps"]]
    require(isinstance(source_maps, list) and len(source_maps) == len(expected_paths)
            and all(isinstance(reference, dict) and isinstance(reference.get("path"), str)
                    and Path(reference["path"]).parts == expected.parts
                    for reference, expected in zip(source_maps, expected_paths)),
            "exact maintained source maps required in catalogue order")
    maps = [checked_json(reference) for reference in source_maps]
    sources = resolve_sources(request["source_selections"], maps, repository)
    for role, reference in request["inputs"].items():
        if role != "census":
            checked_bytes(reference)
    census = checked_json(request["inputs"]["census"])
    require(set(census) == {"schema", "question_ids"} and census["schema"] == "airk.selected-goal-census/1", "opaque census projection required")
    phases = partitions(census["question_ids"])
    # No native profile parsing, task-policy reconstruction or historical deadline reuse.
    return {"schema": "airk.selected-goal-review-packet/1", "status": "prepared_for_review",
            "execution_authorized": False, "qualification_granted": False,
            "operational_adapters_complete": False, "source_maps": request["source_maps"],
            "sources": sources, "inputs": request["inputs"], "phases": phases,
            "required_before_execution": ["review current complete runtime/import closure",
                "review native profile/task-policy compatibility", "review whole-census coverage",
                "implement and review owned controller/capture adapters", "freeze fresh resource release and deadlines"]}


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("--request", required=True)
    parser.add_argument("--request-sha256", required=True)
    parser.add_argument("--request-bytes", type=int, required=True)
    args = parser.parse_args()
    reference = {"path": str(Path(args.request).absolute()), "sha256": args.request_sha256, "bytes": args.request_bytes}
    request = checked_json(reference)
    result = prepare(request, Path(__file__).resolve().parents[4])
    result["request"] = reference
    print(json.dumps(result, ensure_ascii=False, indent=2))

if __name__ == "__main__":
    main()
