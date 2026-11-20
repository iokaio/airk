# SPDX-License-Identifier: Apache-2.0
"""Join two actual original-budget admissions to one typed numerical reference."""
import hashlib
import json
import math
from pathlib import Path


CAMPAIGN = Path(__file__).resolve().parents[4] / "artifacts/v190/final-plan/exec-20261116T052942Z"
OLD = CAMPAIGN / "compact-development-TRAIN_Q111-run-r1"
NEW = CAMPAIGN / "compact-development-TRAIN_Q111-focus1024-run-r1"


def load(path, expected):
    data = path.read_bytes()
    if hashlib.sha256(data).hexdigest() != expected:
        raise ValueError("retained evidence changed: " + path.name)
    return json.loads(data)


def associations(record):
    request = record["request"]
    choices = request["choices"]
    by_id = {entry["id"]: entry for entry in choices}
    if len(by_id) != len(choices):
        raise ValueError("duplicate offered choices")
    records = {entry["id"]: entry for entry in request["records"]}

    def target(entry):
        value = entry["target"]
        prefix = "airk.literal-search/1:"
        if entry["operation"] != "search" or not value.startswith(prefix):
            return value
        literal = json.loads(value[len(prefix):])
        source = records[literal.pop("record")]
        if source["operation"] != "search":
            raise ValueError("literal target points outside its canonical search")
        evidence = {"operation": source["operation"], "arguments": source["arguments"],
                    "hits": source["canonical_result"]["hits"]}
        wire = json.dumps(evidence, sort_keys=True, ensure_ascii=True, separators=(",", ":")).encode()
        return {"literal": literal, "source_search_arguments_and_full_hits_sha256": hashlib.sha256(wire).hexdigest()}

    actions = []
    for card in request["roster"]["cards"]:
        action = card["action"]
        if action["kind"] == "read":
            entry = by_id[action["choice"]]
            actions.append({"kind": "read", "operation": entry["operation"], "target": target(entry)})
        else:
            actions.append(action)
    return {"offered_target_order": [[entry["operation"], target(entry)] for entry in choices],
            "scored_action_order": actions}


def main():
    records = []
    sources = {}
    def pinned(path, sha):
        sources[path.relative_to(CAMPAIGN).as_posix()] = sha
        return load(path, sha)

    historical = pinned(CAMPAIGN / "compact-development-TRAIN_Q111-reconciled-r1/receipt.private.json",
                        "4a5098b8d893c5d0c2f477ef7f52a4c6a235d906cc20569c332fe32bc839e93a")
    active = pinned(NEW / "capture.private.json", "2b9d0f236fa317d8547ddc1ae4becb65222ec1c752b94bfafdc95c334c5520a5")
    active_owner = pinned(NEW / "owned.json", "0b5028a926027a0712c024ae7b9e44c58ab5d4d5820f5cc46c5e7c0875a1d4fc")
    if active_owner["exit_code"] or active_owner["owned_job_final_active_processes"] or not historical["fixture_settled"]:
        raise ValueError("admission owner not settled")
    for run, sha in ((OLD, historical["source_proofs"]["fixture/context-cal-admissions/record-0001.json"]),
                     (NEW, active["native_observations"][0]["sha256"])):
        records.append(pinned(run / "fixture/context-cal-admissions/record-0001.json", sha))

    receipt = pinned(CAMPAIGN / "compact-development-q111-publisher-reference-r2/execution.json",
                     "f657bbcdc9dac88829376c639bf2f20b6e4ae4c40f796f6305c1ec044bb59b91")
    result = receipt["result"]
    if receipt["complete"] is not True or result["capture"]["sha256"] != sources[
            (OLD / "fixture/context-cal-admissions/record-0001.json").relative_to(CAMPAIGN).as_posix()]:
        raise ValueError("numerical receipt is not bound to original admission")
    bindings = []
    typed = []
    for record in records:
        request = record["request"]
        profile = record["actual_Context_profile"]["manifest"]
        compiler, = [row for row in record["compiler_census"] if row["profile"] == profile]
        if (not compiler["fits_remaining_allowance"] or not record["actual_Focus_forward_observed"]
                or record["Context_NN_dispatch"] or compiler["NN_invocations"]):
            raise ValueError("unexpected admission role")
        typed.append(compiler["compiled"])
        bindings.append({"focus_profile": record["actual_Focus_profile"], "context_profile": profile,
                         "source_scope": request["scope"], "eligible_source": request["authority"],
                         "parent_budget": request["parent_budget"], "read_attempt": request["read_attempt"],
                         "remaining_ns_at_observation": record["remaining_ns_at_observation"],
                         "remaining_ms": request["remaining_ms"], "remaining_input_tokens": request["remaining_input_tokens"],
                         "ordinal": request["ordinal"], "roster": request["roster"], "choices": request["choices"],
                         "compiled": compiler["compiled"], **associations(record)})
    old, new = bindings
    if (set(typed[0]) != {"prompt", "input_ids", "attention_mask", "position_ids", "decision_position",
                          "option_mask", "candidate_positions", "input_digest"} or typed[0] != typed[1]
            or typed[0] != result["compiled"] or old["context_profile"] != new["context_profile"]
            or old["parent_budget"] != new["parent_budget"] or old["scored_action_order"] != new["scored_action_order"]
            or old["offered_target_order"] != new["offered_target_order"]
            or old["focus_profile"] == new["focus_profile"]):
        raise ValueError("active numerical input or semantic action alignment differs")
    scores = {}
    for key in ("original_fp32", "matching_fp16"):
        vector = result[key]
        if len(vector) != len(new["roster"]["cards"]) or not all(math.isfinite(v) for v in vector):
            raise ValueError("reference live score shape differs")
        ordered = sorted(vector, reverse=True)
        scores[key] = {"winner": vector.index(ordered[0]), "margin": ordered[0] - ordered[1],
                       "within_0020": ordered[0] - ordered[1] <= 0.020, "within_0140": ordered[0] - ordered[1] <= 0.140}
    print(json.dumps({"schema": "airk.private.focus-width-context-input-comparison/1",
                      "case": "TRAIN_Q111", "source_sha256": sources, "bindings": bindings,
                      "exact_compiled_inputs_equal": True, "actual_offered_and_scored_target_order_equal": True,
                      "raw_scope_equal": old["source_scope"] == new["source_scope"],
                      "raw_choice_ids_equal": old["choices"] == new["choices"],
                      "raw_roster_equal": old["roster"] == new["roster"], "referenced_scores": scores,
                      "active_scoring_or_task_quality": False}, sort_keys=True))


if __name__ == "__main__":
    main()
