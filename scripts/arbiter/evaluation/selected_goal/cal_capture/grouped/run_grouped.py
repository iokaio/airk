# SPDX-License-Identifier: Apache-2.0
"""Supervise the complete frozen CAL100 in three bounded recorder lifetimes."""
import argparse
import mmap
import os
from pathlib import Path
import sys
import time

PARENT = Path(__file__).resolve().parents[1]
sys.path.insert(0, str(PARENT))
sys.path.append(str(PARENT/"phased"))
from controller import AUDIT_PATH, AUDIT_SHA, CAMPAIGN, HERE, NODE_SHA, ROOT, link, owned_module, require
from launch import check_cal_sources, native
from lifetime import linked
from phased.capture import make_plan as original_plan
from group_service import now_ms, run_group
from installation_mutex import InstallationMutex, MUTEX_NAME, require_parent_owns
from prepare import pin, read

GROUPS = ((0,1,2,3),(4,5,6,7),(8,9,10,11,12))
NATIVE = ROOT/"crates/airk-service/src/service_runtime/evaluation_fixture_tests/execution_observer.rs"
NATIVE_SHA = "80e8ca821bf6828314f7c0a0c9e3932c6634d9c1e594a6c0f417a0115b9dc301"
R4 = CAMPAIGN/"cal100-focus1024-capture-r4"


def group_roster(phase_ids, groups):
    require(groups == [list(group) for group in GROUPS] and
            [len(ids) for ids in phase_ids] == [8]*12+[4] and
            len({key for ids in phase_ids for key in ids}) == 100 and
            [sum(len(phase_ids[p]) for p in group) for group in groups] == [32,32,36],
            "wrong predeclared lifetime or question roster")
    return groups


def effective_limit(source, image):
    text = source.read_text(encoding="utf-8")
    require(all(s in text for s in ("const MAX_RECORDS: usize = 8192;", "const MAX_BYTES: u64 = 64 << 20;",
                                    'dispatched<=1024,"private native dispatch census bound exceeded"')),
            "native recorder bounds changed")
    with image.open("rb") as stream, mmap.mmap(stream.fileno(),0,access=mmap.ACCESS_READ) as view:
        require(view.find(b"private native dispatch census bound exceeded") >= 0,
                "original service image lacks native refusal marker")


def source_refs():
    names = ("controller.py","prepare.py","phase.cjs","worker.cjs","window_scope.cjs","dispatch_proof.py","launch.py",
             "phased/capture.py","phased/lifetime.py","grouped/run_grouped.py","grouped/group_service.py","grouped/installation_mutex.py")
    return [link(HERE/name) for name in names] + [
        link(ROOT/"scripts/provision-static-onnx-runtime.py"),link(HERE.parents[1]/"run_compact_answer.py"),
        link(HERE.parent/"replay_cal_population.py"),link(ROOT/"scripts/arbiter/analysis/public_source_admission/native_cohort_source_reads.py")]


def make_plan(start_ms, prior, mutex=None):
    plan = original_plan(start_ms)
    audit = read(AUDIT_PATH,AUDIT_SHA)
    if prior is not None:
        require(Path(prior["path"]) == R4/"outer-owned.json" and not (R4/"fixture-closure.json").exists(),
                "R4 incomplete terminal receipt and absent full closure required")
        previous = linked(prior)
        require(previous["exit_code"] != 0 and previous["owned_descendants_settled"] is True and
                previous["owned_job_final_active_processes"] == 0 and previous["owned_job_final_member_pids"] == [],
                "R4 is live, completed or lacks actual terminal settlement")
    native_source = pin(NATIVE,NATIVE_SHA)
    image = Path(audit["inputs"]["service"]["path"])
    pin(image,audit["inputs"]["service"]["sha256"])
    effective_limit(NATIVE,image)
    plan.update(schema="airk.private.cal100-grouped-plan/1",
                phase_groups=group_roster(plan["phase_ids"],[list(g) for g in GROUPS]),
                recorder={"source":native_source,"admission_limit":1024,"record_limit":8192,"byte_limit":64<<20},
                sources=source_refs(), prior_attempt=prior, installation_mutex=mutex)
    return plan


def validate_chain(links, groups, plan_ref, installation_closed, publication):
    require(len(links) == len(groups) == 3, "incomplete native lifetime population")
    seen = set()
    all_phases = []
    prior = installation_closed
    probes = searches = 0
    for group, ref in enumerate(links):
        row = linked(ref)
        phases = groups[group]
        require(row["schema"] == "airk.private.cal100-group-lifetime/1" and row["group"] == group and
                row["phases"] == phases and row["prior"] == prior and row["plan"] == plan_ref and
                row["publication"] == publication and row["instance"] not in seen and
                len(row["phase_closures"]) == len(phases), "native lifetime chain differs")
        seen.add(row["instance"])
        owned = [linked(closure) for closure in row["phase_closures"]]
        require([p["phase"] for p in owned] == phases and row["end_ms"] <= owned[-1]["deadline_ms"] and
                all(p["start_ms"] <= p["end_ms"] <= p["deadline_ms"] <= p["start_ms"]+1800000 for p in owned) and
                all(a["end_ms"] <= b["start_ms"] for a,b in zip(owned,owned[1:])),
                "phase sequence, ownership or original time budget changed")
        all_phases.extend(phases)
        probes += row["probes"]
        searches += row["pipeline_searches"]
        prior = ref
    require(all_phases == list(range(13)) and probes <= 500 and searches > 0, "incomplete grouped native census")
    return probes, searches


def execute(output,node,plan_ref):
    plan = linked(plan_ref)
    require(plan["installation_mutex"] is not None, "shared installation lock was not acquired")
    require_parent_owns(linked(plan["installation_mutex"]), os.getppid())
    require(plan["prior_attempt"] is not None and make_plan(plan["start_ms"],plan["prior_attempt"],plan["installation_mutex"]) == plan,
            "fresh grouped capture plan or terminal dependency differs")
    audit = read(AUDIT_PATH,AUDIT_SHA)
    for ref in [*audit["inputs"].values(),*audit["compiled_sdk"].values()]:
        pin(ref["path"],ref["sha256"])
    sources = check_cal_sources()
    global_deadline = time.monotonic() + (plan["deadline_ms"]-now_ms())/1000
    original = plan["installation"]
    publication = linked(original["bindings"])["bindings"][0]["publication"]
    prior = original["closed"]
    instances = {linked(original["ready"])["health"]["instance"]}
    fingerprints = set()
    closures = []
    for group in range(3):
        require(time.monotonic()+125 < global_deadline, "insufficient original time for another native lifetime")
        pin(plan_ref["path"],plan_ref["sha256"])
        for ref in plan["sources"]:
            pin(ref["path"],ref["sha256"])
        directory = output/f"lifetime-{group:02}"
        directory.mkdir()
        require_parent_owns(linked(plan["installation_mutex"]), os.getppid())
        prior = run_group(directory,group,plan,plan_ref,prior,audit,node,global_deadline,fingerprints,instances)
        closures.append(prior)
    probes, searches = validate_chain(closures,plan["phase_groups"],plan_ref,original["closed"],publication)
    require(len(instances) == 4 and len(fingerprints) == 100 and check_cal_sources() == sources and
            time.monotonic() < global_deadline, "whole CAL census, original source or time budget differs")
    for ref in plan["sources"]:
        pin(ref["path"],ref["sha256"])
    elapsed = now_ms()-plan["start_ms"]+plan["setup_elapsed_ms"]
    require(elapsed <= 10800000, "common elapsed limit exceeded")
    native.write_json(output/"fixture-closure.json",{"schema":"airk.private.cal100-grouped-fixture/1",
                      "plan":plan_ref,"lifetimes":closures,"publication":publication,"ordered_questions":100,
                      "probes":probes,"pipeline_searches":searches,"elapsed_ms":elapsed})


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("--output",required=True,type=Path)
    parser.add_argument("--node",required=True,type=Path)
    parser.add_argument("--prior-outer-sha256",required=True)
    parser.add_argument("--owned-plan-sha256")
    args = parser.parse_args()
    output = args.output
    require(os.name == "nt" and sys.flags.no_site and sys.dont_write_bytecode and output.parent == CAMPAIGN and
            output.is_dir() and output.resolve(strict=True) == output and output != R4,
            "checked fresh Windows campaign output required")
    pin(args.node,NODE_SHA)
    prior = pin(R4/"outer-owned.json",args.prior_outer_sha256)
    path = output/"lifetime-plan.json"
    if args.owned_plan_sha256:
        require(os.environ.get("AIRK_GROUPED_CAL_OWNER") == str(output), "missing original owner")
        with native.capped_worker_job(output):
            execute(output,args.node,pin(path,args.owned_plan_sha256))
        return
    require(not any(output.iterdir()), "fresh grouped capture directory required")
    mutex = InstallationMutex()
    mutex.__enter__()
    acquired_path = output/"control-mutex-acquired.json"
    terminal_path = output/"outer-owned.json"
    try:
        native.write_json(acquired_path, {"name":MUTEX_NAME, "owner_pid":os.getpid(), "at_ms":now_ms()})
        plan = make_plan(now_ms(),prior,link(acquired_path))
        native.write_json(path,plan)
        argv = [sys.executable,"-B","-S",Path(__file__).resolve(),"--output",output,"--node",args.node,
                "--prior-outer-sha256",args.prior_outer_sha256,"--owned-plan-sha256",link(path)["sha256"]]
        owner = owned_module()
        result = owner.run_evaluation_fixture(argv,ROOT,output/"outer.log",(plan["deadline_ms"]-now_ms())/1000,
                                              os.environ|{"AIRK_GROUPED_CAL_OWNER":str(output)})
        native.write_json(terminal_path,result)
    finally:
        mutex.__exit__(None,None,None)
        if acquired_path.is_file():
            native.write_json(output/"control-mutex-released.json",{"name":MUTEX_NAME, "owner_pid":os.getpid(),
                              "at_ms":now_ms(), "acquired":link(acquired_path),
                              "outer":link(terminal_path) if terminal_path.is_file() else None})
    require(owner.successful(result) and result["owned_job_final_active_processes"] == 0 and
            result["owned_job_final_member_pids"] == [] and (output/"fixture-closure.json").is_file(),
            "incomplete grouped capture attempt; original evidence retained")


if __name__ == "__main__":
    main()
