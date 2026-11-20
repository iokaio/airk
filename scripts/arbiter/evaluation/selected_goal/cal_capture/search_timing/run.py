# SPDX-License-Identifier: Apache-2.0
"""Measure real native retrieval mode intervals on the retired CAL publication."""
import argparse
import json
import os
from pathlib import Path
import runpy
import subprocess
import sys
import time

sys.path.insert(0,str(Path(__file__).resolve().parents[1]/"timing"))
from run import (CAMPAIGN, ROOT, SELECTOR, InstallationMutex, MUTEX_NAME, link, linked, native,
                 now, owned_module, pin, read_journal, require, require_parent_owns, wait_stage)
from group_service import settle_service

ATTEMPT=CAMPAIGN/"cal100-focus1024-timing-ab-r2"
REPORT=ATTEMPT.with_name(ATTEMPT.name+"-postclosure-analysis-r1.json")
REPORT_SHA="d2bb6cba0e02213b8040409435ab2e05c229bf0f4a4e2092320be54b00873ab5"
MODES=("hybrid","lexical","vector","vector","lexical","hybrid")


def prepare(out):
    report=linked(pin(REPORT,REPORT_SHA))
    original=linked(report["original_plan"])
    outer=linked(report["original_outer"])
    previous=linked(outer["control_after"])
    ready=linked(report["native"]["ready"])
    closure=linked(report["native"]["closed"])
    require(closure["instance"]==ready["health"]["instance"] and closure["service_lease_reacquired"] and
            closure["shutdown_completed"] and outer["owned_job_final_active_processes"]==0,"unretired installation")
    saved=linked(report["native"]["bindings"])["bindings"][0]
    original_status=link(ATTEMPT/"commands/stage-after.stdout")
    status=linked(original_status)
    require(all(status[k]["availability"]=="ready" and status[k]["queued"]==status[k]["running"]==0
                for k in ("focus","context")),"previous native stage not idle")
    native_ticket=original["installation"]|{"status":original_status}
    transcript=Path(report["passes"][0]["transcript"]["path"])
    pin(transcript,report["passes"][0]["transcript"]["sha256"])
    calls=[json.loads(l)["argv"] for l in transcript.read_bytes().splitlines()
           if l.startswith(b'{"kind":"native_exchange","argv":["tools","call","search"')]
    args=json.loads(calls[0][4])
    require(set(args)=={"query","full_chunks","include_status","max_bytes","limit","response_mode"} and
            args["response_mode"]=="full" and args["include_status"] is True,"unexpected original Search controls")
    return {"out":str(out),"source":link(Path(__file__).resolve()),"start_ms":now(),"deadline_seconds":480,
            "parent_pid":os.getpid(),"custody_owner":original["custody_owner"],"previous_control":outer["control_after"],
            "input_audit":original["input_audit"],"ticket":native_ticket,"retired":report["native"]["closed"],
            "source_scope":saved["index_id"],"publication":saved["publication"]["token"],"query":args,
            "modes":MODES,"warm_mode":"hybrid","corpus_files":saved["status_result"]["structuredContent"]["files"],
            "source_immutable_count":len(previous["immutable"])}


def execute(out,plan):
    acquired=linked(link(out/"control-mutex-acquired.json"))
    require_parent_owns(acquired,os.getppid())
    require(plan["parent_pid"]==acquired["owner_pid"] and plan["modes"]==list(MODES),"different owner or mode order")
    audit=linked(plan["input_audit"])
    work,commands=out/"fixture",out/"commands"
    commands.mkdir()
    ticket=out/"restart-request.json"
    native.write_json(ticket,{k:{n:r[n] for n in ("path","sha256")} for k,r in plan["ticket"].items()})
    env={k:v for k,v in os.environ.items() if not k.upper().startswith(("AIRK_ARBITER_EVALUATION_","AIRK_SELECTED_","AIRK_NATIVE_","NODE_"))}
    env.update(audit["fixture_environment"])
    env.update(AIRK_ARBITER_EVALUATION_WORK=str(work),AIRK_ARBITER_EVALUATION_RESTART=str(ticket),
               AIRK_ARBITER_EVALUATION_RESTART_SHA256=link(ticket)["sha256"],AIRK_NATIVE_WARM_OBSERVER="true",
               AIRK_NATIVE_DISPATCH_OBSERVER="true",AIRK_MCP_RESPONSE_MODE="full")
    end=time.monotonic()+(plan["start_ms"]+480000-now())/1000
    image=pin(audit["inputs"]["service"]["path"],audit["inputs"]["service"]["sha256"])
    root=linked(plan["ticket"]["ready"])
    ctl=[audit["inputs"]["cli"]["path"],"--output","json","--service-runtime-dir",root["control_root"]]
    def cli(name,*args):
        p,_=native.run_logged(ctl+list(args),commands,name,env,end)
        return native.read_json(p)
    values=[]
    with (out/"service.log").open("xb") as log:
        proc=subprocess.Popen([image["path"],"--ignored","--exact",SELECTOR,"--test-threads","1"],
                              cwd=ROOT,env=env,stdin=subprocess.DEVNULL,stdout=log,stderr=log,creationflags=subprocess.CREATE_NO_WINDOW)
        try:
            native.wait_observed(out,lambda:True if (work/"ready.json").is_file() else None,proc,end,"standalone retrieval readiness")
            ready=native.read_json(work/"ready.json")
            previous_status=linked(plan["ticket"]["status"])
            require(ready["health"]["process_id"]==proc.pid and ready["health"]["executable_sha256"]==image["sha256"] and
                    all(ready[k]==root[k] for k in ("profiles","control_root","inputs_sha256")),"different native runtime")
            for k in ("focus","context"):
                wait_stage(work,proc,ready,k,previous_status[k]["active_profile"],0,min(end,time.monotonic()+120))
            before=cli("stage-before","arbiter","status-legacy")
            require(before["configuration"]==previous_status["configuration"],"saved CAL stages differ")
            for i,mode in enumerate((plan["warm_mode"],*plan["modes"])):
                args=plan["query"] | {"mode":mode}
                lo=now()
                body=native.tool_body(cli(f"search-{i}","tools","call","airk_search","--index",plan["source_scope"],
                                          "--args",json.dumps(args,separators=(",",":"))))
                hi=now()
                require(body["execution"]["served_mode"]==mode and body["execution"]["incomplete"] is False and
                        body["publication"]["token"]==plan["publication"],"native retrieval mode or publication differs")
                values.append({"warm":i==0,"mode":mode,"start_ms":lo,"end_ms":hi,
                               "stdout":link(commands/f"search-{i}.stdout")})
            final=cli("stage-after","arbiter","status-legacy")
            require(final["configuration"]==before["configuration"],"stages changed within probe")
        except Exception:
            if proc.poll() is None and (work/"ready.json").is_file() and end-time.monotonic()>15:
                cli("failure-stage-at-shutdown","arbiter","status-legacy")
            raise
        finally:
            def shutdown():
                if proc.poll() is None and (work/"ready.json").is_file():
                    cli("shutdown","service","shutdown")
            def wait():
                require(proc.wait(timeout=min(120,max(1,end-time.monotonic())))==0,"native service failed")
            settle_service(sys.exception(),shutdown,wait,lambda row:native.write_json(out/"service-failure.json",row))
    retired=native.read_json(work/"closed.json")
    require(retired["instance"]==ready["health"]["instance"] and retired["shutdown_completed"] and
            retired["service_lease_reacquired"],"native retirement absent")
    dispatch=read_journal(work,before["focus"]["active_profile"],setup_only=False)
    records=[json.loads(l) for l in (work/"composed-observer.jsonl").read_bytes().splitlines()]
    returns=[r for r in records if r["kind"]=="score_returned"]
    for row in values:
        matched=[r for r in returns if row["start_ms"]<=r["unix_ms"]<=row["end_ms"]]
        row["focus_dispatches"]=len(matched)
        row["focus_inference_seconds"]=sum(r["work"]["inference_us"] for r in matched)/1e6
        row["other_seconds"]=(row["end_ms"]-row["start_ms"])/1000-row["focus_inference_seconds"]
    require(sum(r["focus_dispatches"] for r in values)==dispatch["successful_focus_returns"] and
            dispatch["failed_focus_returns"]==dispatch["context_dispatches"]==0,"missing native returns")
    native.write_json(out/"result.json",{"modes":values,"dispatch":dispatch,"closed":link(work/"closed.json"),
                                        "measurement_scope":"whole process CLI, served retrieval mode and original Focus inference"})


def main():
    ap=argparse.ArgumentParser(description=__doc__)
    ap.add_argument("output",type=Path)
    ap.add_argument("--owned")
    args=ap.parse_args()
    out=args.output
    require(out.resolve(strict=True)==out and out.parent==CAMPAIGN and out.name.startswith("cal100-search-mode-timing-r") and
            sys.flags.no_site and sys.dont_write_bytecode,"verified diagnostic root required")
    if args.owned:
        with native.capped_worker_job(out):
            execute(out,linked(pin(out/"plan.json",args.owned)))
        return
    require(not any(out.iterdir()),"diagnostic already started")
    with InstallationMutex():
        acquired=out/"control-mutex-acquired.json"
        native.write_json(acquired,{"owner_pid":os.getpid(),"name":MUTEX_NAME,"at_ms":now()})
        try:
            plan=prepare(out)
            prior=linked(plan["previous_control"])
            custody=runpy.run_path(plan["custody_owner"]["path"])["custody"]
            before=custody(out,prior["immutable"],True)
            require(linked(before)["files"]==prior["files"],"physical state differs from timed retirement")
            plan["control_before"]=before
            native.write_json(out/"plan.json",plan)
            owned=owned_module().run_evaluation_fixture([sys.executable,"-I","-B","-S",Path(__file__).resolve(),out,
                        "--owned",link(out/"plan.json")["sha256"]],ROOT,out/"outer.log",480,os.environ)
            native.write_json(out/"process-owned.json",owned)
            owned["control_after"]=custody(out,prior["immutable"],False)
            native.write_json(out/"outer-owned.json",owned)
        finally:
            native.write_json(out/"control-mutex-release-intent.json",{"owner_pid":os.getpid(),"acquired":link(acquired),"at_ms":now()})
    native.write_json(out/"control-mutex-released.json",{"owner_pid":os.getpid(),"name":MUTEX_NAME,"at_ms":now(),"outer":link(out/"outer-owned.json")})
    require(owned_module().successful(owned) and (out/"result.json").is_file(),"incomplete native stage diagnostic")


if __name__=="__main__":
    main()
