# SPDX-License-Identifier: Apache-2.0
"""Reconcile the immutable TRAIN_Q111 observer with its delivered Answer."""
import hashlib
import json
from pathlib import Path
import runpy


HERE = Path(__file__).resolve().parent
INPUTS = runpy.run_path(str(HERE / "capture_inputs.py"))
SHARED = HERE.parent / "public_source_admission/run_techqa_predispatch_pilot.py"
PILOT = runpy.run_path(str(SHARED))
CASE = "TRAIN_Q111"
RUN = INPUTS["CAMPAIGN"] / "compact-development-TRAIN_Q111-run-r1"
PROOFS = {
    "owned.json": "fef5be21007f4149d44f23f5dafb8b4b55566f4ee834c26ba976a9d45af119e7",
    "outer.log": "56d3137cc48e77e8c682c0ab0f011b9a4e72f99729e04836f3ac7b1cfe2bba48",
    "commands/answer.stdout": "88a52383434baff51373af146734be6786ff80a84cb9cad6f708d6588ad2bbae",
    "commands/ready-stages.stdout": "bd7ce3a336bb18b04f3b96b8711ec3c7fb2ad5a1b652e190f51eb3981af3fdce",
    "commands/pipeline.stdout": "aacc5720325d7b1334efc8d164d242490c50072f28fcbed4ec9ee204418bbf9d",
    "commands/request-scope.stdout": "d018f406615e29d43eb380a96816257a001a84ea9b9cc0b36610a6c66dde385a",
    "commands/answer.command.json": "ea5dca65f1927421dd822b638086a965d5213300ae57aff8e51d13b70d395b83",
    "fixture/ready.json": "341f2c994875081af4cb34a55bfc0fe9a8e09968b3e4648d505f5da95eff17af",
    "fixture/closed.json": "1a856af22515188f33d93232c7ce8751d03ba7e7df4e9a93ca876548aaee16ea",
    "fixture/composed-observer.closed.json": "b92868c4f1251796a626fea40532b8a6652e2378917631ea8c7ed3bc32bb2838",
    "fixture/composed-observer.jsonl": "9b86df79e0607639d361bb448158d2f3919bc664903a051044461b47e4a09674",
    "fixture/warm-completions.closed.json": "52a5e4a278a973434bfc261aaf94a7e3e3d14921f00fa8d411f5f9d8b4a3ffe3",
    "fixture/context-cal-admissions/record-0001.json": "931d55ada0e27ecef1256c0c34046c641420ac84ad273546f6525294cf4a2b20",
}


def reconcile():
    require, read, pin = (PILOT[k] for k in ("require", "read", "pin"))
    descriptor = INPUTS["validate_one"](CASE)
    request = read(INPUTS["INPUTS"] / CASE / "request.private.json")
    for path, sha in PROOFS.items():
        pin(RUN / path, sha)
    owner = read(RUN / "owned.json")
    require(owner["source_seals_unchanged"] and owner["owned_descendants_settled"] and owner["exit_code"] == 1
            and not owner["timed_out"] and not owner["owned_job_forced_termination"]
            and owner["owned_job_final_active_processes"] == 0 and not owner["owned_job_final_member_pids"]
            and (RUN / "outer.log").read_bytes().rstrip().endswith(b"ValueError: captured request or scope differs from canonical Answer"),
            "original worker disposition differs")
    ready, closed = (read(RUN / "fixture" / name) for name in ("ready.json", "closed.json"))
    require(ready["health"]["instance"] == closed["instance"] and closed["shutdown_completed"]
            and closed["service_lease_reacquired"] and ready["health"]["executable_sha256"] == descriptor["runtime_inputs"]["service"]["sha256"]
            and ready["inputs_sha256"] == descriptor["runtime_inputs"]["inputs"]["sha256"], "fixture identity/settlement differs")
    route = read(RUN / "commands/ready-stages.stdout")
    for stage, profile in (("focus", descriptor["cpu_focus_manifest"]), ("context", descriptor["cpu_context_bootstrap_manifest"])):
        require(route[stage]["active_profile"] == route[stage]["selected_profile"] == profile
                and route[stage]["availability"] == "ready", "required stage unavailable")

    def body(name):
        envelope = read(RUN / "commands" / (name + ".stdout"))
        require(envelope.get("isError") is not True, "failed native envelope")
        return envelope["structuredContent"]

    answer, status, search = (body(key) for key in ("answer", "request-scope", "pipeline"))
    require(read(RUN / "commands/answer.command.json")["exit_code"] == 0
            and search["semantic_readiness"]["pipeline_digest"] == descriptor["required_minilm_pipeline"]
            and not search["execution"]["incomplete"] and search["execution"]["served_mode"] == "hybrid"
            and status["files"] == 50 and status["publication"]["token"] == search["publication"]["token"]
            and status["scope_observation"] == search["scope_observation"] and len(status["read_scope"]["index_ids"]) == 1,
            "canonical source/scope binding differs")
    binding = {"context": descriptor["cpu_context_bootstrap_manifest"], "focus": descriptor["cpu_focus_manifest"],
               "pipeline": descriptor["required_minilm_pipeline"], "publication": search["publication"],
               "scope_observation": status["scope_observation"], "index_id": status["read_scope"]["index_ids"][0]}
    directory = RUN / "fixture/context-cal-admissions"
    require([p.name for p in directory.iterdir()] == ["record-0001.json"]
            and len(answer["assessment_history"]) == 1, "original admission population differs")
    record = read(directory / "record-0001.json")
    observation = PILOT["confirmed_census"](record, answer, request, binding)
    journal = RUN / "fixture/composed-observer.jsonl"
    summary = read(RUN / "fixture/composed-observer.closed.json")
    events = [json.loads(line) for line in journal.read_bytes().splitlines()]
    require(summary["complete"] and not summary["error"] and summary["dropped"] == summary["writer_failed"] == 0
            and journal.stat().st_size == summary["bytes"] and len(events) == summary["records"] == summary["last_sequence"]
            and not any(event["task"] == "airk.context_action.v1" and event["kind"] in ("dispatch_admitted", "score_returned")
                        for event in events), "incomplete dispatch closure")
    return {"schema": "airk.private.compact-development-reconciled-admission/1", "case": CASE, "split": "TRAIN",
            "judged": False, "capture_focus": binding["focus"], "context": binding["context"],
            "descriptor_sha256": INPUTS["digest"](INPUTS["encode"](descriptor)), "source_proofs": PROOFS,
            "canonical_request_id": answer["request_id"], "native_attempt_id": record["request"]["read_attempt"]["request_id"],
            "source_scope": binding["scope_observation"], "publication": binding["publication"],
            "original_worker_exit": 1, "canonical_call_exit": 0, "fixture_settled": True,
            "observed_census": observation, "original_allowance": record["request"]["remaining_input_tokens"],
            "source_sha256": hashlib.sha256(Path(__file__).read_bytes()).hexdigest()}


if __name__ == "__main__":
    print(json.dumps(reconcile(), sort_keys=True, ensure_ascii=True))
