# SPDX-License-Identifier: Apache-2.0
"""Independent original-clock checks for the R6 scientific certificate epoch."""
import hashlib
import os
from pathlib import Path

from reference_inputs import CAMPAIGN, HERE, checked, read, reference, require

ORIGINAL = CAMPAIGN/"cal100-focus1024-launch-r1/cal100-launch-audit.json"
ORIGINAL_SHA = "87731282ecd121eaf08d5074d1bc421c5753bc65f7126f9578b31c6f9303d661"
NEW_AUDIT = CAMPAIGN/"cal100-fused-candidate-launch-r1/cal100-launch-audit.json"
R5 = CAMPAIGN/"cal100-focus1024-capture-r5/lifetime-plan.json"
R5_SHA = "55a89e5219cf6de0c3b59cbaa8a4a8a43dc0d097276368cfb5a28ee2fe6536bf"
ROOT = CAMPAIGN/"cal100-focus1024-capture-r6"
R7_ROOT = CAMPAIGN/"cal100-focus1024-capture-r7"


def audit_for_epoch(pin, service_sha):
    original=read(reference(ORIGINAL,ORIGINAL_SHA))
    path=Path(pin["path"])
    require(path in (ORIGINAL,NEW_AUDIT) and (path!=ORIGINAL or pin["sha256"]==ORIGINAL_SHA),
            "unknown R6 audit or original identity changed")
    actual=read(pin,path)
    for key in ("schema","census","pipeline","profiles","phase_counts","compiled_sdk","native_template_source","focus_static1024","fixture_environment"):
        require(actual[key]==original[key], "R6 original population or scoring inputs changed: "+key)
    require(actual["inputs"].keys()==original["inputs"].keys() and
            all(actual["inputs"][k]==original["inputs"][k] for k in original["inputs"] if k!="service"),
            "R6 source, CLI or model asset changed")
    require(actual["inputs"]["service"]["sha256"]==service_sha and
            actual["calibration_fitted"] is False and actual["fixture_started"] is False, "unbound service or audit")
    require(actual["selected_tests"]==1 and actual["selector_check"]==[actual["inputs"]["service"]["path"],*original["selector_check"][1:]],
            "service image listing did not identify the original single fixture selector")
    ref=actual["inputs"]["service"]
    file=Path(ref["path"])
    before=file.stat()
    require(file.is_absolute() and file.resolve(strict=True)==file and before.st_size==ref["bytes"],"service image identity differs")
    with file.open("rb") as stream:
        require(os.path.samestat(before,os.fstat(stream.fileno())) and hashlib.file_digest(stream,"sha256").hexdigest()==service_sha,
                "active native service image digest differs")
    after=file.stat()
    require(os.path.samestat(before,after) and before.st_size==after.st_size and before.st_mtime_ns==after.st_mtime_ns, "service image changed during authentication")
    return actual


def original_clock(plan, original, question_ids):
    require(len(question_ids)==len(set(question_ids))==100 and
            plan["phase_ids"]==original["phase_ids"]==[question_ids[i:i+8] for i in range(0,100,8)] and
            plan["phase_groups"]==original["phase_groups"]==[list(range(7)),list(range(7,13))],
            "R6 panel, phase or lifetime partition differs")
    require(plan["setup_elapsed_ms"]==original["setup_elapsed_ms"]==545806 and
            type(plan["start_ms"]) is int and type(plan["deadline_ms"]) is int and
            plan["deadline_ms"]-plan["start_ms"]+plan["setup_elapsed_ms"]==10800000,
            "R6 common clock or original setup debit differs")
    require(plan["recorder"]==original["recorder"] and plan["capacity_basis"]==original["capacity_basis"]
            and plan["installation"].keys()==original["installation"].keys() and
            all(plan["installation"][k]==v for k,v in original["installation"].items() if k!="status"),
            "R6 installation, counters or capacity basis changed")


def preflight(root, plan_pin, audit_pin, service_sha):
    require(root in (ROOT, R7_ROOT), "dedicated R6 or R7 physical receipt root required")
    plan=read(plan_pin,root/"lifetime-plan.json")
    context,audit=verify_plan(plan,audit_pin,service_sha,root=root)
    return dict(context,plan_pin=plan_pin),audit


def verify_plan(plan,audit_pin,service_sha,*,root=ROOT):
    resume_r6 = root == R7_ROOT
    attempt = "r7" if resume_r6 else "r6"
    original=read(reference(R5,R5_SHA))
    audit=audit_for_epoch(audit_pin,service_sha)
    require(plan["input_audit"]==audit_pin and plan["census"]==audit["census"], "R6 launch and plan differ")
    original_clock(plan,original,read(plan["census"])["question_ids"])
    require(plan["schema"]==f"airk.private.cal100-{attempt}-two-lifetime-plan/1" and plan["common_ms"]==10800000
            and plan["clock_decision"] is None and plan["predecessor_plan"]==reference(R5,R5_SHA),
            "unregistered R6 clock or predecessor")
    outer=read(plan["prior_attempt"],R5.parent/"outer-owned.json")
    require(plan["prior_attempt"]["sha256"]=="23aedfee1e92ef826714f37b852301894f048f4ad3964e11eb75e65fd7b4e276"
            and outer["exit_code"]==1 and outer["owned_descendants_settled"] and not outer["owned_job_forced_termination"]
            and not outer["owned_job_final_member_pids"] and not (R5.parent/"fixture-closure.json").exists(),
            "R5 retirement changed")
    handoff=plan["physical_handoff"]
    import sys
    group=HERE.parent/"cal_capture/grouped"
    sys.path.insert(0,str(group))
    successor="comparison" in handoff
    idle_pin = handoff["idle"]
    require(not resume_r6 or successor and "candidate_service" in plan,
            "R7 requires the optimized service and original successful Search")
    if successor:
        import r6_search_handoff
        lookup=read(handoff["plan"],r6_search_handoff.RUN/"plan.json")
        search_installation = plan["installation"] | {"status": idle_pin}
        r6_search_handoff.verify(handoff,search_installation,lookup["audit"],live=False)
        require(read(handoff["mutex"])["at_ms"] <= plan["start_ms"],"Search still owned installation at CAL start")
        idle_path=r6_search_handoff.RUN/"commands/stage-after.stdout"
        if resume_r6:
            from r7_restart import INPUTS, validate_restart
            comparison = read(handoff["comparison"],r6_search_handoff.RUN/"comparison.json")
            named = read(comparison["fixture"]["enable-context.bindings.json"],
                         r6_search_handoff.RUN/"fixture/enable-context.bindings.json")
            stable_control = read(handoff["control_after"],r6_search_handoff.RUN/"control-after.json")
            validate_restart(plan,audit,named,stable_control)
            idle_path = INPUTS["idle"][0]
            idle_pin = plan["restart_handoff"]["idle"]
    else:
        retired=read(handoff["retired"],CAMPAIGN/"compact-development-TRAIN_Q598-focus1024-run-r2/owned.json")
        require(handoff["retired"]["sha256"]=="f45abb479f15087f860e0a0de0261f7ac7ff4044e88947fb0f2378a5ee4bc771" and
                retired["exit_code"]==1 and retired["owned_descendants_settled"] and retired["owned_job_final_active_processes"]==0,
                "physical retirement differs")
        idle_path=CAMPAIGN/"cal100-search-mode-timing-r1/commands/stage-after.stdout"
        require(handoff["idle"]["sha256"]=="30dacd28b6d15f240791d29fdbf42734be68dc9bdb85674780bb2d4d3298b84d","original Search idle changed")
    idle=read(idle_pin,idle_path)
    prior=read(original["installation"]["status"])
    config=idle["configuration"]
    require(plan["installation"]["status"]==idle_pin and
            config["revision"]==13 and prior["configuration"]|{"revision":13}==config and
            all(idle[k]["running"]==idle[k]["queued"]==0 and idle[k]["availability"]=="ready" and
                idle[k]["selected_profile"]==idle[k]["active_profile"]==prior[k]["active_profile"] for k in ("focus","context")),
            "current CAL enrollment or stage differs")
    sources=plan["sources"]
    group=HERE.parent/"cal_capture/grouped"
    owner=group/"r6_capture.py"
    extras={owner,group/"r6_handoff.py",group/"r6_preflight.py"}
    if resume_r6:
        extras.add(group/"r7_handoff.py")
    expected_handoff={HERE.parents[2]/"analysis/development_margins"/name for name in
                      ("reuse_installation.py","q598_closed_restart.py","q598_search_handoff.py")}
    worker=group.parent/"worker.cjs"
    if successor:
        expected_handoff={group.parent/"fused_search/compare.py",group.parent/"dispatch_proof.py"}
        extras.add(group/"r6_search_handoff.py")
    r5paths={Path(row["path"]) for row in original["sources"]}
    expected=r5paths | extras | expected_handoff
    if "candidate_service" in plan:
        import r6_candidate
        require(successor,"candidate has no successful real Search retirement")
        candidate=plan["candidate_service"]
        epoch=r6_candidate.checked_epoch(candidate["provenance"]["sha256"],r6_search_handoff.seal)
        target=root/"candidate-sources"
        projected=candidate["sources"]
        require(set(projected)=={"worker.cjs","group_service.py"} and
                all(Path(ref["path"])==target/name for name,ref in projected.items()) and
                {p.name for p in target.iterdir()}==set(projected),"unexpected projected sources")
        from r6_effective_sources import verify as verify_source_bytes
        legacy_directory=Path(read(original["input_audit"])["inputs"]["service"]["path"]).parent
        verify_source_bytes(HERE.parents[4],group.parent,target,epoch["audit"]["sha256"],legacy_directory,
                            {name:checked(ref) for name,ref in projected.items()},resume_r6=resume_r6)
        require(candidate==dict(epoch,sources=projected) and epoch["audit"]==audit_pin==lookup["audit"]
                and epoch["image"]==audit["inputs"]["service"],"different native candidate or projected worker")
        expected.update({group/"r6_candidate.py",target/"worker.cjs",target/"group_service.py"})
        worker=target/"worker.cjs"
    else:
        require(Path(audit_pin["path"])==ORIGINAL,"original worker requires original service audit")
    require({Path(p["path"]) for p in handoff["sources"]}==expected_handoff and
            sources[:len(original["sources"])]==original["sources"] and len(sources)==len(expected) and
            {Path(p["path"]) for p in sources}==expected,"incorrect R6 source owner roster")
    for source in sources:
        checked(source)
    return {"plan_schema":plan["schema"], "fixture_schema":f"airk.private.cal100-{attempt}-two-lifetime-fixture/1",
            "source":owner,"worker":worker,"status_path":idle_path,"resume_r6":resume_r6,
            "anchor_service":read(original["input_audit"])["inputs"]["service"]["sha256"]},audit
