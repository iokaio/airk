# SPDX-License-Identifier: Apache-2.0
"""Use the native same-owner restart for a two-grant status-only comparison."""
import argparse
import os
from pathlib import Path
import subprocess
import sys
import time

from controller import AUDIT_PATH, AUDIT_SHA, CAMPAIGN, HERE, ROOT, link, owned_module, require
from launch import native, wait_stage
from prepare import SELECTOR, pin, read

R1 = CAMPAIGN / "cal100-focus1024-capture-r1"
ORIGINAL = {
    "ready": ("fixture/ready.json", "1833658872eb5e99adf0d1b2f62145742f3dd2242ac7df2a353cb342bf1e237c"),
    "closed": ("fixture/closed.json", "99dc66aeeee18db65dbae35f2e2cd825072e0dc0180fe96358ee506bfd02377f"),
    "status": ("commands/cal-ready.stdout", "d791a1988e63bc2c7c3e5d3d3a386adafea6a3ef819054a623c2808f55680ddb"),
}
BINDING = "3eaeb0b1f9ac922f30d1d44b7f003aa41ab8e2ab5aed9231b5e19c209f034ae3"


def execute(output, node):
    audit = read(AUDIT_PATH, AUDIT_SHA)
    for entry in [*audit["inputs"].values(), *audit["compiled_sdk"].values()]:
        pin(entry["path"], entry["sha256"])
    restart = output / "restart-request.json"
    documents = {name: pin(R1 / path, digest) for name, (path, digest) in ORIGINAL.items()}
    native.write_json(restart, {name:{"path":p["path"], "sha256":p["sha256"]} for name,p in documents.items()})
    old = read(Path(documents["ready"]["path"]), documents["ready"]["sha256"])
    original_status = read(Path(documents["status"]["path"]), documents["status"]["sha256"])
    binding = pin(R1 / "fixture/enable-context.bindings.json", BINDING)
    work = output / "fixture"
    env = {k:v for k,v in os.environ.items() if not k.upper().startswith((
        "AIRK_ARBITER_EVALUATION_", "AIRK_SELECTED_", "AIRK_NATIVE_", "NODE_OPTIONS", "NODE_PATH"))}
    env.update(audit["fixture_environment"])
    env.update(AIRK_ARBITER_EVALUATION_WORK=str(work), AIRK_ARBITER_EVALUATION_RESTART=str(restart),
               AIRK_ARBITER_EVALUATION_RESTART_SHA256=link(restart)["sha256"], AIRK_NATIVE_WARM_OBSERVER="true",
               AIRK_NATIVE_DISPATCH_OBSERVER="true")
    service = audit["inputs"]["service"]["path"]
    cli = audit["inputs"]["cli"]["path"]
    ctl = [cli, "--output", "json", "--service-runtime-dir", old["control_root"]]
    commands = output / "commands"
    commands.mkdir()
    deadline = time.monotonic() + 300
    with (output/"service.log").open("xb") as service_log:
        proc = subprocess.Popen([service, "--ignored", "--exact", SELECTOR, "--test-threads", "1"],
                                cwd=ROOT, stdout=service_log, stderr=service_log, env=env,
                                creationflags=subprocess.CREATE_NO_WINDOW)
        try:
            native.wait_observed(output, lambda: True if (work/"ready.json").exists() else None,
                                 proc, deadline, "original CAL source restart")
            ready = native.read_json(work/"ready.json")
            require(ready["health"]["process_id"] == proc.pid and ready["control_root"] == old["control_root"]
                    and ready["health"]["instance"] != old["health"]["instance"] and
                    ready["profiles"] == old["profiles"], "restart did not bind original indexed state")
            ticket = output/"dry-request.json"
            native.write_json(ticket, {"schema":"airk.private.cal1024-window-scope-dry/1",
                                      "ready":link(work/"ready.json"), "bindings":binding})
            def observe_stages(suffix):
                path, _ = native.run_logged(ctl + ["arbiter","status-legacy"], commands, suffix, env, deadline)
                state = native.read_json(path)
                require(state["configuration"] == original_status["configuration"] and
                        all(state[name]["selected_profile"] == original_status[name]["selected_profile"]
                            for name in ("focus","context")), "dry stage configuration or eligibility differs")
                return state
            for name in ("focus","context"):
                wait_stage(work, proc, ready, name, original_status[name]["active_profile"], 0, deadline)
            before = observe_stages("stage-before")
            result = owned_module().run_evaluation_fixture(
                [node, HERE/"window_scope_dry.cjs", ticket, link(ticket)["sha256"]], ROOT, output/"scope.jsonl",
                min(90, deadline-time.monotonic()), env | {"AIRK_CAL_DRY_TICKET":str(ticket)})
            native.write_json(output/"scope-owned.json", result)
            require(owned_module().successful(result) and not result["owned_job_final_member_pids"], "native scope dry comparison failed")
            after = observe_stages("stage-after")
            require(all(before[name]["active_profile"] == after[name]["active_profile"] and
                        before[name]["availability"] == after[name]["availability"] == "ready"
                        for name in ("focus","context")), "both original stages must be active in the dry comparison")
        finally:
            if proc.poll() is None and (work/"ready.json").exists():
                native.run_logged(ctl + ["service","shutdown"], commands, "shutdown", env, deadline)
            require(proc.wait(timeout=max(1,min(90, deadline-time.monotonic()))) == 0, "dry fixture did not close")
    closed = native.read_json(work/"closed.json")
    require(closed["shutdown_completed"] and closed["service_lease_reacquired"], "dry native lease did not settle")
    journal = native.read_json(work/"composed-observer.closed.json")
    require(journal["complete"] and journal["records"] == journal["dropped"] == journal["writer_failed"] == 0,
            "status-only scope comparison observed scoring work")


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("--output", type=Path, required=True)
    parser.add_argument("--node", type=Path, required=True)
    parser.add_argument("--owned", action="store_true")
    args = parser.parse_args()
    output = args.output
    require(os.name == "nt" and sys.dont_write_bytecode and sys.flags.no_site and output.parent == CAMPAIGN
            and output.resolve(strict=True) == output, "checked private Python execution required")
    from controller import NODE_SHA
    pin(args.node,NODE_SHA)
    if args.owned:
        require(os.environ.get("AIRK_CAL_DRY_ROOT") == str(output), "original checked Job required")
        with native.capped_worker_job(output):
            execute(output,args.node)
    else:
        require(not any(output.iterdir()), "fresh dry output required")
        owner=owned_module()
        command=[sys.executable,"-B","-S",Path(__file__).resolve(),"--owned","--output",output,"--node",args.node]
        result=owner.run_evaluation_fixture(command,ROOT,output/"outer.log",330,os.environ|{"AIRK_CAL_DRY_ROOT":str(output)})
        native.write_json(output/"outer-owned.json",result)
        require(owner.successful(result) and not result["owned_job_final_member_pids"], "native dry owner failed")


if __name__ == "__main__":
    main()
