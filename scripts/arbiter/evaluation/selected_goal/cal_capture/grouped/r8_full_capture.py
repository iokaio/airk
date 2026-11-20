# SPDX-License-Identifier: Apache-2.0
"""Run the complete original CAL100 in a new isolated Windows capture."""

import argparse
import json
import os
from pathlib import Path
import runpy
import sys
import time

HERE = Path(__file__).resolve().parent
sys.path.insert(0, str(HERE))
import two_lifetimes as grouped
from r6_candidate import checked_epoch
from r6_capture import host_admission, require_admission
from r6_preflight import DEFAULT_MS, common_deadline
import r7_handoff
from r8_phase_timing import settled
import r8_predecessor as predecessor
import r8_timing_handoff as timing

OUTPUT = predecessor.CAMPAIGN / "cal100-focus1024-capture-r8"
PLAN = OUTPUT / "lifetime-plan.json"
CLOSURE = OUTPUT / "fixture-closure.json"
OWNER_ENV = "AIRK_R8_CAL_OWNER"
PLAN_SCHEMA = "airk.private.cal100-r8-two-lifetime-plan/1"
CLOSURE_SCHEMA = "airk.private.cal100-r8-two-lifetime-fixture/1"
require = grouped.require


def original_plan():
    path, digest = predecessor.PINS["plan"]
    ref = grouped.pin(predecessor.R7 / path, digest)
    return ref, grouped.linked(ref)


def checked_candidate(original, *, create=False):
    baseline = original["candidate_service"]
    epoch = checked_epoch(baseline["provenance"]["sha256"], grouped.pin)
    require(epoch == {key: value for key, value in baseline.items() if key != "sources"}
            and epoch["audit"] == original["input_audit"], "original candidate epoch changed")
    snapshots = r7_handoff.snapshots(epoch, OUTPUT / "candidate-sources", grouped.pin, create)
    return epoch | {"sources": snapshots}


def make_plan(start, mutex, host, control, handoff, *, create=False):
    original_ref, original = original_plan()
    candidate = checked_candidate(original, create=create)
    sources = [*original["sources"], *candidate["sources"].values(),
               grouped.link(Path(__file__).resolve()), grouped.link(Path(timing.__file__).resolve()),
               grouped.link(Path(predecessor.__file__).resolve())]
    return original | {
        "schema": PLAN_SCHEMA, "start_ms": start,
        "deadline_ms": common_deadline(start, DEFAULT_MS, predecessor.SETUP_MS),
        "installation_mutex": mutex, "host_admission": host, "predecessor_control": control,
        "original_r7_plan": original_ref, "restart_handoff": handoff,
        "installation": original["installation"] | {"status": handoff["native_restart"]["status"]},
        "scope_baseline": handoff["scope_baseline"], "candidate_service": candidate, "sources": sources,
    }


def validate_clock_and_census(plan):
    rows = plan["phase_ids"]
    require(plan["schema"] == PLAN_SCHEMA and plan["common_ms"] == DEFAULT_MS
            and plan["setup_elapsed_ms"] == predecessor.SETUP_MS
            and plan["deadline_ms"] == common_deadline(plan["start_ms"], DEFAULT_MS, predecessor.SETUP_MS)
            and plan["phase_groups"] == [list(group) for group in grouped.PHASE_GROUPS]
            and [len(row) for row in rows] == [8] * 12 + [4]
            and len({key for row in rows for key in row}) == 100,
            "original CAL clock or population changed")


def verify_sources(refs):
    for ref in refs:
        require(grouped.pin(ref["path"], ref["sha256"]) == ref, "capture source or input pin differs")


def execute(plan_ref, node_ref):
    plan = grouped.linked(plan_ref)
    mutex = grouped.linked(plan["installation_mutex"])
    grouped.require_parent_owns(mutex, os.getppid())
    validate_clock_and_census(plan)
    handoff = plan["restart_handoff"]
    timing.verify_frozen(handoff)
    require(grouped.linked(plan["predecessor_control"]) == handoff["control_before"]
            and make_plan(plan["start_ms"], plan["installation_mutex"], plan["host_admission"],
                          plan["predecessor_control"], handoff) == plan,
            "capture plan differs from the frozen original and latest retirement")
    deadline = time.monotonic() + (plan["deadline_ms"] - grouped.now_ms()) / 1000
    require_admission(deadline)
    audit = grouped.linked(plan["input_audit"])
    verify_sources([*audit["inputs"].values(), *audit["compiled_sdk"].values(), audit["census"]])
    source_tree = grouped.check_cal_sources()
    grouped.previous.effective_limit(grouped.previous.NATIVE, Path(audit["inputs"]["service"]["path"]))
    group_pin = plan["candidate_service"]["sources"]["group_service.py"]
    verify_sources([group_pin])
    group_owner = runpy.run_path(group_pin["path"])["run_group"]

    install = plan["installation"]
    publication = grouped.linked(plan["scope_baseline"])["bindings"][0]["publication"]
    prior = install["closed"]
    instances = set(handoff["retired_instances"])
    retired_count = len(instances)
    fingerprints, groups = set(), []
    probes = searches = 0
    for group in range(len(grouped.PHASE_GROUPS)):
        require_admission(deadline)
        grouped.require_parent_owns(mutex, os.getppid())
        verify_sources([plan_ref, *plan["sources"]])
        work = OUTPUT / f"lifetime-{group:02}"
        work.mkdir()
        closed = group_owner(work, group, plan, plan_ref, prior, audit, Path(node_ref["path"]),
                             deadline, fingerprints, instances)
        row = grouped.verify_group(closed, plan_ref, prior, group, publication)
        groups.append(closed)
        prior = closed
        probes += row["probes"]
        searches += row["pipeline_searches"]

    require(len(instances) == retired_count + len(grouped.PHASE_GROUPS) and len(fingerprints) == 100
            and probes <= 500 and searches > 0 and grouped.check_cal_sources() == source_tree,
            "original capture census or source custody differs")
    verify_sources([plan_ref, *plan["sources"]])
    elapsed = grouped.now_ms() - plan["start_ms"] + plan["setup_elapsed_ms"]
    require(elapsed <= plan["common_ms"] and time.monotonic() < deadline,
            "capture exceeded its common clock")
    grouped.native.write_json(CLOSURE, {
        "schema": CLOSURE_SCHEMA, "plan": plan_ref, "lifetimes": groups, "publication": publication,
        "ordered_questions": 100, "probes": probes, "pipeline_searches": searches, "elapsed_ms": elapsed,
    })


def launch(node_ref):
    require(not any(OUTPUT.iterdir()), "fresh calibration output required")
    acquired = OUTPUT / "control-mutex-acquired.json"
    terminal = OUTPUT / "outer-owned.json"
    host = OUTPUT / "host-admission.json"
    control = OUTPUT / "predecessor-control.json"
    lock = grouped.InstallationMutex()
    lock.__enter__()
    try:
        start = grouped.now_ms()
        grouped.native.write_json(acquired, {"name": grouped.MUTEX_NAME, "owner_pid": os.getpid(), "at_ms": start})
        grouped.native.write_json(host, host_admission())
        handoff = timing.observe()
        grouped.native.write_json(control, handoff["control_before"])
        plan = make_plan(start, grouped.link(acquired), grouped.link(host), grouped.link(control), handoff, create=True)
        validate_clock_and_census(plan)
        grouped.native.write_json(PLAN, plan)
        plan_ref = grouped.link(PLAN)
        owner = grouped.owned_module()
        argv = [sys.executable, "-I", "-B", "-S", Path(__file__).resolve(), "--output", OUTPUT,
                "--node", node_ref["path"], "--owned-plan-sha256", plan_ref["sha256"]]
        remaining = (plan["deadline_ms"] - grouped.now_ms()) / 1000
        require(remaining > 125, "insufficient original calibration operation window")
        result = owner.run_evaluation_fixture(argv, grouped.ROOT, OUTPUT / "outer.log", remaining,
                                              os.environ | {OWNER_ENV: str(OUTPUT)})
        grouped.native.write_json(terminal, result)
    finally:
        lock.__exit__(None, None, None)
        if acquired.is_file():
            grouped.native.write_json(OUTPUT / "control-mutex-released.json", {
                "name": grouped.MUTEX_NAME, "owner_pid": os.getpid(), "at_ms": grouped.now_ms(),
                "acquired": grouped.link(acquired), "outer": grouped.link(terminal) if terminal.is_file() else None,
            })
    require(settled(result) and CLOSURE.is_file(), "calibration attempt incomplete; original partial evidence retained")
    closure_ref = grouped.link(CLOSURE)
    closure = grouped.linked(closure_ref)
    require(closure["schema"] == CLOSURE_SCHEMA and closure["ordered_questions"] == 100
            and len(closure["lifetimes"]) == len(grouped.PHASE_GROUPS), "native fixture closure differs")
    return {"ordered_questions": 100, "plan": plan_ref, "fixture": closure_ref, "outer": grouped.link(terminal),
            "release": grouped.link(OUTPUT / "control-mutex-released.json")}


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("--output", type=Path, required=True)
    parser.add_argument("--node", type=Path, required=True)
    parser.add_argument("--owned-plan-sha256")
    args = parser.parse_args()
    require(os.name == "nt" and sys.flags.isolated and sys.flags.no_site and sys.dont_write_bytecode
            and args.output == OUTPUT and args.output.resolve(strict=True) == OUTPUT,
            "checked Windows calibration output required")
    node = grouped.pin(args.node, grouped.NODE_SHA)
    if args.owned_plan_sha256:
        require(os.environ.get(OWNER_ENV) == str(OUTPUT), "full fixture requires its outer Job owner")
        with grouped.native.capped_worker_job(OUTPUT):
            execute(grouped.pin(PLAN, args.owned_plan_sha256), node)
        return
    print(json.dumps(launch(node), allow_nan=False, sort_keys=True))


if __name__ == "__main__":
    main()
