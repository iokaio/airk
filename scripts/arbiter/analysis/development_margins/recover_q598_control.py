# SPDX-License-Identifier: Apache-2.0
"""Recover the existing R3 installation after Q598's original pre-Answer configuration supersession."""
import argparse
import json
import os
from pathlib import Path
import runpy
import subprocess
import sys
import time

HERE=Path(__file__).resolve().parent
ROOT=HERE.parents[3]
E=ROOT/"artifacts/v190/final-plan/exec-20261116T052942Z"
PRIOR=E/"compact-development-TRAIN_Q598-focus1024-run-r1"
FIRST=E/"train-q598-closed-control-recovery-r1"
OUT=E/"train-q598-closed-control-recovery-r2"
CAPTURE=runpy.run_path(str(HERE/"capture.py"))
pilot,inputs,_,focus,_=(CAPTURE["checked"](p) for p in CAPTURE["OWNERS"])
require,pin,read=pilot["require"],pilot["pin"],pilot["read"]
reuse_path=HERE/"reuse_installation.py"
pin(reuse_path,CAPTURE["REUSE_SHA"])
reuse=runpy.run_path(str(reuse_path))
control=reuse["CONTROL"]
status_file=PRIOR/"commands/configure-context.stdout"
prior_sha="6866fe743068d17a73d36022ac1fe82f85bbcea5467b301da2a12dfd89447cd8"
status_sha="d4136d5498b7022a8bfd80ff283301cee9d3325dd670be9427fc5475a055352a"


def verify():
    first=FIRST/"owned.json"
    pin(first,"650937ae2a8d1cbdececd1fc0ba00044d3058fee24624ce57b41acdb1ecde165")
    first_result=read(first)
    link=first_result["shared_installation"]
    restored=reuse["linked"](link["closed"])
    require(first_result["exit_code"]==1 and first_result["owned_descendants_settled"] and
            first_result["owned_job_final_active_processes"]==0 and not first_result["owned_job_forced_termination"] and
            restored["shutdown_completed"] and restored["service_lease_reacquired"] and
            Path(restored["control_root"]).samefile(control) and "status" not in link,"wrong first recovery disposition")
    for rel,sha in reuse["linked"](link["control_after"])["files"].items():
        pin(control/rel,sha)
    pin(PRIOR/"owned.json",prior_sha)
    prior=read(PRIOR/"owned.json")
    require(prior["exit_code"]==1 and prior["owned_descendants_settled"] and not prior["timed_out"] and
            not prior["owned_job_forced_termination"] and prior["owned_job_final_active_processes"]==0 and
            prior["owned_job_final_member_pids"]==[] and prior["source_seals_unchanged"],"wrong Q598 terminal Job")
    old=prior["shared_installation"]
    require(Path(old["control_root"])==control and old["anchor_plan_sha256"]==reuse["PLAN"] and "status" not in old
            and not any((PRIOR/"commands").glob("answer*")),"wrong physical predecessor or Answer submitted")
    closed=reuse["linked"](old["closed"])
    ready=reuse["linked"](old["ready"])
    require(ready["health"]["instance"]==closed["instance"] and closed["shutdown_completed"] and
            closed["service_lease_reacquired"] and Path(closed["control_root"]).samefile(control),"Q598 closure differs")
    command=PRIOR/"commands/configure-context.command.json"
    pin(command,"2ddd1db4a6673f343497d33f35b9911488210a10145af3bef9c9b2f3950af549")
    actual=read(command)
    require(actual["exit_code"]==0 and actual["argv"][1:]==["--output","json","--service-runtime-dir",str(control),
            "arbiter","configure","--legacy","--file",str(PRIOR/"fixture/enable-context.configure.json")],
            "actual configure response origin differs")
    pin(status_file,status_sha)
    status=read(status_file)
    after=reuse["linked"](old["control_after"])
    configuration=control/"arbiter-v3/configuration.json"
    pin(configuration,after["files"]["arbiter-v3/configuration.json"])
    require(status["configuration"]==read(configuration) and status["configuration"]["revision"]==11 and
            status.get("initialization") is None and all(status[k]["queued"]==status[k]["running"]==0 for k in ("focus","context"))
            and status["context"]["availability"]=="requested","last immutable native configure response differs")
    journal=PRIOR/"fixture/warm-completions.jsonl"
    pin(journal,"509d7a605f337d4690ba6614da7f37baf639c9bf87617cbf5034fc2cd3f0838b")
    descriptor=inputs["validate_one"]("TRAIN_Q598")
    descriptor=focus["bind"](descriptor,inputs["digest"](inputs["encode"](descriptor)))
    require(actual["argv"][0]==descriptor["runtime_inputs"]["cli"]["path"],"wrong original matched CLI")
    base,_,immutable=reuse["inventory"](descriptor)
    request={k:{n:base[k][n] for n in ("path","sha256")} for k in ("ready","closed")}
    request["status"]={"path":str(status_file),"sha256":status_sha}
    return descriptor,request,immutable


def worker():
    descriptor,request,_=verify()
    native=runpy.run_path(str(pilot["NATIVE"]))
    assets=descriptor["runtime_inputs"]
    work,logs=OUT/"fixture",OUT/"commands"
    logs.mkdir()
    env={k:v for k,v in os.environ.items() if not k.upper().startswith("AIRK_")}
    env.update(descriptor["required_environment"])
    req=OUT/"restart-request.json"
    native["write_json"](req,request)
    env.update(AIRK_ARBITER_EVALUATION_INPUTS=assets["inputs"]["path"],AIRK_ARBITER_EVALUATION_INPUTS_SHA256=assets["inputs"]["sha256"],
               AIRK_ARBITER_EVALUATION_WORK=str(work),AIRK_ARBITER_EVALUATION_SECONDS="180",
               AIRK_ARBITER_EVALUATION_RESTART=str(req),AIRK_ARBITER_EVALUATION_RESTART_SHA256=pin(req),
               AIRK_ARBITER_EVALUATION_RESOURCE_CONSENT="throughput64gib4threads",AIRK_MCP_RESPONSE_MODE="full")
    until=time.monotonic()+180
    command=[assets["service"]["path"],"--ignored","--exact",descriptor["native_selector"],"--test-threads","1"]
    def native_cli(name,*args):
        path,_=native["run_logged"]([assets["cli"]["path"],"--output","json","--service-runtime-dir",str(control),*args],logs,name,env,until)
        return read(path)
    with (OUT/"service.log").open("xb") as sink, subprocess.Popen(command,cwd=ROOT,env=env,stdin=subprocess.DEVNULL,
             stdout=sink,stderr=sink,creationflags=subprocess.CREATE_NO_WINDOW) as process:
        try:
            wait=native["wait_observed"]
            wait(OUT,lambda:True if (work/"ready.json").is_file() else None,process,until,"actual recovery readiness")
            ready=read(work/"ready.json")
            previous=read(PRIOR/"fixture/ready.json")
            require(Path(ready["control_root"]).samefile(control) and ready["profiles"]==previous["profiles"] and
                    ready["health"]["instance"]!=previous["health"]["instance"] and not (work/"control").exists(),
                    "recovery did not bind existing installation")
            saved=read(work/"restart-owner.json")
            original=read(status_file)["configuration"]
            require(saved["configuration"]==original,"actual native recovered configuration changed")
            def both_warmed():
                j=work/"warm-completions.jsonl"
                if not j.is_file():
                    return None
                raw=j.read_bytes()
                require(len(raw)<=8<<20,"warm observer exceeded original envelope")
                if not raw.endswith(b"\n"):
                    return None
                rows=[json.loads(line) for line in raw.splitlines()]
                for row in rows:
                    require(row["observer_id"]==ready["warm_observer_id"] and row["outcome"]=="ready" and
                            row["profile_manifest"]==original[row["stage"]]["selected_profile"],"original recovery warm refused")
                return True if {r["stage"] for r in rows}=={"focus","context"} else None
            wait(OUT,both_warmed,process,until-15,"independent original Focus and Context rehydration",file_changes=0x18)
            status=native_cli("reuse-final-status","arbiter","status-legacy")
            require(status["configuration"]==original and status.get("initialization") is None and
                    all(status[k]["queued"]==status[k]["running"]==0 and status[k]["availability"]=="ready"
                        and status[k]["active_profile"]==original[k]["selected_profile"] for k in ("focus","context")),
                    "real recovery did not reach original native ready/idle stages")
        finally:
            if process.poll() is None and (work/"ready.json").is_file():
                native_cli("shutdown","service","shutdown")
        require(process.wait(timeout=max(0,until-time.monotonic()))==0,"native recovery did not close")
    closed=read(work/"closed.json")
    require(closed["instance"]==ready["health"]["instance"] and closed["service_lease_reacquired"] and closed["shutdown_completed"],
            "native lease not reacquired")


def launch():
    require(os.environ.get("AIRK_TRAIN_REUSE_AUDITED_CONTROL")==str(control) and not OUT.exists(),
            "checked control custody, active shared owner and fresh recovery folder required")
    _,_,immutable=verify()
    pin(pilot["NATIVE"],pilot["NATIVE_SHA"])
    n=runpy.run_path(str(pilot["NATIVE"]))
    mutex=runpy.run_path(str(ROOT/"scripts/arbiter/evaluation/selected_goal/cal_capture/grouped/installation_mutex.py"))
    def link(p):
        return {"path":str(p),"sha256":pin(p)}
    with mutex["InstallationMutex"]():
        _,_,immutable=verify()
        OUT.mkdir()
        before=reuse["custody"](OUT,immutable,True)
        handler=Path(__file__).resolve()
        result=n["owner_module"]().run_evaluation_fixture([sys.executable,"-I","-B","-S",handler,"--worker"],
                  ROOT,OUT/"outer.log",240,os.environ.copy())
        after=reuse["custody"](OUT,immutable,False)
        shared={"control_root":str(control),"anchor_plan_sha256":reuse["PLAN"],"previous":link(FIRST/"owned.json"),"original_train":link(PRIOR/"owned.json"),
                "control_before":before,"control_after":after}
        for k,p in {"ready":"fixture/ready.json","closed":"fixture/closed.json","saved":"fixture/restart-owner.json",
                    "restart_request":"restart-request.json","status":"commands/reuse-final-status.stdout"}.items():
            if (OUT/p).is_file():
                shared[k]=link(OUT/p)
        result.update(shared_installation=shared,source=link(handler),scope="native non-Answer control recovery",answer_requests=0)
        n["write_json"](OUT/"owned.json",result)
        require(n["owner_module"]().successful(result) and "status" in shared and "closed" in shared,"recovery failed")
    n["write_json"](OUT/"control-mutex-released.json",{"name":mutex["MUTEX_NAME"],"owner_pid":os.getpid(),"owned":link(OUT/"owned.json")})


if __name__=="__main__":
    p=argparse.ArgumentParser(description=__doc__)
    p.add_argument("--worker",action="store_true")
    p.add_argument("--preflight",action="store_true")
    args=p.parse_args()
    if args.preflight:
        d,r,_=verify()
        print(json.dumps({"request":r,"selector":d["native_selector"]}))
    elif args.worker:
        worker()
    else:
        launch()
