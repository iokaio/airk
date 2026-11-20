# SPDX-License-Identifier: Apache-2.0
"""Predeclare the CAL100 phases and execute each once on the same installation."""
import argparse
import math
import os
from pathlib import Path
import sys
import time

sys.path.insert(0, str(Path(__file__).resolve().parents[1]))
from controller import AUDIT_PATH, AUDIT_SHA, CAMPAIGN, HERE, NODE_SHA, ROOT, link, owned_module, require
from dispatch_proof import read_journal
from launch import check_cal_sources, native
from prepare import pin, read
from lifetime import linked, run, warm_closure

ANCHOR = CAMPAIGN / "cal100-focus1024-capture-r3"
ANCHOR_PINS = {
    "ready": ("fixture/ready.json","a977a203165f52a8ff42beb570bee46b6b4cfab609087d8f6120ee21252fd71f"),
    "closed": ("fixture/closed.json","c3feab1c99a4837c2fecc95e571e960474b35b39487f84c0c1d8f9ffd74ec992"),
    "status": ("commands/cal-ready.stdout","c2b6d1f4d31ca023058db4a0ddf4149d231e733cd2fcad881b64138416960dfa"),
    "terminal": ("outer-owned.json","58ae17e9ae259ea6752b50a445138dfadd324131f10b8d0a6dc419349f617031"),
}


def make_plan(start_ms):
    audit = read(AUDIT_PATH, AUDIT_SHA)
    installation = {k: pin(ANCHOR/path, sha) for k, (path, sha) in ANCHOR_PINS.items()}
    for key, name in {"bindings":"fixture/enable-context.bindings.json","start":"fixture-start.json",
                      "request":"live-request.json", "dispatch_summary":"fixture/composed-observer.closed.json",
                      "warm_summary":"fixture/warm-completions.closed.json"}.items():
        installation[key] = link(ANCHOR/name)
    installation["custody"] = pin(CAMPAIGN/"cal100-focus1024-capture-r3-custody.json",
                                 "6fe3f0a9ef5d88c7d3ccda9791165ffd5d055252db8b95ac2ab6d91277ac4f1e")
    require(linked(installation["custody"])["changed_input_count"] == 0, "anchor source custody changed")
    ready, closed, status, terminal = (linked(installation[k]) for k in ("ready","closed","status","terminal"))
    require(ready["health"]["instance"] == closed["instance"] and closed["shutdown_completed"] is True and
            closed["service_lease_reacquired"] is True and terminal["owned_descendants_settled"] is True and
            terminal["parent_assigned_to_job"] is False and terminal["child_assigned_before_resume"] is True and
            terminal["owned_job_final_active_processes"] == 0 and terminal["owned_job_final_member_pids"] == [] and
            not terminal["timed_out"] and not terminal["owned_job_forced_termination"] and terminal["exit_code"] == 1,
            "R3 retirement or terminal ownership differs")
    require(ready["inputs_sha256"] == audit["inputs"]["inputs"]["sha256"] and
            ready["health"]["executable_sha256"] == audit["inputs"]["service"]["sha256"] and
            ready["control_root"] == closed["control_root"], "anchor installation identity differs")
    profile = audit["profiles"]["arbiter-focus-qwen3-reranker-06b"]
    require(status["focus"]["active_profile"] == profile and status["context"]["active_profile"] ==
            audit["profiles"]["arbiter-context-qwen3-17b"], "immutable original active profiles differ")
    read_journal(ANCHOR/"fixture", profile, setup_only=False)
    warm_closure(ANCHOR/"fixture", ready)
    census = linked(audit["census"])
    ids = census["question_ids"]
    require(len(ids) == len(set(ids)) == 100 and audit["phase_counts"] == [8]*12 + [4],
            "original CAL100 phase census changed")
    debit = math.ceil(1000 * terminal["elapsed_seconds"])
    require(0 < debit < 10800000, "unexpected installation cost")
    return {"schema":"airk.private.cal100-lifetime-plan/1", "input_audit":link(AUDIT_PATH), "census":audit["census"],
            "installation":installation, "phase_ids":[ids[i:i+8] for i in range(0,100,8)],
            "setup_elapsed_ms":debit, "start_ms":start_ms, "deadline_ms":start_ms+10800000-debit}


def execute(output, node, ref):
    plan = linked(ref)
    require(make_plan(plan["start_ms"]) == plan, "predeclared plan differs")
    audit = read(AUDIT_PATH, AUDIT_SHA)
    for entry in [*audit["inputs"].values(), *audit["compiled_sdk"].values()]:
        pin(entry["path"], entry["sha256"])
    sources = check_cal_sources()
    deadline = time.monotonic() + (plan["deadline_ms"]-time.time_ns()//1000000)/1000
    instances = {linked(plan["installation"]["ready"])["health"]["instance"]}
    fingerprints = set()
    prior = plan["installation"]["closed"]
    closures = []
    probes = searches = 0
    publication = linked(plan["installation"]["bindings"])["bindings"][0]["publication"]
    for phase in range(13):
        pin(ref["path"], ref["sha256"])
        require(time.monotonic()+125 < deadline, "insufficient original time for next phase")
        folder = output / f"lifetime-{phase:02}"
        folder.mkdir()
        prior = run(folder, phase, plan, ref, prior, audit, node, deadline, fingerprints, instances)
        result = linked(prior)
        require(result["publication"] == publication, "original publication changed")
        closures.append(prior)
        probes += result["probes"]
        searches += result["pipeline_searches"]
    require(len(closures) == 13 and len(instances) == 14 and len(fingerprints) == 100 and
            searches > 0 and probes <= 500 and check_cal_sources() == sources and time.monotonic() < deadline,
            "incomplete phase set, native census, original source or elapsed budget")
    elapsed = time.time_ns()//1000000 - plan["start_ms"] + plan["setup_elapsed_ms"]
    require(elapsed <= 10800000, "whole campaign elapsed budget exceeded")
    native.write_json(output/"fixture-closure.json", {"schema":"airk.private.cal100-phased-fixture/1",
                      "plan":ref, "lifetimes":closures, "publication":publication, "ordered_questions":100,
                      "probes":probes, "pipeline_searches":searches, "elapsed_ms":elapsed})


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("--output", required=True, type=Path)
    parser.add_argument("--node", required=True, type=Path)
    parser.add_argument("--owned-plan-sha256")
    args = parser.parse_args()
    output = args.output
    require(os.name == "nt" and sys.flags.no_site and sys.dont_write_bytecode and output.parent == CAMPAIGN and
            output.resolve(strict=True) == output and output.is_dir(), "checked Windows campaign directory required")
    pin(args.node, NODE_SHA)
    path = output / "lifetime-plan.json"
    if args.owned_plan_sha256:
        require(os.environ.get("AIRK_PHASED_CAL_OWNER") == str(output), "requires original enclosing Job")
        with native.capped_worker_job(output):
            execute(output, args.node, pin(path,args.owned_plan_sha256))
        return
    require(not any(output.iterdir()), "fresh predeclared campaign required")
    start = time.time_ns()//1000000
    plan = make_plan(start)
    native.write_json(path, plan)
    args = [sys.executable, "-B", "-S", Path(__file__).resolve(), "--output",output,"--node",args.node,
            "--owned-plan-sha256",link(path)["sha256"]]
    owner = owned_module()
    remaining = (plan["deadline_ms"]-time.time_ns()//1000000)/1000
    result = owner.run_evaluation_fixture(args,ROOT,output/"outer.log",remaining,os.environ|{"AIRK_PHASED_CAL_OWNER":str(output)})
    native.write_json(output/"outer-owned.json", result)
    require(owner.successful(result) and result["owned_job_final_active_processes"] == 0 and
            result["owned_job_final_member_pids"] == [] and (output/"fixture-closure.json").is_file(),
            "incomplete phased campaign; original closures and partials retained")


if __name__ == "__main__":
    main()
