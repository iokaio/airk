# SPDX-License-Identifier: Apache-2.0
"""Own a new two-lifetime original-service R6 under one declared common clock."""
import argparse
import os
from pathlib import Path
import runpy
import sys
import time

# Isolated Python skips the script directory. Only import maintained sibling sources.
HERE = Path(__file__).resolve().parent
sys.path.insert(0, str(HERE))
import r6_candidate
import r6_handoff
import r6_preflight as policy
import two_lifetimes as r5
import r7_handoff

PLAN_SCHEMA = "airk.private.cal100-r6-two-lifetime-plan/1"
FIXTURE_SCHEMA = "airk.private.cal100-r6-two-lifetime-fixture/1"
OUTPUT = policy.CAMPAIGN / "cal100-focus1024-capture-r6"
OWNERS = [Path(__file__).resolve(), HERE / "r6_handoff.py", HERE / "r6_preflight.py"]


def validate_configuration(previous, current):
    def settings(row):
        return {k: v for k, v in row["configuration"].items() if k != "revision"}
    policy.require(settings(previous) == settings(current)
                   and previous["configuration"]["revision"] == 3 and current["configuration"]["revision"] == 13
                   and all(previous[k]["active_profile"] == current[k]["active_profile"] == current[k]["selected_profile"]
                           and current[k]["availability"] == "ready" and current[k]["running"] == current[k]["queued"] == 0
                           for k in ("focus", "context")), "current idle CAL configuration or active stages differ")


def make_plan(start, mutex, decision, candidate_sha=None, search_handoff=None, *,
              create_sources=False, resume_r6=False):
    # R5 already contains the measured original population, source roster, installation and capacity proof.
    policy.inspect(decision)
    if resume_r6:
        policy.require(search_handoff and candidate_sha and decision is None,
                       "R7 needs the same candidate, Search evidence and original common clock")
    previous = policy.original(policy.R5 / "lifetime-plan.json", policy.PLAN)
    plan = r5.make_plan(start, None, mutex)
    for field in ("phase_ids", "phase_groups", "input_audit", "census", "capacity_basis",
                  "installation", "recorder", "sources", "setup_elapsed_ms"):
        policy.require(plan[field] == previous[field], "different original R5 plan field: " + field)
    if search_handoff:
        import r6_search_handoff as successor
        handoff = successor.evidence(*search_handoff)
        handoff_sources = [r5.link(HERE / "r6_search_handoff.py"), *handoff["sources"]]
    else:
        handoff = r6_handoff.evidence(r5.pin)
        handoff_sources = handoff["sources"]
    validate_configuration(r5.linked(plan["installation"]["status"]), r5.linked(handoff["idle"]))
    plan["installation"]["status"] = handoff["idle"]
    common = policy.selected_clock(decision)
    plan.update(schema=PLAN_SCHEMA, deadline_ms=policy.common_deadline(start, common, plan["setup_elapsed_ms"]),
                common_ms=common, clock_decision=r5.link(decision) if decision else None,
                predecessor_plan=r5.pin(policy.R5 / "lifetime-plan.json", policy.PLAN),
                prior_attempt=r5.pin(policy.R5 / "outer-owned.json", policy.OUTER), physical_handoff=handoff)
    for ref in [*(r5.link(path) for path in OWNERS), *handoff_sources]:
        if ref not in plan["sources"]:
            plan["sources"].append(ref)
    policy.require(len({ref["path"] for ref in plan["sources"]}) == len(plan["sources"]),
                   "one R6 source path has contradictory provenance")
    if candidate_sha is not None:
        epoch = r6_candidate.checked_epoch(candidate_sha, r5.pin)
        r5.previous.effective_limit(r5.previous.NATIVE, Path(epoch["image"]["path"]))
        if resume_r6:
            policy.require(plan["deadline_ms"] <= 1795194600000,
                           "original common clock no longer fits before the campaign cutoff")
            retirement = r7_handoff.evidence(plan, epoch, r5, create=create_sources)
            plan.update(schema=r7_handoff.PLAN_SCHEMA, restart_handoff=retirement,
                        scope_baseline=retirement["bindings"])
            plan["installation"]["status"] = retirement["idle"]
            plan["sources"].append(r5.link(Path(r7_handoff.__file__).resolve()))
            epoch["sources"] = r7_handoff.snapshots(epoch, r7_handoff.R7 / "candidate-sources",
                                                    r5.pin, create_sources)
        else:
            epoch["sources"] = r6_candidate.snapshots(epoch, OUTPUT / "candidate-sources",
                                                       r5.pin, create_sources)
        plan["sources"].extend([r5.link(HERE / "r6_candidate.py"), *epoch["sources"].values()])
        plan.update(input_audit=epoch["audit"], candidate_service=epoch)
    return plan


def execute(plan_ref, node, decision, candidate_sha=None, search_handoff=None, *, resume_r6=False):
    plan = r5.linked(plan_ref)
    mutex = r5.linked(plan["installation_mutex"])
    r5.require_parent_owns(mutex, os.getppid())
    policy.require(make_plan(plan["start_ms"], plan["installation_mutex"], decision, candidate_sha,
                             search_handoff, resume_r6=resume_r6) == plan, "frozen CAL plan differs")
    output = r7_handoff.R7 if resume_r6 else OUTPUT
    deadline = time.monotonic() + (plan["deadline_ms"] - r5.now_ms()) / 1000
    require_admission(deadline)
    if search_handoff:
        import r6_search_handoff as successor
        comparison_audit = r5.linked(plan["physical_handoff"]["plan"])["audit"]
        if candidate_sha:
            policy.require(comparison_audit == plan["input_audit"], "R6 candidate differs from native Search comparison")
        installation = plan["installation"]
        if resume_r6:
            installation = installation | {"status": plan["physical_handoff"]["idle"]}
            checked = r7_handoff.evidence(plan | {"installation": installation},
                                          plan["candidate_service"], r5, live=True)
            policy.require(checked == plan["restart_handoff"], "changed original R6 retirement")
        successor.verify(plan["physical_handoff"], installation, comparison_audit, live=not resume_r6)
    else:
        policy.require(not (policy.CAMPAIGN / "cal100-fused-canonical-search-r1/outer-owned.json").exists(),
                       "completed Search comparison requires its own physical handoff")
        r6_handoff.verify(plan["physical_handoff"], plan["installation"], r5.pin)
    group_owner = r5.run_group
    if candidate_sha is not None:
        module = plan["candidate_service"]["sources"]["group_service.py"]
        r5.pin(module["path"], module["sha256"])
        group_owner = runpy.run_path(module["path"])["run_group"]
    audit = r5.linked(plan["input_audit"])
    for ref in [*audit["inputs"].values(), *audit["compiled_sdk"].values()]:
        r5.pin(ref["path"], ref["sha256"])
    source = r5.check_cal_sources()
    install = plan["installation"]
    publication = r5.linked(install["bindings"])["bindings"][0]["publication"]
    prior = install["closed"]
    instances = {r5.linked(install["ready"])["health"]["instance"]}
    if resume_r6:
        instances.add(r5.linked(plan["restart_handoff"]["ready"])["health"]["instance"])
    prior_instances = len(instances)
    fingerprints, groups = set(), []
    probes = searches = 0
    for group in range(len(r5.PHASE_GROUPS)):
        r5.require_parent_owns(mutex, os.getppid())
        r5.pin(plan_ref["path"], plan_ref["sha256"])
        for ref in plan["sources"]:
            r5.pin(ref["path"], ref["sha256"])
        require_admission(deadline)
        work = output / f"lifetime-{group:02}"
        work.mkdir()
        closed = group_owner(work, group, plan, plan_ref, prior, audit, node, deadline, fingerprints, instances)
        row = r5.verify_group(closed, plan_ref, prior, group, publication)
        groups.append(closed)
        prior = closed
        probes += row["probes"]
        searches += row["pipeline_searches"]
    policy.require(len(instances) == prior_instances + len(r5.PHASE_GROUPS)
                   and len(fingerprints) == 100 and probes <= 500 and searches > 0
                   and r5.check_cal_sources() == source, "R6 native census or sources differ")
    for ref in plan["sources"]:
        r5.pin(ref["path"], ref["sha256"])
    elapsed = r5.now_ms() - plan["start_ms"] + plan["setup_elapsed_ms"]
    policy.require(elapsed <= plan["common_ms"] and time.monotonic() < deadline, "R6 exceeded declared clock")
    r5.native.write_json(output / "fixture-closure.json",
                         {"schema": r7_handoff.FIXTURE_SCHEMA if resume_r6 else FIXTURE_SCHEMA,
                          "plan": plan_ref, "lifetimes": groups,
                          "publication": publication, "ordered_questions": 100, "probes": probes,
                          "pipeline_searches": searches, "elapsed_ms": elapsed})


def host_admission():
    import ctypes
    from ctypes import wintypes
    import shutil

    class Performance(ctypes.Structure):
        _fields_ = [("cb", wintypes.DWORD), *[(name, ctypes.c_size_t) for name in
                    ("CommitTotal", "CommitLimit", "CommitPeak", "PhysicalTotal", "PhysicalAvailable",
                     "SystemCache", "KernelTotal", "KernelPaged", "KernelNonpaged", "PageSize")],
                    *[(name, wintypes.DWORD) for name in ("HandleCount", "ProcessCount", "ThreadCount")]]
    data = Performance()
    data.cb = ctypes.sizeof(data)
    policy.require(ctypes.WinDLL("psapi").GetPerformanceInfo(ctypes.byref(data), data.cb), "missing Windows capacity")
    row = {"available_physical": data.PhysicalAvailable * data.PageSize,
           "available_commit": (data.CommitLimit - data.CommitTotal) * data.PageSize,
           "available_disk": shutil.disk_usage(OUTPUT.parent).free,
           "host_required": (32 + 8 + 38) << 30, "disk_required": (140 + 13) << 30}
    policy.require(min(row["available_physical"], row["available_commit"]) >= row["host_required"]
                   and row["available_disk"] >= row["disk_required"], "other live reservations prevent R6 admission")
    return row


def require_admission(deadline):
    policy.require(time.monotonic() + 125 < deadline, "insufficient R6 time for an original service or call")


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("--output", type=Path, required=True)
    parser.add_argument("--node", type=Path, required=True)
    parser.add_argument("--decision", type=Path)
    parser.add_argument("--candidate-provenance-sha256", help="observed native producer sidecar SHA-256")
    parser.add_argument("--search-handoff", nargs=3, metavar=("OUTER_SHA", "RELEASE_SHA", "COMPARISON_SHA"))
    parser.add_argument("--owned-plan-sha256")
    parser.add_argument("--resume-r6", action="store_true", help="start the distinct R7 after verified R6 retirement")
    args = parser.parse_args()
    output = r7_handoff.R7 if args.resume_r6 else OUTPUT
    policy.require(os.name == "nt" and sys.flags.isolated and sys.flags.no_site and sys.dont_write_bytecode
                   and args.output == output and output.resolve(strict=True) == output, "checked native CAL output required")
    r5.pin(args.node, r5.NODE_SHA)
    policy.selected_clock(args.decision)
    policy.require(not args.candidate_provenance_sha256 or args.search_handoff,
                   "candidate capture requires its own successful original Search comparison")
    plan_path = output / "lifetime-plan.json"
    if args.owned_plan_sha256:
        policy.require(os.environ.get("AIRK_R6_CAL_OWNER") == str(output), "original CAL outer Job required")
        with r5.native.capped_worker_job(output):
            execute(r5.pin(plan_path, args.owned_plan_sha256), args.node, args.decision,
                    args.candidate_provenance_sha256, args.search_handoff, resume_r6=args.resume_r6)
        return
    policy.require(not any(output.iterdir()), "fresh CAL directory required")
    lock = r5.InstallationMutex()
    lock.__enter__()
    acquired = output / "control-mutex-acquired.json"
    terminal = output / "outer-owned.json"
    try:
        start = r5.now_ms()
        r5.native.write_json(acquired, {"name": r5.MUTEX_NAME, "owner_pid": os.getpid(), "at_ms": start})
        r5.native.write_json(output / "host-admission.json", host_admission())
        plan = make_plan(start, r5.link(acquired), args.decision, args.candidate_provenance_sha256,
                         args.search_handoff, create_sources=True, resume_r6=args.resume_r6)
        r5.native.write_json(plan_path, plan)
        argv = [sys.executable, "-I", "-B", "-S", OWNERS[0], "--output", output, "--node", args.node,
                "--owned-plan-sha256", r5.link(plan_path)["sha256"]]
        if args.resume_r6:
            argv.append("--resume-r6")
        if args.decision:
            argv.extend(["--decision", args.decision])
        if args.search_handoff:
            argv.extend(["--search-handoff", *args.search_handoff])
        if args.candidate_provenance_sha256:
            argv.extend(["--candidate-provenance-sha256", args.candidate_provenance_sha256])
        owner = r5.owned_module()
        remaining = (plan["deadline_ms"] - r5.now_ms()) / 1000
        policy.require(remaining > 125, "insufficient original R6 owner window")
        result = owner.run_evaluation_fixture(argv, r5.ROOT, output / "outer.log", remaining,
                                              os.environ | {"AIRK_R6_CAL_OWNER": str(output)})
        r5.native.write_json(terminal, result)
    finally:
        lock.__exit__(None, None, None)
        if acquired.is_file():
            r5.native.write_json(output / "control-mutex-released.json",
                                 {"name": r5.MUTEX_NAME, "owner_pid": os.getpid(), "at_ms": r5.now_ms(),
                                  "acquired": r5.link(acquired), "outer": r5.link(terminal) if terminal.is_file() else None})
    policy.require(owner.successful(result) and result["owned_job_final_active_processes"] == 0
                   and result["owned_job_final_member_pids"] == [] and (output / "fixture-closure.json").is_file(),
                   "CAL attempt incomplete; retained original partial evidence")


if __name__ == "__main__":
    main()
