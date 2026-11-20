# SPDX-License-Identifier: Apache-2.0
"""Independently authenticate the phased CAL100 capture and original persistent installation."""
import math
from pathlib import Path
import re
import runpy

from capture_closure import question_events, settled
from reference_inputs import CAMPAIGN, HERE, checked, digest, parsed, read, reference, require


ANCHOR = CAMPAIGN / "cal100-focus1024-capture-r3"
ANCHOR_FILES = {
    "ready": ("fixture/ready.json","a977a203165f52a8ff42beb570bee46b6b4cfab609087d8f6120ee21252fd71f"),
    "closed": ("fixture/closed.json","c3feab1c99a4837c2fecc95e571e960474b35b39487f84c0c1d8f9ffd74ec992"),
    "status": ("commands/cal-ready.stdout","c2b6d1f4d31ca023058db4a0ddf4149d231e733cd2fcad881b64138416960dfa"),
    "terminal": ("outer-owned.json","58ae17e9ae259ea6752b50a445138dfadd324131f10b8d0a6dc419349f617031"),
}


def installation_identity(bound):
    row, = bound["bindings"]
    status = row["status_result"]["structuredContent"]
    scope = dict(status["scope_observation"])
    fingerprint = scope.pop("fingerprint")
    require(re.fullmatch(r"[0-9a-f]{64}",fingerprint), "original native scope fingerprint missing")
    return ({k:v for k,v in row.items() if k!="status_result"},
            {k:status[k] for k in ("files","indexed_vectors","publication","semantic_readiness","read_scope")},scope),fingerprint


def check_warm(refs, work, ready):
    end = read(refs["summary"], work/"warm-completions.closed.json")
    require(Path(refs["journal"]["path"]) == work/"warm-completions.jsonl" and
            len(checked(refs["journal"])) == end["bytes"] and end["complete"] is True and
            end["dropped"] == 0 and end["error"] is None and end["observer_id"] == ready["warm_observer_id"]
            and end["records"] == end["last_sequence"] == 2, "incomplete native warm lifetime")


def phase_interval(start, end, prior_end, common_deadline):
    require(type(start) is int and type(end) is int and prior_end<=start<end<=common_deadline and
            end-start<=1800000, "phase lifetime overlaps, exceeds 1800 seconds or refreshes the campaign clock")


def phased_events(root, outer_sha, fixture_sha, audit, audit_sha, ids, sources, index_root, tasks):
    require(root.parent == CAMPAIGN and re.fullmatch(r"cal100-focus1024-capture-r[1-9][0-9]*",root.name),
            "foreign phased campaign root")
    outer = read(reference(root/"outer-owned.json",outer_sha))
    closure = read(reference(root/"fixture-closure.json",fixture_sha))
    plan = read(closure["plan"], root/"lifetime-plan.json")
    require(settled(outer) and "--owned-plan-sha256" in outer["argv"] and str(root) in outer["argv"]
            and closure["plan"]["sha256"] in outer["argv"] and plan["schema"]=="airk.private.cal100-lifetime-plan/1"
            and closure["schema"]=="airk.private.cal100-phased-fixture/1" and plan["input_audit"]["sha256"]==audit_sha
            and read(plan["input_audit"])==audit and plan["census"]==audit["census"] and read(plan["census"])["question_ids"]==ids
            and len(ids)==len(set(ids))==closure["ordered_questions"]==100 and len(closure["lifetimes"])==13
            and plan["phase_ids"]==[ids[i:i+8] for i in range(0,100,8)],"incomplete or changed predeclaration")
    anchor = plan["installation"]
    values = {}
    for key,(suffix,sha) in ANCHOR_FILES.items():
        require(anchor[key]["sha256"]==sha, "original installation changed")
        values[key] = read(anchor[key],ANCHOR/suffix)
    original = read(anchor["request"],ANCHOR/"live-request.json")
    require(original["ready"]==anchor["ready"] and original["bindings"]==anchor["bindings"] and
            original["start_receipt"]==anchor["start"],"anchor references differ")
    base_binding = read(anchor["bindings"],ANCHOR/"fixture/enable-context.bindings.json")
    template = read(original["answer"],ANCHOR/"fixture/context-request.json")
    require(template["budget"]["input_tokens"]==8192 and template["budget"]["deadline_ms"]==120000 and
            template["response_mode"]=="full" and template["include_status"] is True,"original Answer template differs")
    require(anchor["custody"]["sha256"]=="6fe3f0a9ef5d88c7d3ccda9791165ffd5d055252db8b95ac2ab6d91277ac4f1e" and
            read(anchor["custody"],CAMPAIGN/"cal100-focus1024-capture-r3-custody.json")["changed_input_count"]==0,
            "original installation input custody changed")
    terminal, original_ready, original_closed = values["terminal"],values["ready"],values["closed"]
    require(terminal["exit_code"]==1 and terminal["owned_descendants_settled"] is True and
            terminal["owned_job_final_member_pids"]==[] and terminal["owned_job_forced_termination"] is False and
            original_closed["service_lease_reacquired"] is True and original_closed["shutdown_completed"] is True and
            original_ready["health"]["instance"]==original_closed["instance"]==base_binding["instance"] and
            original_ready["inputs_sha256"]==audit["inputs"]["inputs"]["sha256"] and
            original_ready["health"]["executable_sha256"]==audit["inputs"]["service"]["sha256"],"unclosed anchor installation")
    debit = math.ceil(terminal["elapsed_seconds"]*1000)
    require(debit==plan["setup_elapsed_ms"] and 0<debit<10800000 and
            plan["deadline_ms"]==plan["start_ms"]+10800000-debit,"campaign clock changed")
    proof = HERE.parent/"cal_capture/dispatch_proof.py"
    require(digest(proof.read_bytes())=="75b370b0446e0be1946896cb8978e8028aed0fb939320d4c72426f40386ea027","dispatch proof changed")
    read_dispatch = runpy.run_path(str(proof))["read_journal"]
    focus = audit["profiles"]["arbiter-focus-qwen3-reranker-06b"]
    stage = values["status"]
    require(stage["focus"]["active_profile"]==focus and
            stage["context"]["active_profile"]==audit["profiles"]["arbiter-context-qwen3-17b"], "anchor profiles differ")
    anchor_work = ANCHOR/"fixture"
    read(anchor["dispatch_summary"],anchor_work/"composed-observer.closed.json")
    read_dispatch(anchor_work,focus,setup_only=False)
    warm_end = read(anchor["warm_summary"],anchor_work/"warm-completions.closed.json")
    require(warm_end["complete"] is True and warm_end["observer_id"]==original_ready["warm_observer_id"] and
            warm_end["records"]==warm_end["last_sequence"]==2 and warm_end["bytes"]==(anchor_work/"warm-completions.jsonl").stat().st_size
            and warm_end["dropped"]==0 and warm_end["error"] is None,"anchor warm ledger differs")
    identity, initial_fingerprint = installation_identity(base_binding)
    fingerprints, instances = {initial_fingerprint},{original_ready["health"]["instance"]}
    prior = anchor["closed"]
    last = plan["start_ms"]
    sums = {"probes":0,"pipeline_searches":0}
    for phase,ref in enumerate(closure["lifetimes"]):
        directory = root/f"lifetime-{phase:02}"
        work = directory/"fixture"
        lifetime = read(ref,directory/"lifetime-closure.json")
        phase_interval(lifetime["start_ms"],lifetime["end_ms"],last,plan["deadline_ms"])
        require(lifetime["schema"]=="airk.private.cal100-phase-lifetime/1" and lifetime["phase"]==phase and
                lifetime["plan"]==closure["plan"] and lifetime["prior"]==prior,"phase ordering or prior closure differs")
        restart = read(lifetime["restart"],directory/"restart-request.json")
        require(restart=={k:{n:anchor[k][n] for n in ("path","sha256")} for k in ("ready","closed","status")},
                "restart references a different installation")
        owner_path = work/"restart-owner.json"
        require(owner_path.stat().st_size<=64<<10,"restart owner record too large")
        restart_owner = parsed(owner_path.read_bytes())
        require(restart_owner=={"control":original_ready["control_root"],"configuration":stage["configuration"],
                                "prior_instance":original_ready["health"]["instance"]},
                "native restart owner did not bind original control, instance and configuration")
        live = read(lifetime["live_request"],directory/"live-request.json")
        ready = read(live["ready"],work/"ready.json")
        clock = read(live["start_receipt"],directory/"fixture-start.json")
        closed = read(lifetime["closed"],work/"closed.json")
        bound = read(live["bindings"],work/"enable-context.bindings.json")
        instance = lifetime["instance"]
        require(live["fixture_root"]==str(work) and clock["start_ms"]==lifetime["start_ms"] and
                clock["deadline_ms"]-clock["start_ms"]==10800000 and clock["input_audit_sha256"]==audit_sha and
                ready["health"]["process_id"]==clock["pid"] and instance not in instances and instance==ready["health"]["instance"]==
                closed["instance"]==bound["instance"] and ready["health"]["executable_sha256"]==audit["inputs"]["service"]["sha256"] and
                all(ready[k]==original_ready[k] for k in ("inputs_sha256","control_root","profiles")) and
                closed["shutdown_completed"] is True and closed["service_lease_reacquired"] is True,
                "reused instance, changed installation or incomplete native settlement")
        instances.add(instance)
        current, named = installation_identity(bound)
        require(current==identity and named not in fingerprints and read(live["answer"],work/"context-request.json")==template,
                "changed source binding/template or stale named scope")
        fingerprints.add(named)
        binding = sources["ResidentBinding"](work/"enable-context.bindings.json",source_root=index_root)
        require(binding.publication==closure["publication"]==lifetime["publication"] and
                lifetime["dispatch"]==read_dispatch(work,focus,setup_only=False),"changed publication or dispatch ledger")
        for side in ("before","after"):
            status = parsed((directory/"commands"/f"stage-{side}.stdout").read_bytes())
            require(status["configuration"]==stage["configuration"] and all(
                status[n]["availability"]=="ready" and status[n]["selected_profile"]==status[n]["active_profile"]==
                stage[n]["active_profile"] for n in ("focus","context")),"stage configuration changed")
        check_warm(lifetime["warm"],work,ready)
        census = {k:lifetime[k] for k in sums}
        census.update(phase_closures=[lifetime["phase_closure"]],request_sha256=lifetime["live_request"]["sha256"])
        yield from question_events(directory,lifetime,census,ids,binding,template,tasks,audit["pipeline"],
                                   first_phase=phase,lifetime_fingerprints=fingerprints)
        binding.verify()
        for k in sums:
            sums[k] += lifetime[k]
        last, prior = lifetime["end_ms"],ref
    require(all(closure[k]==n for k,n in sums.items()) and sums["pipeline_searches"]>0 and len(instances)==14 and
            len(fingerprints)==114 and last-plan["start_ms"]+debit<=closure["elapsed_ms"]<=10800000,
            "incomplete phased census, unique scopes or common wall clock")
