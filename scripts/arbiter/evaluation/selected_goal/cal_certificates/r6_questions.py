# SPDX-License-Identifier: Apache-2.0
"""Independently reopen each R6 phase under the exact worker enrolled for that service."""
import re
from pathlib import Path
from capture_closure import settled, exchange, observed_searches, window_binding
from reference_inputs import HERE, checked, parsed, read, reference, require


def question_events(root, closure, census, ids, binding, template, tasks, pipeline, *,
                    first_phase, lifetime_fingerprints, worker):
    observed_names = set()
    fingerprints = set() if lifetime_fingerprints is None else lifetime_fingerprints
    total = searches = 0
    for phase, pin in enumerate(census["phase_closures"], start=first_phase):
        expected = ids[phase*8:(phase+1)*8]
        stem = (root / "phases" if lifetime_fingerprints is None else root) / f"phase-{phase:02}"
        owner = read(pin, stem.with_suffix(".owned.json"))
        require(settled(owner) and owner["phase"] == phase and owner["request_sha256"] == census["request_sha256"]
                and Path(owner["transcript"]["path"]) == stem.with_suffix(".jsonl"), "incomplete phase ownership")
        expected_sources = {HERE.parent / "cal_capture" / n for n in
                            ("worker.cjs", "phase.cjs", "window_scope.cjs", "controller.py", "prepare.py")}
        expected_sources.discard(HERE.parent/"cal_capture/worker.cjs")
        expected_sources.add(worker)
        require(owner["argv"][1:5]==[str(worker),str(root/"live-request.json"),census["request_sha256"],str(phase)], "phase worker, request or ordinal changed")
        expected_sources.add(HERE.parents[3] / "provision-static-onnx-runtime.py")
        require(len(owner["sources"]) == len(expected_sources) and
                {Path(r["path"]) for r in owner["sources"]} == expected_sources, "phase source roster differs")
        for ref in owner["sources"]:
            checked(ref)
        raw = checked(owner["transcript"])
        exchanges, captures, processed = [], [], []
        closed = False
        grant = completed = None
        for line in raw.splitlines():
            event = parsed(line)
            kind = event["kind"]
            if kind == "window_grant":
                require(not closed and grant is None and completed is None and len(processed) < len(expected)
                        and event["question_id"] == expected[len(processed)], "unexpected or overlapping window grant")
                grant = event
            elif kind == "native_exchange":
                require(not closed and grant is not None and completed is None, "exchange outside original live grant")
                exchanges.append(event)
            elif kind == "native_observation":
                name = event["name"]
                require(not closed and grant is not None and completed is None and re.fullmatch(r"record-[0-9]{4}\.json", name) and name not in observed_names,
                        "duplicate or foreign native admission file")
                observed_names.add(name)
                captures.append(read(reference(root / "fixture/context-cal-admissions" / name, event["sha256"])))
            elif kind == "cal_question":
                evidence = event["evidence"]
                key = evidence["id"]
                probes = evidence["probes"]
                require(not closed and grant is not None and completed is None and len(processed) < len(expected) and key == expected[len(processed)] and
                        captures == evidence["captured"] and len(captures) <= 1 and len(probes) <= 5 and
                        len(exchanges) == 2 + len(probes), "question, capture or counterfactual census differs")
                status = exchange(exchanges[0], "status", {"response_mode": "full", "include_status": True})
                answer = exchange(exchanges[1], "answer", dict(template, question=tasks[key]["question"]))
                require(status == {"refused": False, "body": evidence["scope"]} and answer == evidence["answer"],
                        "question evidence differs from original delivered result")
                for record, call in zip(probes, exchanges[2:], strict=True):
                    p = record["probe"]
                    require(exchange(call, p["tool"], p["arguments"]) == record["result"],
                            "counterfactual result differs from delivered bytes")
                bound = window_binding(binding, grant, status["body"], fingerprints)
                total += len(probes)
                searches += observed_searches(evidence, binding.publication, pipeline)
                completed = (evidence, bound)
                exchanges.clear()
                captures.clear()
            elif kind == "window_release":
                require(completed is not None and grant is not None and event == {"kind":"window_release",
                        "question_id":grant["question_id"], "result":{"status":"scope_released"}},
                        "window release differs from completed question")
                processed.append(grant["question_id"])
                yield completed
                completed = grant = None
            elif kind == "phase_complete":
                require(not closed and grant is None and completed is None and event == {"kind": "phase_complete", "phase": phase, "ids": expected,
                        "instance": closure["instance"], "publication": binding.publication} and processed == expected,
                        "phase close differs")
                closed = True
        require(closed and grant is None and completed is None and not exchanges and not captures, "missing close or unassociated native observations")
    require(searches == census["pipeline_searches"] and (lifetime_fingerprints is not None or searches > 0), "incomplete original Search census")
    require(total == census["probes"] and observed_names == {p.name for p in
            (root / "fixture/context-cal-admissions").glob("record-*.json")}, "capture omitted observations")
