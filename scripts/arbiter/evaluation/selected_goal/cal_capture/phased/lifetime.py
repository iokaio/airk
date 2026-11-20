# SPDX-License-Identifier: Apache-2.0
"""Run one predeclared phase under a freshly restarted, independently closed observer."""
import os
from pathlib import Path
import subprocess
import sys
import time

from controller import AUDIT_SHA, HERE, NODE_SHA, ROOT, link, owned_module, require, summary
from dispatch_proof import read_journal
from launch import native, wait_stage
from prepare import SELECTOR, pin, read


def linked(ref):
    return read(Path(ref["path"]), ref["sha256"])


def binding_identity(binding):
    row, = binding["bindings"]
    status = row["status_result"]["structuredContent"]
    scope = dict(status["scope_observation"])
    fingerprint = scope.pop("fingerprint")
    require(len(fingerprint) == 64, "missing native fingerprint")
    return ({k: v for k, v in row.items() if k != "status_result"},
            {k: status[k] for k in ("files", "indexed_vectors", "publication", "semantic_readiness", "read_scope")},
            scope)


def warm_closure(work, ready):
    end = native.read_json(work / "warm-completions.closed.json")
    journal = link(work / "warm-completions.jsonl")
    require(end["complete"] is True and end["dropped"] == 0 and end["error"] is None and
            end["observer_id"] == ready["warm_observer_id"] and end["records"] == end["last_sequence"] == 2 and
            end["bytes"] == journal["bytes"], "native warm journal did not settle")
    return {"summary": link(work / "warm-completions.closed.json"), "journal": journal}


def run(output, phase, plan, plan_ref, prior, audit, node, global_deadline, fingerprints, instances):
    original = plan["installation"]
    anchor = linked(original["ready"])
    stage = linked(original["status"])
    work = output / "fixture"
    commands = output / "commands"
    commands.mkdir()
    start_ms = time.time_ns() // 1000000
    deadline = min(global_deadline, time.monotonic() + 1800)
    source = [link(HERE / name) for name in ("worker.cjs", "phase.cjs", "window_scope.cjs", "controller.py", "prepare.py")]
    owner = owned_module()
    source.append(link(Path(owner.__file__)))
    request = output / "restart-request.json"
    native.write_json(request, {k: {n: original[k][n] for n in ("path", "sha256")} for k in ("ready", "closed", "status")})
    env = {k: v for k, v in os.environ.items() if not k.upper().startswith((
        "AIRK_ARBITER_EVALUATION_", "AIRK_SELECTED_", "AIRK_NATIVE_", "NODE_OPTIONS", "NODE_PATH"))}
    env.update(audit["fixture_environment"])
    env.update(AIRK_ARBITER_EVALUATION_WORK=str(work), AIRK_ARBITER_EVALUATION_RESTART=str(request),
               AIRK_ARBITER_EVALUATION_RESTART_SHA256=link(request)["sha256"], AIRK_NATIVE_WARM_OBSERVER="true",
               AIRK_NATIVE_DISPATCH_OBSERVER="true", AIRK_MCP_RESPONSE_MODE="full")
    inputs = audit["inputs"]
    service = inputs["service"]["path"]
    argv = [service, "--ignored", "--exact", SELECTOR, "--test-threads", "1"]
    ctl = [inputs["cli"]["path"], "--output", "json", "--service-runtime-dir", anchor["control_root"]]
    def status(label):
        path, _ = native.run_logged(ctl + ["arbiter", "status-legacy"], commands, label, env, deadline)
        current = native.read_json(path)
        require(current["configuration"] == stage["configuration"] and all(
            current[name]["availability"] == "ready" and
            current[name]["selected_profile"] == current[name]["active_profile"] == stage[name]["active_profile"]
            for name in ("focus", "context")), "active stage or saved configuration differs")
    with (output / "service.log").open("xb") as log:
        proc = subprocess.Popen(argv, cwd=ROOT, env=env, stdin=subprocess.DEVNULL, stdout=log, stderr=log,
                                creationflags=subprocess.CREATE_NO_WINDOW)
        try:
            clock = output / "fixture-start.json"
            native.write_json(clock, {"schema":"airk.private.cal100-fixture-start/1", "pid":proc.pid,
                              "start_ms":start_ms, "deadline_ms":start_ms+10800000, "fixture_root":str(work),
                              "input_audit_sha256":AUDIT_SHA, "command":argv})
            native.wait_observed(output, lambda: True if (work / "ready.json").is_file() else None,
                                 proc, deadline, "CAL phase native restart")
            ready = native.read_json(work / "ready.json")
            instance = ready["health"]["instance"]
            require(instance not in instances and ready["health"]["process_id"] == proc.pid and
                    ready["health"]["executable_sha256"] == inputs["service"]["sha256"] and
                    all(ready[k] == anchor[k] for k in ("inputs_sha256", "control_root", "profiles")) and
                    (work / "context-cal-admissions").is_dir(), "native restart differs from installation")
            instances.add(instance)
            for name in ("focus", "context"):
                wait_stage(work, proc, ready, name, stage[name]["active_profile"], 0, min(deadline, time.monotonic()+120))
            status("stage-before")
            emit_env = env | {"AIRK_ARBITER_EVALUATION_SCOPES":inputs["scopes"]["path"],
                              "AIRK_ARBITER_EVALUATION_SCOPES_SHA256":inputs["scopes"]["sha256"],
                              "AIRK_ARBITER_EVALUATION_PROFILE":stage["context"]["active_profile"],
                              "AIRK_ARBITER_EVALUATION_STAGE":"context", "AIRK_ARBITER_EVALUATION_ENABLED":"true",
                              "AIRK_ARBITER_EVALUATION_CONTROL_PREFIX":"enable-context"}
            native.run_logged(native.ignored(service, "emit_private_arbiter_configuration"), commands,
                              "bind-original-scope", emit_env, deadline)
            bindings = work / "enable-context.bindings.json"
            bound = native.read_json(bindings)
            require(bound["instance"] == instance and binding_identity(bound) == binding_identity(linked(original["bindings"])),
                    "restarted native source publication, membership, scope or semantic pipeline differs")
            live = output / "live-request.json"
            native.write_json(live, {"schema":"airk.private.cal100-live-fixture/1", "fixture_root":str(work),
                              "ready":link(work/"ready.json"), "bindings":link(bindings),
                              "answer":link(work/"context-request.json"), "start_receipt":link(clock)})
            live_ref = link(live)
            pin(node, NODE_SHA)
            remaining = min(deadline-time.monotonic(), (plan["deadline_ms"]-time.time_ns()//1000000)/1000)
            require(remaining > 125, "shared campaign clock cannot admit original call")
            transcript = output / f"phase-{phase:02}.jsonl"
            run_env = env | {"AIRK_CAL100_LIVE_REQUEST":str(live), "AIRK_CAL100_PYTHON":sys.executable}
            result = owner.run_evaluation_fixture([node, HERE/"worker.cjs", live, live_ref["sha256"], phase, remaining],
                                                  ROOT, transcript, remaining, run_env)
            result.update(phase=phase, transcript=link(transcript), request_sha256=live_ref["sha256"], sources=source)
            phase_owned = output / f"phase-{phase:02}.owned.json"
            native.write_json(phase_owned, result)
            require(owner.successful(result) and result["owned_job_final_active_processes"] == 0 and
                    result["owned_job_final_member_pids"] == [], "original phase failed or Job did not settle")
            pub = bound["bindings"][0]["publication"]
            probes, searches = summary(transcript, phase, plan["phase_ids"][phase], instance, pub, fingerprints)
            status("stage-after")
        finally:
            if proc.poll() is None and (work/"ready.json").is_file():
                native.run_logged(ctl + ["service","shutdown"], commands, "shutdown", env, deadline)
            require(proc.wait(timeout=max(0, min(120, deadline-time.monotonic()))) == 0, "phase service failed natural shutdown")
    for ref in source:
        pin(ref["path"], ref["sha256"])
    closed = native.read_json(work/"closed.json")
    require(closed["instance"] == instance and closed["shutdown_completed"] is True and
            closed["service_lease_reacquired"] is True, "phase native lease did not settle")
    out = output / "lifetime-closure.json"
    native.write_json(out, {"schema":"airk.private.cal100-phase-lifetime/1", "phase":phase, "plan":plan_ref,
                      "prior":prior, "restart":link(request), "instance":instance, "publication":pub,
                      "live_request":live_ref, "phase_closure":link(phase_owned), "closed":link(work/"closed.json"),
                      "warm":warm_closure(work, ready), "dispatch":read_journal(work, stage["focus"]["active_profile"], setup_only=False),
                      "probes":probes, "pipeline_searches":searches, "start_ms":start_ms, "end_ms":time.time_ns()//1000000})
    require(time.monotonic() <= deadline, "phase exceeded bounded lifetime")
    return link(out)
