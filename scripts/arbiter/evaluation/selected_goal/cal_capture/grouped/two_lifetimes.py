# SPDX-License-Identifier: Apache-2.0
"""Run the frozen CAL100 in two measured recorder groups after an incomplete R4."""
import argparse
import os
from pathlib import Path
import sys
import time

import run_grouped as previous
from controller import AUDIT_PATH, AUDIT_SHA, CAMPAIGN, NODE_SHA, ROOT, link, owned_module, require
from dispatch_proof import read_journal
from group_service import now_ms, run_group
from installation_mutex import InstallationMutex, MUTEX_NAME, require_parent_owns
from launch import check_cal_sources, native
from lifetime import linked
from prepare import pin, read

PHASE_GROUPS = (tuple(range(7)),tuple(range(7,13)))
PLAN_SCHEMA = "airk.private.cal100-two-lifetime-plan/1"
CLOSURE_SCHEMA = "airk.private.cal100-two-lifetime-fixture/1"


def capacity_bound(counts):
    require(len(counts) == 11 and all(type(v) is int and 0 <= v <= 8*28 for v in counts),
            "expected the first eleven complete original phase admission counts")
    observed = [sum(counts[:7]),sum(counts[7:])]
    upper = [observed[0],observed[1]+12*28]
    require(observed == [960,584] and max(upper) <= 1024,
            "R4 original measured counts or remaining conservative allowance differs")
    return observed, upper


def capacity_basis(focus):
    refs, counts = [], []
    for phase in range(11):
        work = previous.R4/f"lifetime-{phase:02}"
        ref = link(work/"lifetime-closure.json")
        closure = linked(ref)
        require(closure["schema"] == "airk.private.cal100-phase-lifetime/1" and closure["phase"] == phase and
                linked(closure["closed"])["service_lease_reacquired"] is True,
                "capacity evidence is not an original settled native phase")
        actual = read_journal(work/"fixture",focus,setup_only=False)
        require(actual == closure["dispatch"],"original complete journal and published capacity evidence differ")
        refs.append(ref)
        counts.append(actual["focus_dispatches"])
    observed, upper = capacity_bound(counts)
    return {"phase_closures":refs,"observed":observed,"remaining":12,"per_question_upper":28,"group_upper":upper}


def make_plan(start,prior,mutex):
    plan = previous.make_plan(start,prior,mutex)
    groups = [list(group) for group in PHASE_GROUPS]
    require([sum(len(plan["phase_ids"][n]) for n in group) for group in groups] == [56,44],
            "original phase or population budget changed")
    audit = read(AUDIT_PATH,AUDIT_SHA)
    plan.update(schema=PLAN_SCHEMA,phase_groups=groups,
                capacity_basis=capacity_basis(audit["profiles"]["arbiter-focus-qwen3-reranker-06b"]))
    plan["sources"].append(link(Path(__file__).resolve()))
    return plan


def verify_group(ref,plan_ref,prior,group,publication):
    row = linked(ref)
    require(row["schema"] == "airk.private.cal100-group-lifetime/1" and row["group"] == group and
            row["prior"] == prior and row["plan"] == plan_ref and row["publication"] == publication and
            row["phases"] == list(PHASE_GROUPS[group]),"original grouped closure differs")
    phases = [linked(ref) for ref in row["phase_closures"]]
    require([p["phase"] for p in phases] == row["phases"] and row["start_ms"] == phases[0]["start_ms"] and
            row["end_ms"] <= phases[-1]["deadline_ms"] and
            all(p["start_ms"] <= p["end_ms"] <= p["deadline_ms"] <= p["start_ms"]+1800000 for p in phases) and
            all(a["end_ms"] <= b["start_ms"] for a,b in zip(phases,phases[1:])),
            "original phases, grouping or elapsed allowance differ")
    return row


def execute(output,node,plan_ref):
    plan = linked(plan_ref)
    require(plan["prior_attempt"] is not None and plan["installation_mutex"] is not None,
            "original incomplete R4 and exclusive installation required")
    mutex = linked(plan["installation_mutex"])
    require_parent_owns(mutex,os.getppid())
    require(make_plan(plan["start_ms"],plan["prior_attempt"],plan["installation_mutex"]) == plan,
            "predeclared original roster and source identities differ")
    audit = read(AUDIT_PATH,AUDIT_SHA)
    for ref in [*audit["inputs"].values(),*audit["compiled_sdk"].values()]:
        pin(ref["path"],ref["sha256"])
    original_sources = check_cal_sources()
    deadline = time.monotonic()+(plan["deadline_ms"]-now_ms())/1000
    install = plan["installation"]
    pub = linked(install["bindings"])["bindings"][0]["publication"]
    prior = install["closed"]
    instances = {linked(install["ready"])["health"]["instance"]}
    fingerprints, closures = set(), []
    probes = searches = 0
    for group in range(len(PHASE_GROUPS)):
        require(time.monotonic()+125 < deadline,"insufficient original time for next service")
        require_parent_owns(mutex,os.getppid())
        pin(plan_ref["path"],plan_ref["sha256"])
        for ref in plan["sources"]:
            pin(ref["path"],ref["sha256"])
        folder = output/f"lifetime-{group:02}"
        folder.mkdir()
        closed = run_group(folder,group,plan,plan_ref,prior,audit,node,deadline,fingerprints,instances)
        row = verify_group(closed,plan_ref,prior,group,pub)
        closures.append(closed)
        prior = closed
        probes += row["probes"]
        searches += row["pipeline_searches"]
    require(len(instances) == 3 and len(fingerprints) == 100 and probes <= 500 and searches > 0 and
            check_cal_sources() == original_sources,"incomplete 100-question original census")
    for ref in plan["sources"]:
        pin(ref["path"],ref["sha256"])
    elapsed = now_ms()-plan["start_ms"]+plan["setup_elapsed_ms"]
    require(elapsed <= 10800000 and time.monotonic() < deadline,"original campaign clock exceeded")
    native.write_json(output/"fixture-closure.json",{"schema":CLOSURE_SCHEMA,"plan":plan_ref,"lifetimes":closures,
                      "publication":pub,"ordered_questions":100,"probes":probes,"pipeline_searches":searches,
                      "elapsed_ms":elapsed})


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("--output",type=Path,required=True)
    parser.add_argument("--node",type=Path,required=True)
    parser.add_argument("--prior-outer-sha256",required=True)
    parser.add_argument("--owned-plan-sha256")
    args = parser.parse_args()
    out = args.output
    require(os.name == "nt" and sys.flags.no_site and sys.dont_write_bytecode and out.parent == CAMPAIGN and
            out.resolve(strict=True) == out and out.name == "cal100-focus1024-capture-r5", "fresh checked R5 required")
    pin(args.node,NODE_SHA)
    prior = pin(previous.R4/"outer-owned.json",args.prior_outer_sha256)
    path = out/"lifetime-plan.json"
    if args.owned_plan_sha256:
        require(os.environ.get("AIRK_TWO_CAL_OWNER") == str(out), "original two-lifetime owner required")
        with native.capped_worker_job(out):
            execute(out,args.node,pin(path,args.owned_plan_sha256))
        return
    require(not any(out.iterdir()),"fresh R5 output required")
    lock = InstallationMutex()
    lock.__enter__()
    acquired = out/"control-mutex-acquired.json"
    terminal = out/"outer-owned.json"
    try:
        start = now_ms()
        native.write_json(acquired,{"name":MUTEX_NAME,"owner_pid":os.getpid(),"at_ms":start})
        plan = make_plan(start,prior,link(acquired))
        native.write_json(path,plan)
        argv = [sys.executable,"-B","-S",Path(__file__).resolve(),"--output",out,"--node",args.node,
                "--prior-outer-sha256",args.prior_outer_sha256,"--owned-plan-sha256",link(path)["sha256"]]
        owner = owned_module()
        result = owner.run_evaluation_fixture(argv,ROOT,out/"outer.log",(plan["deadline_ms"]-now_ms())/1000,
                                              os.environ|{"AIRK_TWO_CAL_OWNER":str(out)})
        native.write_json(terminal,result)
    finally:
        lock.__exit__(None,None,None)
        if acquired.is_file():
            native.write_json(out/"control-mutex-released.json",{"name":MUTEX_NAME,"owner_pid":os.getpid(),
                              "at_ms":now_ms(),"acquired":link(acquired),"outer":link(terminal) if terminal.is_file() else None})
    require(owner.successful(result) and result["owned_job_final_active_processes"] == 0 and
            result["owned_job_final_member_pids"] == [] and (out/"fixture-closure.json").is_file(),
            "two-lifetime capture incomplete; partial evidence retained")


if __name__ == "__main__":
    main()
