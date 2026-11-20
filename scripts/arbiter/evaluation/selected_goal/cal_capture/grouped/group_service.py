# SPDX-License-Identifier: Apache-2.0
"""Keep one native recorder and installation lease for a predeclared group of original phase Jobs."""
import os
from pathlib import Path
import subprocess
import sys
import time

from controller import AUDIT_SHA, HERE, NODE_SHA, ROOT, link, owned_module, require, summary
from dispatch_proof import read_journal
from launch import native, wait_stage
from lifetime import binding_identity, linked, warm_closure
from prepare import SELECTOR, pin


def now_ms():
    return time.time_ns() // 1000000


def phase_deadline(start_ms, campaign_deadline_ms):
    return min(start_ms + 1800000, campaign_deadline_ms)


def settle_service(original, shutdown, wait, retain):
    def typed(error):
        return {"type":f"{type(error).__module__}.{type(error).__qualname__}", "message":str(error)}
    failures = []
    for operation, action in (("shutdown",shutdown),("wait",wait)):
        try:
            action()
        except Exception as error:
            failures.append((operation,error))
    if original is not None or failures:
        details = {"original":None if original is None else typed(original),
                   "settlement":[{"operation":operation,**typed(error)} for operation,error in failures],
                   "terminal_failed":True}
        try:
            retain(details)
        except Exception as error:
            failures.append(("retain_failure_receipt",error))
    if original is not None:
        for operation,error in failures:
            original.add_note(f"Additional {operation} failure: {type(error).__name__}: {error}")
    elif failures:
        raise ExceptionGroup("native group settlement failed", [error for _,error in failures])


def run_phase(folder, phase, request, plan, node, env, start_ms, deadline_ms, deadline, fingerprints):
    ready = linked(request["ready"])
    binding = linked(request["bindings"])
    instance = ready["health"]["instance"]
    publication = binding["bindings"][0]["publication"]
    owner = owned_module()
    sources = [link(HERE / n) for n in ("worker.cjs", "phase.cjs", "window_scope.cjs", "controller.py", "prepare.py")]
    sources.append(link(Path(owner.__file__)))
    live = folder / "live-request.json"
    live_ref = link(live)
    require(linked(live_ref) == request, "original group request changed")
    pin(node, NODE_SHA)
    timeout = min(1800, deadline-time.monotonic(), (deadline_ms-now_ms())/1000)
    require(timeout > 125, "phase cannot admit an original request within its remaining time")
    log = folder / f"phase-{phase:02}.jsonl"
    run_env = env | {"AIRK_CAL100_LIVE_REQUEST":str(live), "AIRK_CAL100_PYTHON":sys.executable}
    result = owner.run_evaluation_fixture([node, HERE/"worker.cjs", live, live_ref["sha256"], phase, timeout],
                                          ROOT, log, timeout, run_env)
    result.update(phase=phase, transcript=link(log), request_sha256=live_ref["sha256"], sources=sources,
                  start_ms=start_ms, deadline_ms=deadline_ms, end_ms=now_ms())
    closure = folder/f"phase-{phase:02}.owned.json"
    native.write_json(closure, result)
    require(owner.successful(result) and result["owned_job_final_active_processes"] == 0 and
            result["owned_job_final_member_pids"] == [] and result["end_ms"] <= deadline_ms and
            time.monotonic() <= deadline, "original phase failed, exceeded time or did not settle")
    counts = summary(log, phase, plan["phase_ids"][phase], instance, publication, fingerprints)
    for ref in sources:
        pin(ref["path"], ref["sha256"])
    return link(closure), counts


def run_group(output, group, plan, plan_ref, prior_ref, audit, node, global_deadline, fingerprints, instances):
    original = plan["installation"]
    anchor, stage = linked(original["ready"]), linked(original["status"])
    prior = linked(prior_ref)
    if group == 0:
        require(prior == linked(original["closed"]), "first lifetime lacks the installation closure")
    else:
        require(prior["group"] == group-1 and prior["plan"] == plan_ref and
                prior["phases"] == plan["phase_groups"][group-1] and
                linked(prior["closed"])["service_lease_reacquired"] is True,
                "preceding group lacks its own natural settlement")
    phases = plan["phase_groups"][group]
    work, commands = output/"fixture", output/"commands"
    commands.mkdir()
    start_ms = now_ms()
    end_ms = phase_deadline(start_ms, plan["deadline_ms"])
    deadline = min(global_deadline, time.monotonic()+(end_ms-start_ms)/1000)
    ticket = output/"restart-request.json"
    native.write_json(ticket, {k:{n:original[k][n] for n in ("path","sha256")} for k in ("ready","closed","status")})
    env = {k:v for k,v in os.environ.items() if not k.upper().startswith((
        "AIRK_ARBITER_EVALUATION_", "AIRK_SELECTED_", "AIRK_NATIVE_", "NODE_OPTIONS", "NODE_PATH"))}
    env.update(audit["fixture_environment"])
    env.update(AIRK_ARBITER_EVALUATION_WORK=str(work), AIRK_ARBITER_EVALUATION_RESTART=str(ticket),
               AIRK_ARBITER_EVALUATION_RESTART_SHA256=link(ticket)["sha256"], AIRK_NATIVE_WARM_OBSERVER="true",
               AIRK_NATIVE_DISPATCH_OBSERVER="true", AIRK_MCP_RESPONSE_MODE="full")
    inputs = audit["inputs"]
    service = inputs["service"]["path"]
    argv = [service, "--ignored", "--exact", SELECTOR, "--test-threads", "1"]
    ctl = [inputs["cli"]["path"], "--output", "json", "--service-runtime-dir", anchor["control_root"]]

    def checked_status(label, observation_deadline=None):
        bound = deadline if observation_deadline is None else min(observation_deadline,deadline)
        path, _ = native.run_logged(ctl+["arbiter","status-legacy"], commands, label, env, bound)
        current = native.read_json(path)
        require(current["configuration"] == stage["configuration"] and all(
            current[k]["availability"] == "ready" and
            current[k]["selected_profile"] == current[k]["active_profile"] == stage[k]["active_profile"]
            for k in ("focus","context")), "installed native stages or saved configuration differ")
    probes = searches = 0
    closures = []
    with (output/"service.log").open("xb") as log:
        proc = subprocess.Popen(argv,cwd=ROOT,env=env,stdin=subprocess.DEVNULL,stdout=log,stderr=log,
                                creationflags=subprocess.CREATE_NO_WINDOW)
        try:
            clock = output/"fixture-start.json"
            native.write_json(clock, {"schema":"airk.private.cal100-fixture-start/1", "pid":proc.pid,
                              "start_ms":start_ms, "deadline_ms":start_ms+10800000, "fixture_root":str(work),
                              "input_audit_sha256":AUDIT_SHA, "command":argv})
            native.wait_observed(output, lambda: True if (work/"ready.json").is_file() else None, proc, deadline,
                                 "group native bootstrap")
            ready = native.read_json(work/"ready.json")
            instance = ready["health"]["instance"]
            require(instance not in instances and ready["health"]["process_id"] == proc.pid and
                    ready["health"]["executable_sha256"] == inputs["service"]["sha256"] and
                    all(ready[k] == anchor[k] for k in ("inputs_sha256","control_root","profiles")) and
                    (work/"context-cal-admissions").is_dir(), "unexpected group lifetime owner")
            instances.add(instance)
            for name in ("focus","context"):
                wait_stage(work,proc,ready,name,stage[name]["active_profile"],0,min(deadline,time.monotonic()+120))
            checked_status("stage-before")
            emit_env = env | {"AIRK_ARBITER_EVALUATION_SCOPES":inputs["scopes"]["path"],
                              "AIRK_ARBITER_EVALUATION_SCOPES_SHA256":inputs["scopes"]["sha256"],
                              "AIRK_ARBITER_EVALUATION_PROFILE":stage["context"]["active_profile"],
                              "AIRK_ARBITER_EVALUATION_STAGE":"context","AIRK_ARBITER_EVALUATION_ENABLED":"true",
                              "AIRK_ARBITER_EVALUATION_CONTROL_PREFIX":"enable-context"}
            native.run_logged(native.ignored(service,"emit_private_arbiter_configuration"),commands,
                              "bind-original-scope",emit_env,deadline)
            binding_path = work/"enable-context.bindings.json"
            binding = native.read_json(binding_path)
            require(binding["instance"] == instance and binding_identity(binding) == binding_identity(linked(original["bindings"])),
                    "original publication or scope changed on native restart")
            request = {"schema":"airk.private.cal100-live-fixture/1","fixture_root":str(work),
                       "ready":link(work/"ready.json"),"bindings":link(binding_path),
                       "answer":link(work/"context-request.json"),"start_receipt":link(clock)}
            require(request["answer"]["sha256"] == linked(original["request"])["answer"]["sha256"],
                    "native request template changed")
            native.write_json(output/"live-request.json", request)
            for phase in phases:
                phase_start = start_ms if phase == phases[0] else now_ms()
                if phase != phases[0]:
                    end_ms = phase_deadline(phase_start,plan["deadline_ms"])
                    deadline = min(global_deadline,time.monotonic()+(end_ms-phase_start)/1000)
                closure, counts = run_phase(output,phase,request,plan,node,env,phase_start,end_ms,deadline,fingerprints)
                closures.append(closure)
                probes += counts[0]
                searches += counts[1]
            checked_status("stage-after")
        except Exception as original_error:
            latest = time.monotonic()
            if proc.poll() is None and (work/"ready.json").is_file() and deadline-latest >= 20:
                try:
                    checked_status("failure-stage-at-shutdown",latest+5)
                except Exception as diagnostic:
                    try:
                        native.write_json(output/"failure-status-unavailable.json",{"error":str(diagnostic)})
                    except OSError as error:
                        original_error.add_note(f"failure-state diagnostic could not be retained: {error}")
            raise
        finally:
            def shutdown():
                if proc.poll() is None and (work/"ready.json").is_file():
                    native.run_logged(ctl+["service","shutdown"],commands,"shutdown",env,deadline)
            def wait():
                require(proc.wait(timeout=max(0,min(120,deadline-time.monotonic()))) == 0,
                        "group service failed natural shutdown")
            settle_service(sys.exception(),shutdown,wait,
                           lambda evidence: native.write_json(output/"service-failure.json",evidence))
    completed = now_ms()
    closed = native.read_json(work/"closed.json")
    require(closed["instance"] == instance and closed["shutdown_completed"] is True and
            closed["service_lease_reacquired"] is True, "group native lease did not settle")
    dispatch = read_journal(work,stage["focus"]["active_profile"],setup_only=False)
    warm = warm_closure(work,ready)
    require(completed <= end_ms and now_ms() <= end_ms and time.monotonic() <= deadline and
            dispatch["focus_dispatches"] <= plan["recorder"]["admission_limit"],
            "final phase deadline or native lifetime limit exceeded")
    out = output/"lifetime-closure.json"
    native.write_json(out, {"schema":"airk.private.cal100-group-lifetime/1","group":group,"phases":phases,
                      "phase_closures":closures,"prior":prior_ref,"plan":plan_ref,"instance":instance,
                      "publication":binding["bindings"][0]["publication"],"live_request":link(output/"live-request.json"),
                      "restart":link(ticket),"closed":link(work/"closed.json"),"warm":warm,"dispatch":dispatch,
                      "probes":probes,"pipeline_searches":searches,"start_ms":start_ms,"end_ms":now_ms()})
    return link(out)
