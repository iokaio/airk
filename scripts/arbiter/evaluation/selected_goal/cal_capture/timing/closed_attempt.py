# SPDX-License-Identifier: Apache-2.0
"""Read-only timing analysis after the original native service and both measured passes have retired."""
import argparse
import json
from pathlib import Path
import sys

sys.path.insert(0,str(Path(__file__).resolve().parent))
from run import CAMPAIGN, MUTEX_NAME, link, linked, native, owned_module, pin, read_journal, require, summary


def closed(out, outer_sha, release_sha, custody_sha):
    require(out.parent == CAMPAIGN and out.name == "cal100-focus1024-timing-ab-r2", "foreign timing attempt")
    outer_ref = pin(out/"outer-owned.json", outer_sha)
    outer = linked(outer_ref)
    release = linked(pin(out/"control-mutex-released.json", release_sha))
    custody = linked(pin(out.with_name(out.name+"-custody.json"), custody_sha))
    plan_ref = outer["timing_plan"]
    plan = linked(plan_ref)
    acquired = linked(release["acquired"])
    require(release["name"] == acquired["name"] == MUTEX_NAME and
            release["outer"]["sha256"] == outer_sha and release["owner_pid"] == acquired["owner_pid"] and
            release["at_ms"] >= acquired["at_ms"] and release["acquired"] == plan["mutex"],
            "inconsistent original installation ownership")
    require(plan["output"] == str(out) and Path(plan_ref["path"]) == out/"plan.json" and
            outer["exit_code"] == custody["exit_code"] == 1 and not custody["source_drift"] and
            outer["owned_descendants_settled"] and not outer["owned_job_forced_termination"] and
            outer["owned_job_final_active_processes"] == 0 and not outer["owned_job_final_member_pids"] and
            "ValueError: missing or reused timing grant" in (out/"outer.log").read_text() and
            not (out/"service-failure.json").exists(), "original analysis failure or Job settlement differs")
    work = out/"fixture"
    docs = {k:link(work/n) for k,n in (("ready","ready.json"),("closed","closed.json"),
                                        ("bindings","enable-context.bindings.json"),("journal","composed-observer.jsonl"),
                                        ("journal_closure","composed-observer.closed.json"))}
    ready, retired = linked(docs["ready"]),linked(docs["closed"])
    require(retired["instance"] == ready["health"]["instance"] and retired["shutdown_completed"] and
            retired["service_lease_reacquired"], "native runtime has not retired")
    for item in plan["sources"]:
        pin(item["path"],item["sha256"])
    previous = linked(plan["previous_control_after"])
    before, after = linked(plan["control_before"]), linked(outer["control_after"])
    require(before["files"] == previous["files"] and before["immutable"] == after["immutable"] and
            all(after["files"].get(k) == v for k,v in before["immutable"].items()), "installed native assets changed")
    audit = linked(plan["input_audit"])
    stage = native.read_json(out/"commands/stage-after.stdout")
    require(stage == native.read_json(out/"commands/stage-before.stdout") or
            stage["configuration"] == native.read_json(out/"commands/stage-before.stdout")["configuration"],
            "timed stage configuration changed")
    focus = audit["profiles"]["arbiter-focus-qwen3-reranker-06b"]
    require(stage["focus"]["active_profile"] == focus and all(stage[k]["availability"]=="ready" and
            stage[k]["running"]==stage[k]["queued"]==0 for k in ("focus","context")), "final stages not ready and idle")
    dispatch = read_journal(work,focus,setup_only=False)
    binding, = linked(docs["bindings"])["bindings"]
    passes, scopes = [], set()
    for i in range(2):
        receipt_ref = link(out/f"pass-{i}.owned.json")
        job = linked(receipt_ref)
        log = out/f"pass-{i}.jsonl"
        request = out/f"pass-{i}-request.json"
        argv = job["argv"]
        require(owned_module().successful(job) and job["owned_job_final_active_processes"] == 0 and
                not job["owned_job_final_member_pids"] and Path(job["log"]) == log and Path(argv[2]) == request and
                native.read_json(request)["timing_plan"] == plan_ref and native.read_json(request)["timing_pass"] == i,
                "incomplete original phase Job or request")
        pin(request,argv[3])
        summary(log,12,plan["question_ids"],retired["instance"],binding["publication"],scopes)
        passes.append({"owned":receipt_ref,"transcript":link(log)})
    require(len(scopes)==8,"eight original W4 windows required")
    from closed_results import paired
    analysis = paired(plan,out,passes,dispatch,binding)
    return {"schema":"airk.private.cal-timing-postclosure-analysis/1","original_outer":outer_ref,
            "release":link(out/"control-mutex-released.json"),"post_custody":link(out.with_name(out.name+"-custody.json")),
            "reader":link(Path(__file__).resolve()),"independent_reader":link(Path(__file__).with_name("closed_readout.py").resolve()),
            "paired_reader":link(Path(__file__).with_name("closed_results.py").resolve()),
            "original_plan":plan_ref,"passes":passes,"native":docs,"dispatch":dispatch,"distinct_windows":len(scopes),
            "observed":analysis,"qualification":False}


if __name__ == "__main__":
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("output",type=Path)
    parser.add_argument("outer_sha")
    parser.add_argument("release_sha")
    parser.add_argument("custody_sha")
    args = parser.parse_args()
    print(json.dumps(closed(args.output,args.outer_sha,args.release_sha,args.custody_sha),allow_nan=False))
