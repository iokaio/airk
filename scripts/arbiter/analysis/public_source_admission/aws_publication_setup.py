# SPDX-License-Identifier: Apache-2.0
"""Native offline AWS source indexing and same-publication census; no retrieval calls."""
import hashlib
import json
import os
from pathlib import Path
import subprocess
import time


def link(path):
    with path.open("rb") as stream:
        return {"path": str(path), "sha256": hashlib.file_digest(stream, "sha256").hexdigest()}


def source_closure(w, owner, menu):
    n = menu["native"]
    manifest = n["CAMPAIGN"] / menu["INVENTORY"]
    n["checked"](manifest, menu["INVENTORY_SHA"])
    name, cases, members = menu["inputs"](w["read_json"](manifest))
    tree = owner.plain(n["CAMPAIGN"] / name)
    paths = {p.relative_to(tree).as_posix() for p in tree.rglob("*") if p.is_file()}
    n["require"](paths == members.keys(), "private full source directory is incomplete")
    for path, data in members.items():
        n["checked"](owner.plain(tree / path, regular=True), data["sha256"])
    return tree, cases


def wait_job(w, cli, submitted, service, deadline):
    """Sample this native job until its terminal state or the original monotonic deadline."""
    number = 0
    while True:
        job = cli(f"job-{submitted['kind']}-{number:04d}", "jobs", "get", submitted["id"])["job"]
        w["require"]((job["id"], job["kind"]) == (submitted["id"], submitted["kind"]), "native job identity changed")
        if job["state"] == "succeeded":
            w["require"](job["outcome"]["state"] == "succeeded", "native job outcome is incomplete")
            return job
        w["require"](job["state"] in ("running", "pending"), "native offline job failed")
        remaining = deadline - time.monotonic()
        w["require"](remaining > 0, "offline publication phase deadline reached")
        try:
            service.wait(timeout=min(10, remaining))
        except subprocess.TimeoutExpired:
            number += 1
        else:
            raise ValueError("native service exited before job completion")


def source_status(w, cli, index, label):
    arguments = json.dumps({"response_mode": "full", "include_status": True})
    status = w["tool_body"](cli(label, "tools", "call", "airk_status", "--index", index, "--args", arguments))
    w["require"](status["discovery_readiness"]["state"] == "ready"
                 and status["semantic_readiness"]["state"] == "disabled"
                 and status["index_state"] == "available" and type(status["files"]) is int
                 and 0 < status["files"] <= 35515,
                 "complete source-only publication not observed")
    return status


def setup(w, cli, command, output, tree, process, deadline):
    data = output / "index"
    data.mkdir()
    result = command("register", "--root", tree, "--data-dir", data, "library", "setup",
                     "--name", "aws-full-source-visibility", "--watch", "off", "--apply")
    with result.open("rb") as stream:
        draft, applied = map(json.loads, stream)
    w["require"]("draft" in draft, "missing real registration draft")
    registration = applied["receipt"]
    project = registration["request"]["registration"]["binding"]
    index = registration["index_id"]
    off = output / "source-only-vectors.json"
    w["write_json"](off, {"operation": "vectors", "enabled": False})
    config = cli("configure-source-only", "--service-project", project, "configure", "--file", off)["job"]
    w["require"](config["kind"] == "configure", "wrong native vector-configuration job")
    wait_job(w, cli, config, process, deadline)
    job = cli("index", "--service-project", project, "index")["job"]
    w["require"](job["kind"] == "index", "wrong native source indexing job")
    done = wait_job(w, cli, job, process, deadline)
    status = source_status(w, cli, index, "source-publication")
    w["require"](status["publication"]["token"] == done["outcome"]["revision"],
                 "source index job and ready discovery publication differ")
    return {"project": project, "index": index, "source_root": str(tree), "store_root": str(data),
            "source_configuration": {"vectors": False, "watch": "off"},
            "registration": link(result), "source_job": done, "status": status}


def census(w, menu, cli, output, source, cases, tree):
    index = source["index"]
    first = source_status(w, cli, index, "resume-publication")
    publication = first["publication"]["token"]
    w["require"](publication == source["status"]["publication"]["token"]
                 and first["scope_observation"] == source["status"]["scope_observation"],
                 "settled publication or scope changed across restart")

    def call(label, tool, args):
        body = w["tool_body"](cli(label, "tools", "call", tool, "--index", index, "--args", json.dumps(args)))
        w["require"](body["publication"]["token"] == publication, "native source publication changed")
        return body

    rows = menu["observe"](menu["COHORT"], cases, tree, call)
    last = source_status(w, cli, index, "complete-publication")
    w["require"](last["publication"]["token"] == publication
                 and last["scope_observation"] == first["scope_observation"], "final publication changed")
    return {"index": index, "publication": publication, "rows": rows,
            "source_configuration": source["source_configuration"], "complete_source_inventory": True,
            "ranking_observed": False, "scored_requests": 0}


def execute(w, owner, menu, args, output, deadline):
    n = menu["native"]
    # The original source-only census has separate clocks for setup and each read.
    # Reserve closure time inside the outside Job's deadline, including on refusal.
    phase_deadline = deadline - 100
    close_deadline = deadline - 10
    tree, cases = source_closure(w, owner, menu)
    plan_path = n["CAMPAIGN"] / "fp32-paired-answer-r1/cpu-launch-manifest-bound.json"
    n["checked"](plan_path, n["PLAN"])
    plan = w["read_json"](plan_path)
    binaries = {k: w["pin"](owner, plan[k]) for k in ("service", "cli", "inputs")}
    env = {k: v for k, v in os.environ.items() if not k.upper().startswith(("AIRK_", "RUST_LOG"))}
    work = output / "fixture"
    control = work / "control"
    remaining = int(close_deadline - time.monotonic())
    w["require"](remaining > 120, "insufficient window after source authentication")
    env.update(AIRK_ARBITER_EVALUATION_INPUTS=str(binaries["inputs"]),
               AIRK_ARBITER_EVALUATION_INPUTS_SHA256=plan["inputs"]["sha256"],
               AIRK_ARBITER_EVALUATION_WORK=str(work), AIRK_ARBITER_EVALUATION_SECONDS=str(remaining + 5),
               AIRK_ARBITER_EVALUATION_RESOURCE_CONSENT="throughput64gib4threads", AIRK_MCP_RESPONSE_MODE="full")
    source = None
    if args.phase == "census":
        checkpoint = w["pin"](owner, {"path": str(args.checkpoint), "sha256": args.checkpoint_sha256}, output.parent)
        saved = w["read_json"](checkpoint)
        w["require"](saved["phase"] == "setup" and saved["source_receipt"] == menu["INVENTORY_SHA"]
                     and saved["native_pins"] == {k: plan[k]["sha256"] for k in binaries},
                     "setup checkpoint source or image differs")
        prior = w["pin"](owner, saved["owned"], output.parent)
        observed = w["read_json"](prior)
        require_settlement(w, observed)
        details = w["read_json"](w["pin"](owner, saved["data"], output.parent))
        source = details["result"]
        w["require"](Path(source["source_root"]).samefile(tree), "source root changed")
        restart = {k: details[k] for k in ("ready", "closed", "arbiter_status")}
        for pin in restart.values():
            w["pin"](owner, pin, output.parent)
        restart_path = output / "restart.json"
        w["write_json"](restart_path, {"ready": restart["ready"], "closed": restart["closed"],
                                       "status": restart["arbiter_status"]})
        previous = w["read_json"](Path(restart["ready"]["path"]))
        control = owner.plain(Path(previous["control_root"]))
        env.update(AIRK_ARBITER_EVALUATION_RESTART=str(restart_path),
                   AIRK_ARBITER_EVALUATION_RESTART_SHA256=owner.digest(restart_path))
    runs = output / "commands"
    runs.mkdir()

    def command(label, *extra, closing=False):
        limit = close_deadline if closing else phase_deadline
        return w["run_logged"]([binaries["cli"], "--output", "json", "--service-runtime-dir", control, *extra],
                               runs, label, env, limit)[0]

    def read(label, *extra):
        return w["read_json"](command(label, *extra))

    def cli(label, *extra):
        if extra[:2] == ("tools", "call"):
            return n["publication_read"](read, w["read_json"], runs, label, extra, phase_deadline)
        return read(label, *extra)

    with (output / "service.log").open("xb") as log:
        process = subprocess.Popen(w["fixture_command"](binaries["service"], "cpu"), cwd=n["ROOT"], env=env,
                                   stdout=log, stderr=log, stdin=subprocess.DEVNULL,
                                   creationflags=subprocess.CREATE_NO_WINDOW | subprocess.BELOW_NORMAL_PRIORITY_CLASS)
        try:
            w["wait_observed"](output, lambda: True if (work / "ready.json").is_file() else None,
                               process, phase_deadline, "private service ready")
            ready = w["read_json"](work / "ready.json")
            w["require"](ready["health"]["process_id"] == process.pid
                         and ready["health"]["executable_sha256"] == plan["service"]["sha256"]
                         and ready["inputs_sha256"] == plan["inputs"]["sha256"]
                         and control.samefile(ready["control_root"]), "private service identity differs")
            if source is not None:
                w["require"](ready["health"]["instance"] != previous["health"]["instance"]
                             and ready["health"]["catalog_clock"]["namespace"]
                             == previous["health"]["catalog_clock"]["namespace"],
                             "native restart did not produce a new instance on the same catalog")
            result = (setup(w, cli, command, output, tree, process, phase_deadline) if source is None else
                      census(w, menu, cli, output, source, cases, tree))
            arbiter = command("restart-arbiter-status", "arbiter", "status-legacy")
        finally:
            if process.poll() is None and (work / "ready.json").is_file():
                command("shutdown", "service", "shutdown", closing=True)
            w["require"](process.wait(timeout=max(0, close_deadline - time.monotonic())) == 0,
                         "native public source service failed to settle")
    closed = w["read_json"](work / "closed.json")
    w["require"](closed["instance"] == ready["health"]["instance"] and closed["shutdown_completed"]
                 and closed["service_lease_reacquired"], "native owner closure differs")
    w["write_json"](output / "phase.private.json",
                    {"phase": args.phase, "result": result, "ready": link(work / "ready.json"),
                     "closed": link(work / "closed.json"), "arbiter_status": link(arbiter)})


def require_settlement(w, result):
    w["require"](result["exit_code"] == 0 and not result["timed_out"] and not result["log_oversized"]
                 and result["child_assigned_before_resume"] and result["owned_descendants_settled"]
                 and not result["owned_job_forced_termination"] and result["owned_job_final_active_processes"] == 0
                 and result["owned_job_final_member_pids"] == [], "offline native job did not settle")
