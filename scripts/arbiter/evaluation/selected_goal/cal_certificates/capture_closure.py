# SPDX-License-Identifier: Apache-2.0
"""Follow the original closed fixture's pins down to every delivered question and probe."""

from pathlib import Path
import re
import runpy

from reference_inputs import AUDIT, CAMPAIGN, HERE, checked, digest, parsed, read, reference, require


def settled(owner):
    return (type(owner.get("exit_code")) == int and owner["exit_code"] == 0 and owner.get("timed_out") is False
            and owner.get("log_oversized") is False and owner.get("child_assigned_before_resume") is True
            and owner.get("owned_descendants_settled") is True and owner.get("owned_job_forced_termination") is False
            and owner.get("owned_job_final_active_processes") == 0 and owner.get("owned_job_final_member_pids") == []
            and "cleanup_refusal" not in owner)


def verify_fixture(root, outer_sha, fixture_sha, audit, sources, index_root):
    require(root.is_absolute() and root.parent == CAMPAIGN and root.name.startswith("cal100-live-capture-"),
            "original CAL capture directory required")
    outer = read(reference(root / "outer-owned.json", outer_sha))
    require(settled(outer) and "--capture" in outer["argv"] and "--owned" in outer["argv"] and
            str(root) in outer["argv"], "full naturally settled capture Job required")
    closure = read(reference(root / "fixture-closure.json", fixture_sha))
    require(closure["setup_only"] is False and closure["fitted"] is False and closure["phase_census"],
            "full CAL fixture closure required")
    work = root / "fixture"
    closed = read(closure["closed"], work / "closed.json")
    request = read(closure["live_request"], root / "live-request.json")
    start = read(request["start_receipt"], root / "fixture-start.json")
    ready = read(request["ready"], work / "ready.json")
    binding_path = work / "enable-context.bindings.json"
    bindings = read(request["bindings"], binding_path)
    answer = read(request["answer"], work / "context-request.json")
    census = read(closure["phase_census"], root / "phases.json")
    require(closed["shutdown_completed"] is True and closed["service_lease_reacquired"] is True and
            closed["instance"] == ready["health"]["instance"] == closure["instance"] == bindings["instance"] ==
            census["instance"] and len(bindings["bindings"]) == 1 and request["fixture_root"] == str(work) and
            start["input_audit_sha256"] == AUDIT and start["deadline_ms"] - start["start_ms"] == 10800000 and
            ready["health"]["process_id"] == start["pid"] and ready["health"]["executable_sha256"] ==
            audit["inputs"]["service"]["sha256"] and ready["inputs_sha256"] == audit["inputs"]["inputs"]["sha256"],
            "source fixture, service, clock or closure binding differs")
    binding = sources["ResidentBinding"](binding_path, source_root=index_root)
    require(census["schema"] == "airk.private.cal100-owned-phase-census/1" and
            census["publication"] == binding.publication and census["request_sha256"] == closure["live_request"]["sha256"]
            and census["original_start"] == request["start_receipt"] and census["ordered_questions"] == 100 and
            len(census["phase_closures"]) == 13 and 0 <= census["probes"] <= 500 and
            census["independent_certificates"] is False and census["fitted"] is False, "wrong phase census")
    proof = HERE.parent / "cal_capture/dispatch_proof.py"
    require(digest(proof.read_bytes()) == "75b370b0446e0be1946896cb8978e8028aed0fb939320d4c72426f40386ea027",
            "dispatch proof owner changed")
    focus = audit["profiles"]["arbiter-focus-qwen3-reranker-06b"]
    require(runpy.run_path(str(proof))["read_journal"](work, focus, setup_only=False) == closure["dispatch"],
            "Focus return or zero-Context dispatch proof differs")
    warm = read(closure["warm_summary"], work / "warm-completions.closed.json")
    require(Path(closure["warm_journal"]["path"]) == work / "warm-completions.jsonl" and
            len(checked(closure["warm_journal"])) == warm["bytes"] and warm["complete"] is True and
            warm["dropped"] == 0 and warm["error"] is None and warm["observer_id"] == ready["warm_observer_id"] and
            warm["records"] == warm["last_sequence"] == 2, "warm observation closure differs")
    require(answer["budget"]["input_tokens"] == 8192 and answer["budget"]["deadline_ms"] == 120000 and
            answer["response_mode"] == "full" and answer["include_status"] is True, "Answer template differs")
    return closure, census, binding, answer


def exchange(event, tool, args, index):
    cmd = event["argv"]
    require(len(cmd) == 9 and cmd[:6] == ["tools", "call", tool, "--index", index, "--args"] and
            parsed(cmd[6]) == args and cmd[7:] == ["--output", "json"], "observed canonical call arguments differ")
    result = event["result"]
    envelope = parsed(result["stdout"])
    body = envelope.get("structuredContent")
    if body is None:
        require(len(envelope["content"]) == 1 and envelope["content"][0]["type"] == "text", "missing full envelope")
        body = parsed(envelope["content"][0]["text"])
    if result["exitCode"] == 1 and result["class"] == "refused" and envelope.get("isError") is True:
        error = body["error"]
        require(error["code"] == "verification_reply_budget" and error["retryable"] is False and
                error["verification_performed"] is False, "unrecognized refusal in complete capture")
        return {"refused": True, "body": body}
    require(result["exitCode"] == 0 and result["class"] == "success" and envelope.get("isError") is not True,
            "canonical call missing or failed")
    return {"refused": False, "body": body}


def question_events(root, closure, census, ids, binding, template, tasks):
    observed_names = set()
    total = 0
    for phase, pin in enumerate(census["phase_closures"]):
        expected = ids[phase*8:(phase+1)*8]
        stem = root / "phases" / f"phase-{phase:02}"
        owner = read(pin, stem.with_suffix(".owned.json"))
        require(settled(owner) and owner["phase"] == phase and owner["request_sha256"] == census["request_sha256"]
                and Path(owner["transcript"]["path"]) == stem.with_suffix(".jsonl"), "incomplete phase ownership")
        expected_sources = {HERE.parent / "cal_capture" / n for n in
                            ("worker.cjs", "phase.cjs", "controller.py", "prepare.py")}
        expected_sources.add(HERE.parents[3] / "provision-static-onnx-runtime.py")
        require(len(owner["sources"]) == len(expected_sources) and
                {Path(r["path"]) for r in owner["sources"]} == expected_sources, "phase source roster differs")
        for ref in owner["sources"]:
            checked(ref)
        raw = checked(owner["transcript"])
        exchanges, captures, processed = [], [], []
        closed = False
        for line in raw.splitlines():
            event = parsed(line)
            kind = event["kind"]
            if kind == "native_exchange":
                require(not closed, "exchange after phase close")
                exchanges.append(event)
            elif kind == "native_observation":
                name = event["name"]
                require(not closed and re.fullmatch(r"record-[0-9]{4}\.json", name) and name not in observed_names,
                        "duplicate or foreign native admission file")
                observed_names.add(name)
                captures.append(read(reference(root / "fixture/context-cal-admissions" / name, event["sha256"])))
            elif kind == "cal_question":
                evidence = event["evidence"]
                key = evidence["id"]
                probes = evidence["probes"]
                require(not closed and len(processed) < len(expected) and key == expected[len(processed)] and
                        captures == evidence["captured"] and len(captures) <= 1 and len(probes) <= 5 and
                        len(exchanges) == 2 + len(probes), "question, capture or counterfactual census differs")
                status = exchange(exchanges[0], "status", {"response_mode": "full", "include_status": True}, binding.index_id)
                answer = exchange(exchanges[1], "answer", dict(template, question=tasks[key]["question"]), binding.index_id)
                require(status == {"refused": False, "body": evidence["scope"]} and answer == evidence["answer"],
                        "question evidence differs from original delivered result")
                for record, call in zip(probes, exchanges[2:], strict=True):
                    p = record["probe"]
                    require(exchange(call, p["tool"], p["arguments"], binding.index_id) == record["result"],
                            "counterfactual result differs from delivered bytes")
                processed.append(key)
                total += len(probes)
                yield evidence
                exchanges.clear()
                captures.clear()
            elif kind == "phase_complete":
                require(not closed and event == {"kind": "phase_complete", "phase": phase, "ids": expected,
                        "instance": closure["instance"], "publication": binding.publication} and processed == expected,
                        "phase close differs")
                closed = True
        require(closed and not exchanges and not captures, "missing close or unassociated native observations")
    require(total == census["probes"] and observed_names == {p.name for p in
            (root / "fixture/context-cal-admissions").glob("record-*.json")}, "capture omitted observations")
