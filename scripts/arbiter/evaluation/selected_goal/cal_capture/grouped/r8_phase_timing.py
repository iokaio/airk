# SPDX-License-Identifier: Apache-2.0
"""Own one fresh-service, timing-only replay of original R7 phase 11."""

import argparse
from collections import Counter
import hashlib
import json
import os
from pathlib import Path
import runpy
import sys
import time

HERE = Path(__file__).resolve().parent
sys.path.insert(0, str(HERE))
import two_lifetimes as grouped
from controller import summary
from r6_capture import host_admission
from r6_candidate import checked_epoch, replace_bytes
import r7_handoff
import r8_predecessor as predecessor

OUTPUT = predecessor.CAMPAIGN / "cal100-r8-phase11-isolated-timing-r1"
WORK = OUTPUT / "timing-run"
PLAN = OUTPUT / "timing-plan.json"
OWNER_ENV = "AIRK_R8_PHASE11_TIMING_OWNER"
PHASE, QUESTIONS, PROBES = 11, 8, 40
EXCHANGES = {"answer": 8, "status": 8, "context": 11, "search": 29}
PHASE_MS, CALL_MS, DELIVERY_MS = 1_800_000, 120_000, 5_000
PLAN_SCHEMA = "airk.private.cal100-phase11-timing-plan/1"
CLOSURE_SCHEMA = "airk.private.cal100-phase11-timing-closure/1"
CLOSURE = WORK / "phase-11-timing-closure.json"
ORIGINAL_PHASE = (predecessor.LAST / "phase-11.owned.json",
                  "6a862ca0ce38272589a28d2d33921d6b69ae17532f3f8147384298aa519bcdca")
ORIGINAL_TRANSCRIPT = (predecessor.LAST / "phase-11.jsonl",
                       "34a859037751311db231ab3b6498e11770307818d0fb6b99fece7f4e542eabe3")
OBSERVED_CONTROL = {
    "http-1/endpoint.json": "6a7401db36abf800cd8ad3ea3e40abf0c6f6495d63f637c530ba4580930c91b3",
    "logs-1.4/current.jsonl": "3bdb2c91308f89e1715820a835e1cbcedcac9a75bfb8dda825b89b2e240d1aea",
    "service-1.4.sqlite3": "bb6e0347d930868c550de405185cdac7e750c3308e40e09f6fb353f9bb32f7b6",
}
require = grouped.require


def settled(owner):
    return (owner["exit_code"] == 0 and owner["child_assigned_before_resume"] is True
            and owner["owned_descendants_settled"] is True and owner["timed_out"] is False
            and owner["log_oversized"] is False and owner["owned_job_forced_termination"] is False
            and owner["owned_job_final_active_processes"] == 0 and owner["owned_job_final_member_pids"] == [])


def diagnostic_group(source):
    return replace_bytes(
        source,
        'out = output/"lifetime-closure.json"\n    native.write_json(out, {"schema":"airk.private.cal100-group-lifetime/1",',
        'out = output/"phase-11-timing-closure.json"\n    native.write_json(out, {"schema":"' + CLOSURE_SCHEMA + '",',
    )


def local_sources(epoch, *, create=False):
    ordered = r7_handoff.snapshots(epoch, OUTPUT / "candidate-sources", grouped.pin, create)
    original = Path(ordered["group_service.py"]["path"])
    source = diagnostic_group(original.read_bytes())
    target = OUTPUT / "timing_group_service.py"
    if create:
        with target.open("xb") as file:
            file.write(source)
    return ordered | {"timing_group_service.py": grouped.pin(target, hashlib.sha256(source).hexdigest())}


def stopped_installation(proof):
    # The launcher already holds the mutex; inventory itself does not acquire it.
    before = grouped.linked(proof["receipts"]["control"])
    control = predecessor.ANCHOR / "control"
    require(control.resolve(strict=True) == control and before["control_root"] == str(control),
            "different original stopped CAL installation")

    def seal(path):
        require(path.is_file() and path.stat().st_nlink == 1, "nonordinary CAL control entry")
        return grouped.link(path)

    current = r7_handoff.inventory(control, before, seal)
    require({name: current["files"][name] for name in OBSERVED_CONTROL} == OBSERVED_CONTROL,
            "stopped control bytes differ from the verified R7 successor observation")
    return current


def exchange_roster(data):
    calls = []
    for line in data.splitlines():
        event = json.loads(line)
        if event["kind"] == "native_exchange":
            argv = event["argv"]
            require(len(argv) == 7 and argv[:2] == ["tools", "call"]
                    and argv[3] == "--args" and argv[5:] == ["--output", "json"],
                    "original timing exchange arguments differ")
            args = json.loads(argv[4])
            require(isinstance(args, dict), "expected native argument object")
            calls.append((argv[2], args))
    return calls


def roster_digest(calls):
    require(Counter(tool for tool, _ in calls) == EXCHANGES, "phase 11 native exchange census differs")
    return hashlib.sha256(json.dumps(calls, sort_keys=True, separators=(",", ":"), allow_nan=False).encode()).hexdigest()


def original_phase(original, proof):
    ref = grouped.pin(*ORIGINAL_PHASE)
    phase = grouped.linked(ref)
    transcript = grouped.pin(*ORIGINAL_TRANSCRIPT)
    ready = grouped.linked(proof["receipts"]["final_ready"])
    publication = grouped.linked(proof["scope_baseline"])["bindings"][0]["publication"]
    fingerprints = set()
    require(settled(phase) and phase["phase"] == PHASE and phase["transcript"] == transcript,
            "different historical R7 phase 11")
    probes, _ = summary(ORIGINAL_TRANSCRIPT[0], PHASE, original["phase_ids"][PHASE],
                        ready["health"]["instance"], publication, fingerprints)
    require(probes == PROBES and len(fingerprints) == QUESTIONS,
            "original phase 11 question or probe census differs")
    calls = exchange_roster(ORIGINAL_TRANSCRIPT[0].read_bytes())
    return ref, roster_digest(calls)


def bind_plan(original, proof, epoch, sources, phase, roster, node, control, mutex, start):
    require(epoch == {key: original["candidate_service"][key] for key in epoch}
            and epoch["audit"] == original["input_audit"],
            "timing replay requires the exact original candidate epoch")
    return {
        "schema": PLAN_SCHEMA, "start_ms": start, "deadline_ms": start + PHASE_MS,
        "requested_call_ms": CALL_MS, "delivery_grace_ms": DELIVERY_MS, "node": node,
        "phase_groups": [[PHASE]], "phase_ids": original["phase_ids"], "recorder": original["recorder"],
        "installation": original["installation"] | {"status": proof["native_restart"]["status"]},
        "scope_baseline": proof["scope_baseline"], "input_audit": original["input_audit"],
        "candidate_service": epoch, "candidate_sources": sources,
        "original_phase": phase, "ordered_exchange_sha256": roster,
        "original_plan": proof["receipts"]["plan"], "predecessor": proof,
        "installation_mutex": mutex, "control_before": control,
        "sources": [*original["sources"], *sources.values(), grouped.link(Path(__file__).resolve()),
                    grouped.link(Path(predecessor.__file__).resolve())],
        "calibration_fitted": False,
    }


def validate_plan(plan, original, proof):
    require(plan["schema"] == PLAN_SCHEMA and plan["phase_groups"] == [[PHASE]]
            and plan["phase_ids"] == original["phase_ids"] and len(plan["phase_ids"][PHASE]) == QUESTIONS
            and plan["deadline_ms"] - plan["start_ms"] == PHASE_MS
            and plan["requested_call_ms"] == CALL_MS and plan["delivery_grace_ms"] == DELIVERY_MS
            and plan["recorder"] == original["recorder"] and plan["input_audit"] == original["input_audit"]
            and plan["installation"] == original["installation"] | {"status": proof["native_restart"]["status"]}
            and plan["scope_baseline"] == proof["scope_baseline"]
            and plan["original_plan"] == proof["receipts"]["plan"]
            and plan["calibration_fitted"] is False and proof["capture_complete"] is False,
            "timing plan changed the bounded phase or R7 provenance")


def pin_references(refs):
    for ref in refs:
        require(grouped.pin(ref["path"], ref["sha256"]) == ref, "timing source or input identity differs")


def validate_result(plan, ref, result, phase, fingerprints, prior_instances, instances):
    require(Path(ref["path"]) == CLOSURE and result["schema"] == CLOSURE_SCHEMA
            and result["group"] == 0 and result["phases"] == [PHASE] and result["probes"] == PROBES
            and len(result["phase_closures"]) == 1 and len(fingerprints) == QUESTIONS
            and instances - prior_instances == {result["instance"]} and prior_instances < instances
            and settled(phase) and phase["phase"] == PHASE
            and plan["start_ms"] <= result["start_ms"] == phase["start_ms"] < phase["end_ms"]
            <= result["end_ms"] <= phase["deadline_ms"] <= plan["deadline_ms"]
            and phase["deadline_ms"] - phase["start_ms"] <= PHASE_MS
            and result["dispatch"]["context_dispatches"] == 0,
            "timing phase lacks its original controls or natural settlement")


def execute(plan_ref, node_ref):
    plan = grouped.linked(plan_ref)
    grouped.require_parent_owns(grouped.linked(plan["installation_mutex"]), os.getppid())
    proof = plan["predecessor"]
    original = grouped.linked(plan["original_plan"])
    validate_plan(plan, original, proof)
    require(plan["node"] == node_ref
            and plan["candidate_sources"] == local_sources(plan["candidate_service"])
            and plan["original_phase"] == grouped.pin(*ORIGINAL_PHASE),
            "timing child differs from its frozen owner plan")
    grouped.pin(*ORIGINAL_TRANSCRIPT)
    deadline = time.monotonic() + (plan["deadline_ms"] - grouped.now_ms()) / 1000
    require(time.monotonic() + (CALL_MS + DELIVERY_MS) / 1000 < deadline,
            "timing phase cannot admit an original operation")
    audit = grouped.linked(plan["input_audit"])
    require(audit["inputs"]["service"] == plan["candidate_service"]["image"], "candidate image differs from audit")
    pin_references([*plan["sources"], *audit["inputs"].values(), *audit["compiled_sdk"].values(), audit["census"]])
    sources_before = grouped.check_cal_sources()
    grouped.previous.effective_limit(grouped.previous.NATIVE, Path(audit["inputs"]["service"]["path"]))
    WORK.mkdir()
    prior = {grouped.linked(ref)["health"]["instance"] for ref in
             (plan["installation"]["ready"], original["restart_handoff"]["ready"],
              proof["receipts"]["first_ready"], proof["receipts"]["final_ready"])}
    require(len(prior) == 4, "retired native instances are not distinct")
    instances, fingerprints = set(prior), set()
    owner = plan["candidate_sources"]["timing_group_service.py"]
    pin_references([owner])
    run_group = runpy.run_path(owner["path"])["run_group"]
    result_ref = run_group(WORK, 0, plan, plan_ref, plan["installation"]["closed"], audit,
                           Path(node_ref["path"]), deadline, fingerprints, instances)
    result = grouped.linked(result_ref)
    phase = grouped.linked(result["phase_closures"][0])
    validate_result(plan, result_ref, result, phase, fingerprints, prior, instances)
    historical = exchange_roster(ORIGINAL_TRANSCRIPT[0].read_bytes())
    diagnostic = exchange_roster(Path(phase["transcript"]["path"]).read_bytes())
    require(roster_digest(historical) == plan["ordered_exchange_sha256"]
            and diagnostic == historical, "timing phase changed the ordered native operations or source arguments")
    pin_references(plan["sources"])
    require(grouped.check_cal_sources() == sources_before and time.monotonic() <= deadline,
            "timing changed original sources or exceeded its deadline")


def launch(node_ref):
    require(not any(OUTPUT.iterdir()), "fresh timing output required")
    acquired, terminal = (OUTPUT / name for name in ("control-mutex-acquired.json", "outer-owned.json"))
    lock = grouped.InstallationMutex()
    lock.__enter__()
    try:
        grouped.native.write_json(acquired, {"name": grouped.MUTEX_NAME, "owner_pid": os.getpid(),
                                             "at_ms": grouped.now_ms()})
        grouped.native.write_json(OUTPUT / "host-admission.json", host_admission())
        proof = predecessor.observe()
        original = grouped.linked(proof["receipts"]["plan"])
        current = stopped_installation(proof)
        control = OUTPUT / "control-before.json"
        grouped.native.write_json(control, current)
        epoch = checked_epoch(original["candidate_service"]["provenance"]["sha256"], grouped.pin)
        sources = local_sources(epoch, create=True)
        phase, roster = original_phase(original, proof)
        plan = bind_plan(original, proof, epoch, sources, phase, roster, node_ref, grouped.link(control),
                         grouped.link(acquired), grouped.now_ms())
        validate_plan(plan, original, proof)
        grouped.native.write_json(PLAN, plan)
        owner = grouped.owned_module()
        argv = [sys.executable, "-I", "-B", "-S", Path(__file__).resolve(), "--output", OUTPUT,
                "--node", node_ref["path"], "--owned-plan-sha256", grouped.link(PLAN)["sha256"]]
        remaining = (plan["deadline_ms"] - grouped.now_ms()) / 1000
        require((CALL_MS + DELIVERY_MS) / 1000 < remaining <= PHASE_MS / 1000, "invalid owned timing interval")
        outer = owner.run_evaluation_fixture(argv, grouped.ROOT, OUTPUT / "outer.log", remaining,
                                             os.environ | {OWNER_ENV: str(OUTPUT)})
        grouped.native.write_json(terminal, outer)
    finally:
        lock.__exit__(None, None, None)
        if acquired.is_file():
            grouped.native.write_json(OUTPUT / "control-mutex-released.json", {
                "name": grouped.MUTEX_NAME, "owner_pid": os.getpid(), "at_ms": grouped.now_ms(),
                "acquired": grouped.link(acquired), "outer": grouped.link(terminal) if terminal.is_file() else None,
            })
    require(settled(outer) and CLOSURE.is_file(), "timing study did not close naturally")
    closure_ref = grouped.link(CLOSURE)
    closure = grouped.linked(closure_ref)
    old = grouped.linked(plan["original_phase"])
    phase_ref, = closure["phase_closures"]
    new = grouped.linked(phase_ref)
    return {
        "schema": "airk.private.cal100-phase11-timing-observation/1", "phase": PHASE, "ordered_questions": QUESTIONS,
        "probes": PROBES, "exchanges": EXCHANGES, "ordered_exchange_sha256": plan["ordered_exchange_sha256"],
        "diagnostic_only": True, "capture_complete": False, "calibration_fitted": False,
        "warm_state_equivalence_established": False, "plan": grouped.link(PLAN), "outer": grouped.link(terminal),
        "native_closure": closure_ref, "release": grouped.link(OUTPUT / "control-mutex-released.json"),
        "original_phase": plan["original_phase"], "diagnostic_phase": phase_ref,
        "original_worker_elapsed_seconds": old["elapsed_seconds"], "diagnostic_worker_elapsed_seconds": new["elapsed_seconds"],
        "diagnostic_native_elapsed_ms": closure["end_ms"] - closure["start_ms"],
        "diagnostic_start_to_close_ms": closure["end_ms"] - plan["start_ms"],
    }


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("--output", type=Path, required=True)
    parser.add_argument("--node", type=Path, required=True)
    parser.add_argument("--owned-plan-sha256")
    args = parser.parse_args()
    require(os.name == "nt" and sys.flags.isolated and sys.flags.no_site and sys.dont_write_bytecode
            and args.output == OUTPUT and args.output.resolve(strict=True) == OUTPUT,
            "checked isolated Windows timing output required")
    node = grouped.pin(args.node, grouped.NODE_SHA)
    if args.owned_plan_sha256:
        require(os.environ.get(OWNER_ENV) == str(OUTPUT), "timing fixture is missing its outer Job owner")
        with grouped.native.capped_worker_job(OUTPUT):
            execute(grouped.pin(PLAN, args.owned_plan_sha256), node)
        return
    print(json.dumps(launch(node), allow_nan=False, sort_keys=True))


if __name__ == "__main__":
    main()
