# SPDX-License-Identifier: Apache-2.0
"""Start one CPU CAL observer fixture in a Windows Job and retain its natural closure."""

import argparse
import importlib.util
import json
import os
from pathlib import Path
import subprocess
import sys
import time

from controller import AUDIT_PATH, AUDIT_SHA, CAMPAIGN, HERE, ROOT, capture, link, owned_module, require
from prepare import SELECTOR, pin, read
from dispatch_proof import read_journal

EVALUATION = HERE.parents[1]
spec = importlib.util.spec_from_file_location("compact_fixture", EVALUATION / "run_compact_answer.py")
native = importlib.util.module_from_spec(spec)
spec.loader.exec_module(native)


def wait_stage(work, service, ready, stage, manifest, revision, deadline):
    def observe():
        journal = work / "warm-completions.jsonl"
        require(journal.stat().st_size <= 8 << 20, "warm journal exceeds observer bound")
        with journal.open("rb") as stream:
            for line in stream:
                if not line.endswith(b"\n"):
                    return None
                value = json.loads(line)
                if value["stage"] == stage and value["accepted_configure_revision"] == revision:
                    require(value["profile_manifest"] == manifest and value["observer_id"] == ready["warm_observer_id"]
                            and value["outcome"] == "ready", "native stage warm failed or identity changed")
                    return value
        return None
    return native.wait_observed(work, observe, service, deadline, stage + " original warm completion", file_changes=0x18)


def wait_cal_publication(cli, runs, index_id, process, deadline):
    """Observe CAL vector progress under the original call, count and fixture bounds."""
    import runpy
    helper = ROOT / "scripts/arbiter/analysis/public_source_admission/native_cohort_source_reads.py"
    repeat = runpy.run_path(str(helper))["publication_read"]
    arguments = ("tools", "call", "airk_status", "--index", index_id, "--args", '{"response_mode":"full"}')
    attempts = 0
    previous = None
    scope = None
    interval = 0.5

    def invoke(label, *args):
        nonlocal attempts
        require(attempts < 240 and time.monotonic() < observation_deadline, "CAL status observation budget exhausted")
        attempts += 1
        return cli(label, observation_deadline, *args)

    def read_refusal(path):
        envelope = native.read_json(path)
        body = envelope.get("structuredContent") or {}
        observed = body.get("scope_observation") or {}
        members = observed.get("members", [])
        require(len(members) == 1 and members[0].get("index_id") == index_id and
                observed.get("selection", {}).get("owner") == "agent" and
                (scope is None or observed == scope) and type(body.get("retry_after_ms")) is int and
                1 <= body["retry_after_ms"] <= 1000, "publication refusal lost original native authority")
        return envelope

    while attempts < 240:
        observation_deadline = min(deadline, time.monotonic() + 120)
        result = repeat(invoke, read_refusal, runs, f"published-status-{attempts}", arguments, observation_deadline)
        status = native.tool_body(result)
        observed = status.get("scope_observation") or {}
        members = observed.get("members", [])
        require(status["read_scope"] == {"source": "named", "index_ids": [index_id]} and
                len(members) == 1 and members[0]["index_id"] == index_id and
                observed.get("selection", {}).get("owner") == "agent" and (scope is None or scope == observed),
                "CAL status scope changed")
        scope = observed
        semantic = status["semantic_readiness"]
        if semantic["state"] == "ready":
            require(status["publication"]["token"], "ready CAL roster lacks a real publication")
            return status
        require(semantic["state"] in ("pending", "partial"), "unexpected CAL semantic state")
        # The fleet publishes vectors in batches. Avoid spending the observation
        # budget on an unchanged count; reset cadence immediately upon progress.
        completed = semantic.get("completed_artifacts", 0)
        interval = min(8, interval * 2) if previous == completed else 0.5
        previous = completed
        left = min(interval, deadline - time.monotonic())
        require(left > 0, "CAL fixture expired during publication")
        try:
            process.wait(timeout=left)
        except subprocess.TimeoutExpired:
            continue
        raise ValueError("CAL service exited during publication")
    raise ValueError("CAL status observation budget exhausted")


def check_cal_sources():
    # Import only the maintained, non-executing CAL population preparation API.
    parent = str(HERE.parent)
    sys.path.insert(0, parent)
    try:
        from replay_cal_population import prepare
        return prepare(ROOT)
    finally:
        sys.path.remove(parent)


def execute(output, node, *, setup_only):
    before = check_cal_sources()
    audit = read(AUDIT_PATH, AUDIT_SHA)
    inputs = audit["inputs"]
    for ref in [*inputs.values(), *audit["compiled_sdk"].values()]:
        pin(Path(ref["path"]), ref["sha256"])
    scopes = read(Path(inputs["scopes"]["path"]), inputs["scopes"]["sha256"])
    require(scopes["experiment_partition"] == "CAL" and len(scopes["source_roots"]) == 1, "one CAL source root required")
    service_image = Path(inputs["service"]["path"])
    cli_image = Path(inputs["cli"]["path"])
    work, commands, index = output / "fixture", output / "commands", output / "index"
    commands.mkdir()
    index.mkdir()
    env = {k:v for k,v in os.environ.items() if not k.upper().startswith((
        "AIRK_ARBITER_EVALUATION_", "AIRK_SELECTED_", "AIRK_NATIVE_", "NODE_OPTIONS", "NODE_PATH"))}
    env.update(audit["fixture_environment"])
    env.update(AIRK_ARBITER_EVALUATION_WORK=str(work), AIRK_NATIVE_WARM_OBSERVER="true",
               AIRK_NATIVE_DISPATCH_OBSERVER="true", AIRK_MCP_RESPONSE_MODE="full")
    start_ms = time.time_ns() // 1000000
    deadline = time.monotonic() + 10800
    command = [service_image, "--ignored", "--exact", SELECTOR, "--test-threads", "1"]
    ctl = [cli_image, "--output", "json", "--service-runtime-dir", work / "control"]

    def invoke(label, *args):
        return native.run_logged(ctl + list(args), commands, label, env, deadline)

    def cli(label, *args):
        return native.read_json(invoke(label, *args)[0])

    with (output / "service.log").open("xb") as log:
        process = subprocess.Popen(command,cwd=ROOT,env=env,stdin=subprocess.DEVNULL,
                                   stdout=log,stderr=log,creationflags=subprocess.CREATE_NO_WINDOW)
        try:
            original = output / "fixture-start.json"
            native.write_json(original, {"schema":"airk.private.cal100-fixture-start/1","pid":process.pid,
                              "start_ms":start_ms,"deadline_ms":start_ms+10800000,"fixture_root":str(work),
                              "input_audit_sha256":AUDIT_SHA,"command":[str(v) for v in command]})
            native.wait_observed(output,lambda: True if (work/"ready.json").is_file() else None,
                                 process,deadline,"original CAL fixture readiness")
            ready=native.read_json(work/"ready.json")
            require(ready["health"]["process_id"]==process.pid and ready["health"]["executable_sha256"]==inputs["service"]["sha256"]
                    and ready["inputs_sha256"]==inputs["inputs"]["sha256"] and (work/"context-cal-admissions").is_dir(),
                    "original CAL observer not ready")
            setup_file,_=invoke("setup","--root",scopes["source_roots"][0],"--data-dir",index,
                                "library","setup","--name","CAL100","--watch","off","--apply")
            with setup_file.open("rb") as stream:
                draft,applied=map(json.loads,stream)
            require("draft" in draft and "receipt" in applied,"native setup transaction missing")
            project=applied["receipt"]["request"]["registration"]["binding"]
            index_id=applied["receipt"]["index_id"]
            drafts=output/"semantic-drafts"
            native.run_logged(native.ignored(service_image,"emit_private_selected_vector_configuration"),
                              commands,"emit-minilm", env | {"AIRK_SELECTED_MINILM_DIRECTORY":str(Path(inputs["minilm"]["path"]).parent),
                              "AIRK_SELECTED_VECTOR_DRAFT_OUTPUT":str(drafts)},deadline)
            job=cli("install-minilm","--service-project",project,"configure","--file",drafts/"minilm-install.json")["job"]
            native.wait_job(cli,job["id"],"configure")
            job=cli("index","--service-project",project,"index")["job"]
            native.wait_job(cli,job["id"],"index")
            def status_call(label, original_deadline, *args):
                result, _ = native.run_logged(ctl + list(args), commands, label, env, original_deadline)
                return native.read_json(result)
            status = wait_cal_publication(status_call, commands, index_id, process, deadline)
            require(status["files"] == 753, "CAL source publication count differs")
            if setup_only:
                # The setup probe observes the served pipeline before any stage warms.
                # Full captures prove it on each delivered native Search instead.
                search = native.tool_body(cli("pipeline-observation", "tools", "call", "airk_search", "--index", index_id,
                                          "--args", json.dumps({"query": "source", "limit": 1, "max_bytes": 4096,
                                          "full_chunks": True, "response_mode": "full", "include_status": True})))
                require(search["semantic_readiness"]["state"] == "ready" and
                        search["semantic_readiness"]["pipeline_digest"] == audit["pipeline"] and
                        search["publication"]["token"] == status["publication"]["token"] and
                        search["execution"]["served_mode"] in ("hybrid", "vector") and
                        search["execution"]["incomplete"] is False, "actual default semantic pipeline differs")
            selected = {"focus":audit["profiles"]["arbiter-focus-qwen3-reranker-06b"],
                        "context":audit["profiles"]["arbiter-context-qwen3-17b"]}
            for stage, manifest in selected.items():
                prefix="enable-"+stage
                stage_env=env | {"AIRK_ARBITER_EVALUATION_SCOPES":inputs["scopes"]["path"],
                                 "AIRK_ARBITER_EVALUATION_SCOPES_SHA256":inputs["scopes"]["sha256"],
                                 "AIRK_ARBITER_EVALUATION_PROFILE":manifest,"AIRK_ARBITER_EVALUATION_STAGE":stage,
                                 "AIRK_ARBITER_EVALUATION_ENABLED":"true","AIRK_ARBITER_EVALUATION_CONTROL_PREFIX":prefix}
                native.run_logged(native.ignored(service_image,"emit_private_arbiter_configuration"),
                                  commands,"emit-"+stage,stage_env,deadline)
                config=cli("configure-"+stage,"arbiter","configure","--legacy","--file",work/(prefix+".configure.json"))
                wait_stage(work,process,ready,stage,manifest,config["configuration"]["revision"],
                           min(deadline,time.monotonic()+120))
            state=cli("cal-ready","arbiter","status-legacy")
            require(all(state[k]["availability"]=="ready" and state[k]["active_profile"]==v
                        and state[k]["selected_profile"]==v for k,v in selected.items()),"both native stages must be ready")
            request=output/"live-request.json"
            native.write_json(request,{"schema":"airk.private.cal100-live-fixture/1","fixture_root":str(work),
                              "ready":link(work/"ready.json"),"bindings":link(work/"enable-context.bindings.json"),
                              "answer":link(work/"context-request.json"),"start_receipt":link(original)})
            if not setup_only:
                folder=output/"phases"
                folder.mkdir()
                result=capture(request,link(request)["sha256"],folder,node)
                native.write_json(output/"phases.json",result)
        finally:
            if process.poll() is None and (work/"ready.json").is_file():
                invoke("shutdown","service","shutdown")
            require(process.wait(timeout=max(0,min(120,deadline-time.monotonic())))==0,
                    "CAL fixture failed to exit naturally")
    require(check_cal_sources() == before, "CAL source corpus changed during fixture run")
    closed=native.read_json(work/"closed.json")
    require(closed["instance"]==ready["health"]["instance"] and closed["shutdown_completed"] is True and
            closed["service_lease_reacquired"] is True,"CAL fixture lease did not settle")
    dispatch = read_journal(work, selected["focus"], setup_only=setup_only)
    warm = native.read_json(work/"warm-completions.closed.json")
    require(warm["observer_id"] == ready["warm_observer_id"] and warm["complete"] is True and
            warm["dropped"] == 0 and warm["error"] is None and warm["records"] == warm["last_sequence"] == 2 and
            warm["bytes"] == (work/"warm-completions.jsonl").stat().st_size, "incomplete warm recorder")
    native.write_json(output/"fixture-closure.json", {"instance":closed["instance"],"closed":link(work/"closed.json"),
                      "live_request":link(request),"setup_only":setup_only,"fitted":False,
                      "dispatch":dispatch,"warm_summary":link(work/"warm-completions.closed.json"),
                      "warm_journal":link(work/"warm-completions.jsonl"),
                      "phase_census":None if setup_only else link(output/"phases.json")})


def main():
    parser=argparse.ArgumentParser(description=__doc__)
    parser.add_argument("--output",required=True,type=Path)
    parser.add_argument("--node",required=True,type=Path)
    parser.add_argument("--capture",action="store_true",help="run all 13 reviewed phases; default proves setup only")
    parser.add_argument("--owned",action="store_true",help=argparse.SUPPRESS)
    args=parser.parse_args()
    require(os.name=="nt" and sys.flags.no_site and sys.dont_write_bytecode,"checked Windows Python -B -S required")
    output=args.output
    require(output.is_absolute() and output.is_relative_to(CAMPAIGN) and output.resolve(strict=True)==output and
            output.is_dir(), "fresh checked campaign output directory required")
    if args.owned:
        require(os.environ.get("AIRK_CAL100_OWNER_ROOT")==str(output),"CAL worker must have its original owner")
        with native.capped_worker_job(output):
            execute(output,args.node,setup_only=not args.capture)
        return
    require(not any(output.iterdir()),"CAL attempt output already exists")
    owner=owned_module()
    argv=[sys.executable,"-B","-S",Path(__file__).resolve(),"--output",output,"--node",args.node,"--owned"]
    if args.capture:
        argv.append("--capture")
    result=owner.run_evaluation_fixture(argv,ROOT,output/"outer.log",10800,os.environ | {"AIRK_CAL100_OWNER_ROOT":str(output)})
    native.write_json(output/"outer-owned.json",result)
    require(owner.successful(result) and result["owned_job_final_active_processes"]==0 and
            result["owned_job_final_member_pids"]==[] and (output/"fixture-closure.json").is_file(),
            "original outer owner failed or did not settle")


if __name__=="__main__":
    main()
