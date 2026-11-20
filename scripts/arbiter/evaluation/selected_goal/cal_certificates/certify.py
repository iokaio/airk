# SPDX-License-Identifier: Apache-2.0
"""Generate independent original human-support gain certificates from a complete live CAL capture."""

import argparse
from collections import Counter
import json
import re
from pathlib import Path
import sys

# Isolated script mode intentionally removes the script directory.
HERE = Path(__file__).resolve().parent
sys.path.insert(0, str(HERE))
from capture_closure import question_events, verify_fixture
from reference_inputs import digest, load_inputs, read, reference, require
from phased_closure import phased_events


def real_scope(result, binding):
    require("read_scope" not in result and result["scope_observation"] == binding.observation,
            "canonical result or selected source revision differs")


def original_gains(task, request, probes, binding, ledger, utility):
    cards = request["roster"]["cards"]
    require(len({c["id"] for c in cards}) == len(cards), "duplicate whole-card identity")
    def baseline():
        observed = ledger()
        for record in request["records"]:
            require(record["scope"] == request["scope"], "baseline evidence crossed the native request scope")
            canonical = record["canonical_result"]
            publication = canonical.get("publication", canonical.get("read", {}).get("publication"))
            require(publication == binding.publication, "baseline source publication differs")
            observed.observe(canonical)
        return observed
    before = baseline().visible_candidates()
    progress, certified, unresolved = [], [], []
    for item in probes:
        probe, delivered = item["probe"], item["result"]
        key = probe["choice"]["id"]
        require(probe["tool"] in ("search", "context") and probe["publication"] == binding.publication and
                probe["arguments"]["response_mode"] == "full" and probe["arguments"]["include_status"] is True,
                "foreign original counterfactual")
        if delivered["refused"]:
            unresolved.append({"choice": probe["choice"], "tool": probe["tool"], "arguments": probe["arguments"],
                               "actual_call": delivered, "classification": "UNKNOWN", "semantic_negative": False})
            continue
        canonical = delivered["body"]
        real_scope(canonical, binding)
        require(canonical["publication"] == binding.publication, "counterfactual publication differs")
        after = baseline()
        after.observe(canonical)
        score = utility(task, before, after.visible_candidates())
        progress.append({"choice": key, "original_human_reference_utility": score,
                         "classification": "REFERENCE_GAIN" if score["useful"] else "UNKNOWN",
                         "human_semantic_gold": False, "semantic_negative": False})
        if score["useful"]:
            for card in cards:
                if card["action"].get("kind") == "read" and card["action"].get("choice") == key:
                    certified.append({"card": card["id"], "choice": key, "proxy": "generic_read_original_reference_complement",
                                      "necessity_or_optimality_claim": False})
    return certified, progress, unresolved


def certificate(evidence, task, audit, binding, policy, ledger, utility):
    status, answer, observations, probes = (evidence[k] for k in ("scope", "answer", "captured", "probes"))
    real_scope(status, binding)
    require(status["collection"] == binding.publication["collection"] and
            status["publication"]["token"] == binding.publication["token"], "per-question scope differs")
    request, census, remaining = None, [], None
    certified, progress, unresolved, undelivered = [], [], [], []
    if answer["refused"]:
        require(not probes, "undelivered Answer was probed")
        capture_status = "verification_budget_refusal_no_delivered_assessment"
        undelivered = [{"observation": record, "association": "UNDELIVERED_NOT_A_REQUEST_OR_LABEL"}
                       for record in observations]
    else:
        body = answer["body"]
        real_scope(body, binding)
        require(body["question"] == task["question"], "foreign delivered question")
        capture_status = "no_native_assessment_request"
        if not observations:
            require(not probes, "uncaptured request was probed")
        else:
            record, = observations
            request = record["request"]
            capture_status = "native_request_captured"
            require(record["schema"] == "airk.private-selected-Context-native-admission-observation/1" and
                    record["Context_NN_dispatch"] is False and record["Focus_configuration"] == "ON" and
                    record["actual_Focus_profile"] == audit["profiles"]["arbiter-focus-qwen3-reranker-06b"] and
                    record["actual_Context_profile"]["manifest"] == audit["profiles"]["arbiter-context-qwen3-17b"]
                    and record["MiniLM_pipeline"] == audit["pipeline"], "native assessment profile differs")
            require(request["question"] == task["question"] and request["roster"]["policy"] == policy and
                    re.fullmatch(r"[a-f0-9]{32}", request["read_attempt"]["request_id"]) and
                    re.fullmatch(r"[a-f0-9]{32}", body["request_id"]) and
                    isinstance(request["records"], list) and bool(request["records"]) and isinstance(body["records"], list) and
                    request["records"] == body["records"][:len(request["records"])] and
                    request["scope"] == body["assessment"]["scope"], "foreign native request association")
            binding.scope(request["scope"])
            history = [r for r in body["assessment_history"] if r["ordinal"] == request["ordinal"]]
            require(len(history) == 1 and history[0]["basis"] == request["roster"]["basis"], "assessment basis differs")
            advice = history[0]["result"]["advice"]
            require(advice["input_digest"] == record["planned_input_digest"] and advice["observed_input_tokens"] == 0
                    and advice["work"]["input_tokens"] == 0 and advice["outcome"]["status"] == "unavailable",
                    "native non-dispatch or planned-input association differs")
            require([p["probe"] for p in probes] == record["verified_probe_requests"] and
                    len({p["probe"]["choice"]["id"] for p in probes}) == len(probes) <= 5,
                    "counterfactual roster differs")
            certified, progress, unresolved = original_gains(task, request, probes, binding, ledger, utility)
            census = record["compiler_census"]
            remaining = min(request["remaining_ms"], record["remaining_ns_at_observation"] // 1000000)
    return {"id": task["id"], "question": task["question"], "split": "calibration", "source_group": task["source_component"],
            "native_capture_status": capture_status, "request": request, "compiler_census": census,
            "remaining_ms_at_observation": remaining, "human_semantic_gold": False, "semantic_negative": False,
            "reference_proxies": {"valid_reference_proxy_whole_cards": certified,
                                  "counterfactual_reference_progress": progress,
                                  "counterfactual_unresolved": unresolved,
                                  "undelivered_observations_are_not_scored": undelivered}}


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("--preflight", action="store_true")
    parser.add_argument("--capture", type=Path)
    parser.add_argument("--outer-sha256")
    parser.add_argument("--closure-sha256")
    parser.add_argument("--mutex-release-sha256", help="Grouped capture only: checked final shared installation release")
    parser.add_argument("--active-audit", type=Path)
    parser.add_argument("--audit-sha256")
    parser.add_argument("--focus-manifest")
    args = parser.parse_args()
    require(sys.flags.isolated and sys.dont_write_bytecode and sys.flags.no_site, "use checked Python -I -B -S")
    epoch = (args.active_audit, args.audit_sha256, args.focus_manifest)
    require(all(epoch) or not any(epoch), "supply all active audit and Focus identity arguments")
    active = reference(args.active_audit, args.audit_sha256) if all(epoch) else None
    audit, ids, tasks, ledger, utility, policy, sources, index_root = load_inputs(active, args.focus_manifest)
    if args.preflight:
        require(not any((args.capture, args.outer_sha256, args.closure_sha256)), "preflight is source-only")
        print(json.dumps({"ordered_cal_questions": len(ids), "source_files": len(ledger().files),
                          "source_groups": len({t["source_component"] for t in tasks.values()}),
                          "task_policy": policy, "focus_tokens": 1024 if active else 512,
                          "focus_profile": audit["profiles"]["arbiter-focus-qwen3-reranker-06b"],
                          "reference_certificates_issued": 0}))
        return
    require(active is not None, "live certificates require the active static-1024 Focus launch audit")
    if not all((args.capture, args.outer_sha256, args.closure_sha256)):
        parser.error("supply a complete capture plus outer and fixture-closure SHA-256")
    closure = read(reference(args.capture/"fixture-closure.json",args.closure_sha256))
    phased = closure.get("schema") in ("airk.private.cal100-phased-fixture/1","airk.private.cal100-grouped-fixture/1")
    if phased:
        events = phased_events(args.capture,args.outer_sha256,args.closure_sha256,audit,args.audit_sha256,
                               ids,sources,index_root,tasks,release_sha=args.mutex_release_sha256)
        custody = {"lifetime_plan_sha256":closure["plan"]["sha256"],
                   "lifetime_closure_sha256":[ref["sha256"] for ref in closure["lifetimes"]]}
    else:
        closure, census, binding, answer = verify_fixture(args.capture, args.outer_sha256, args.closure_sha256,
                                                          audit, sources, index_root, args.audit_sha256)
        events = question_events(args.capture, closure, census, ids, binding, answer, tasks, audit["pipeline"])
        custody = {"phase_census_sha256":closure["phase_census"]["sha256"]}
    if closure["schema"] == "airk.private.cal100-grouped-fixture/1":
        custody["mutex_release_sha256"] = args.mutex_release_sha256
    rows = [certificate(e, tasks[e["id"]], audit, window, policy, ledger, utility) for e, window in events]
    require([r["id"] for r in rows] == ids, "incomplete CAL100")
    if not phased:
        binding.verify()
    refs = audit["inputs"]
    print(json.dumps({"schema": "airk.private-selected-Context-CAL-reference-preflight/1", "complete": True,
                      "qualified_profiles": [], "Focus_configuration": "ON",
                      "Focus_profile": audit["profiles"]["arbiter-focus-qwen3-reranker-06b"],
                      "MiniLM_pipeline": audit["pipeline"], "corpus_sha256": refs["corpus"]["sha256"],
                      "capture_sha256": args.closure_sha256, "source_manifest_sha256": refs["manifest"]["sha256"],
                      "rubric_sha256": refs["rubric"]["sha256"], "task_policy": policy, "human_semantic_gold_count": 0,
                      "outer_owned_sha256": args.outer_sha256, **custody,
                      "launch_audit_sha256": args.audit_sha256, "focus_tokens": 1024, "source_sha256": {p.name: digest(p.read_bytes()) for p in
                          (HERE/"certify.py", HERE/"capture_closure.py", HERE/"phased_closure.py", HERE/"grouped_closure.py", HERE/"reference_inputs.py")},
                      "native_capture_status_counts": Counter(r["native_capture_status"] for r in rows),
                      "scope": "Original human support gain from additional verified reads only. Terminal labels unavailable.",
                      "rows": rows}, ensure_ascii=True, separators=(",", ":"), allow_nan=False))


if __name__ == "__main__":
    main()
