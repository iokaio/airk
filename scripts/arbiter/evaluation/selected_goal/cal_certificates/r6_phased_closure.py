# SPDX-License-Identifier: Apache-2.0
"""Independently authenticate the phased CAL100 capture and original persistent installation."""
import math
from pathlib import Path
import runpy

from capture_closure import question_events, settled
from reference_inputs import CAMPAIGN, HERE, checked, digest, parsed, read, reference, require


from phased_closure import ANCHOR, ANCHOR_FILES, installation_identity, check_warm


def phased_events(root, outer_sha, fixture_sha, audit, audit_sha, ids, sources, index_root, tasks, *, release_sha, r6):
    require(root.parent == CAMPAIGN and root.name == "cal100-focus1024-capture-r6",
            "foreign phased campaign root")
    outer = read(reference(root/"outer-owned.json",outer_sha))
    closure = read(reference(root/"fixture-closure.json",fixture_sha))
    plan = read(closure["plan"], root/"lifetime-plan.json")
    import r6_group as groups
    shape = groups.TWO_GROUPS
    require(closure["schema"] == r6["fixture_schema"] and closure["plan"] == r6["plan_pin"] and
            str(r6["source"]) in outer["argv"], "wrong R6 original owner or fixture")
    require(settled(outer) and "--owned-plan-sha256" in outer["argv"] and str(root) in outer["argv"]
            and closure["plan"]["sha256"] in outer["argv"] and plan["schema"]==r6["plan_schema"]
            and plan["input_audit"]["sha256"]==audit_sha
            and read(plan["input_audit"])==audit and plan["census"]==audit["census"] and read(plan["census"])["question_ids"]==ids
            and len(ids)==len(set(ids))==closure["ordered_questions"]==100 and len(closure["lifetimes"])==len(shape)
            and plan["phase_ids"]==[ids[i:i+8] for i in range(0,100,8)],"incomplete or changed predeclaration")
    anchor = plan["installation"]
    values = {}
    for key,(suffix,sha) in ANCHOR_FILES.items():
        if key == "status":
            values[key] = read(anchor[key],r6["status_path"])
            continue
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
            original_ready["health"]["executable_sha256"]==r6["anchor_service"],"unclosed anchor installation")
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
    for group,ref in enumerate(closure["lifetimes"]):
        phase = shape[group][0]
        directory = root/f"lifetime-{group:02}"
        work = directory/"fixture"
        lifetime = read(ref,directory/"lifetime-closure.json")
        require(lifetime["plan"]==closure["plan"] and lifetime["prior"]==prior,"plan or predecessor changed")
        phase_pins=groups.check_group(directory,lifetime,group,plan,last)
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
        census.update(phase_closures=phase_pins,request_sha256=lifetime["live_request"]["sha256"])
        yield from question_events(directory,lifetime,census,ids,binding,template,tasks,audit["pipeline"],
                                   first_phase=phase,lifetime_fingerprints=fingerprints)
        binding.verify()
        for k in sums:
            sums[k] += lifetime[k]
        last, prior = lifetime["end_ms"],ref
    groups.check_mutex(root,plan,outer_sha,release_sha,plan["start_ms"]+closure["elapsed_ms"]-debit)
    require(all(closure[k]==n for k,n in sums.items()) and sums["pipeline_searches"]>0 and len(instances)==len(shape)+1 and
            len(fingerprints)==len(ids)+len(shape)+1 and last-plan["start_ms"]+debit<=closure["elapsed_ms"]<=10800000,
            "incomplete phased census, unique scopes or common wall clock")
