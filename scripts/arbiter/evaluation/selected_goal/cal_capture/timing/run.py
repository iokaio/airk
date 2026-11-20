# SPDX-License-Identifier: Apache-2.0
"""Run two counterbalanced passes of the original four-question native tail on one enrolled service."""
import argparse
import json
import os
import runpy
from pathlib import Path
import subprocess
import sys
import time

sys.path.insert(0,str(Path(__file__).resolve().parent))
from affinity import CAMPAIGN, HERE, ORDER, ROOT, require, sample, sha
CAPTURE = HERE.parent
sys.path[:0] = [str(CAPTURE),str(CAPTURE/"phased"),str(CAPTURE/"grouped")]
from controller import AUDIT_PATH, AUDIT_SHA, NODE_SHA, link, owned_module, summary
from dispatch_proof import read_journal
from group_service import settle_service
from installation_mutex import InstallationMutex, MUTEX_NAME, require_parent_owns
from launch import check_cal_sources, native, wait_stage
from lifetime import linked
from prepare import SELECTOR, pin

R5 = CAMPAIGN/"cal100-focus1024-capture-r5"
PLAN_SHA = "55a89e5219cf6de0c3b59cbaa8a4a8a43dc0d097276368cfb5a28ee2fe6536bf"
OUTER_SHA = "23aedfee1e92ef826714f37b852301894f048f4ad3964e11eb75e65fd7b4e276"


def now():
    return time.time_ns()//1000000


def verify_saved_binding(old, new):
    a, = old["bindings"]
    b, = new["bindings"]
    require(all(a[k]==b[k] for k in ("definition","index_id","policy","publication","source_root","source_roots")),
            "CAL root or original indexed publication changed")
    x,y = (row["status_result"]["structuredContent"] for row in (a,b))
    require(all(x[k]==y[k] for k in ("files","indexed_vectors","publication","semantic_readiness")),
            "CAL corpus or pipeline differs")
    scope, current = x["scope_observation"],y["scope_observation"]
    require(scope["qualifier"]==current["qualifier"] and scope["members"]==current["members"] and
            a["window_scope_selection"]["project_ids"]==b["window_scope_selection"]["project_ids"],
            "different authorized CAL project or membership")


def original_ticket(physical,latest):
    ready=linked(physical["ready"])
    closed=linked(physical["closed"])
    work=Path(physical["ready"]["path"]).parent
    require((work/"control").samefile(ready["control_root"]) and
            (work/"closed.json").samefile(physical["closed"]["path"]) and
            ready["health"]["instance"]==closed["instance"] and closed["shutdown_completed"] and
            closed["service_lease_reacquired"],"native restart requires the original physical control anchor")
    return {"ready":physical["ready"],"closed":physical["closed"],"status":latest["status"]}


def plan_for(out,node,previous,excluded,acquired):
    base = linked(pin(R5/"lifetime-plan.json", PLAN_SHA))
    terminal = linked(pin(R5/"outer-owned.json", OUTER_SHA))
    predecessor = linked(previous)
    require(terminal["exit_code"]==1 and terminal["owned_job_final_active_processes"]==0 and
            predecessor["exit_code"]==0 and predecessor["owned_descendants_settled"] is True and
            predecessor["owned_job_final_active_processes"]==0 and not predecessor["owned_job_forced_termination"] and
            (predecessor.get("source_seals_unchanged") is True or
             (predecessor.get("scope")=="native non-Answer control recovery" and predecessor.get("answer_requests")==0)),
            "missing original native retirement")
    if predecessor.get("scope")=="native non-Answer control recovery":
        source=predecessor["source"]
        require(Path(source["path"])==ROOT/"scripts/arbiter/analysis/development_margins/recover_q598_control.py" and
                sha(source["path"])==source["sha256"],"different recovery owner")
        retired=Path(previous["path"]).with_name("control-mutex-released.json")
        release=native.read_json(retired)
        require(release["name"]==MUTEX_NAME and release["owned"]["sha256"]==previous["sha256"] and
                release["owned"]["path"]==previous["path"],"recovery mutex not retired")
    shared = predecessor["shared_installation"]
    anchor = {key:pin(shared[key]["path"],shared[key]["sha256"]) for key in ("ready","closed","status")}
    ready,closed,status = (linked(anchor[k]) for k in ("ready","closed","status"))
    original=linked(base["installation"]["ready"])
    require(ready["health"]["instance"]==closed["instance"] and closed["shutdown_completed"] and
            closed["service_lease_reacquired"] and all(ready[k]==original[k] for k in ("inputs_sha256","profiles","control_root")) and
            ready["health"]["executable_sha256"]==original["health"]["executable_sha256"] and
            all(status[k]["availability"]=="ready" for k in ("focus","context")),"unsettled latest installation")
    audit = linked(pin(AUDIT_PATH,AUDIT_SHA))
    for item in (*audit["inputs"].values(),*audit["compiled_sdk"].values(),audit["census"]):
        pin(item["path"],item["sha256"])
    sources=list(base["sources"])
    sources.extend(link(HERE/n) for n in ("run.py","affinity.py","worker.cjs","exchanges.cjs","readout.py","results.py"))
    sources.append(link(ROOT/"scripts/arbiter-fp16-execution.py"))
    custody_owner=pin(ROOT/"scripts/arbiter/analysis/development_margins/reuse_installation.py",
                      "e3989a250b10c4ee945423105af278fcb61dfc219262c3af0fb20c5fa5b91a60")
    sources.append(custody_owner)
    installed=linked(shared["control_after"])
    require(Path(installed["control_root"]).samefile(ready["control_root"]) and installed["immutable"] and
            all(installed["files"].get(k)==v for k,v in installed["immutable"].items()),
            "latest installed weight or catalog custody differs")
    for p in sources:
        pin(p["path"],p["sha256"])
    warm_log=R5/"lifetime-01/phase-12.jsonl"
    qs=next(json.loads(l) for l in warm_log.read_bytes().splitlines()
            if l.startswith(b'{"kind":"native_exchange","argv":["tools","call","search"'))
    warm_args=json.loads(qs["argv"][4])
    require(set(warm_args)=={"full_chunks","include_status","limit","max_bytes","query","response_mode"},
            "original warm Search contains unexpected parameters")
    return {"schema":"airk.private.cal-phase12-affinity-timing/1","output":str(out),"node":pin(node,NODE_SHA),
            "input_audit":base["input_audit"],"sources":sources,"affinity_helper":link(HERE/"affinity.py"),
            "question_ids":base["phase_ids"][12],"schedule":ORDER,"service":audit["inputs"]["service"],
            "original_bindings":base["installation"]["bindings"],"latest_retirement":anchor,
            "installation":original_ticket(base["installation"],anchor),"predecessor":previous,
            "custody_owner":custody_owner,"previous_control_after":shared["control_after"],
            "original_source_counts":check_cal_sources(),"mutex":link(acquired),"topology":sample(excluded),
            "warm_search":warm_args,"warm_count":2,"diagnostic_wall_ms":1800000,"requested_calls_ms":120000,
            "delivery_grace_ms":5000,"start_ms":now(),"calibration_fitted":False}


def execute(out,plan,reference):
    require_parent_owns(linked(plan["mutex"]),os.getppid())
    audit=linked(plan["input_audit"])
    root=linked(plan["installation"]["ready"])
    work,commands=out/"fixture",out/"commands"
    commands.mkdir()
    ticket=out/"restart-request.json"
    native.write_json(ticket,{k:{n:r[n] for n in ("path","sha256")} for k,r in plan["installation"].items()})
    env={k:v for k,v in os.environ.items() if not k.upper().startswith(
        ("AIRK_ARBITER_EVALUATION_","AIRK_SELECTED_","AIRK_NATIVE_","NODE_OPTIONS","NODE_PATH"))}
    env.update(audit["fixture_environment"])
    env.update(AIRK_ARBITER_EVALUATION_WORK=str(work),AIRK_ARBITER_EVALUATION_RESTART=str(ticket),
               AIRK_ARBITER_EVALUATION_RESTART_SHA256=sha(ticket),AIRK_NATIVE_WARM_OBSERVER="true",
               AIRK_NATIVE_DISPATCH_OBSERVER="true",AIRK_MCP_RESPONSE_MODE="full")
    end=plan["start_ms"]+plan["diagnostic_wall_ms"]
    deadline=time.monotonic()+(end-now())/1000
    service=Path(plan["service"]["path"])
    ctl=[audit["inputs"]["cli"]["path"],"--output","json","--service-runtime-dir",root["control_root"]]
    def cli(label,*args,limit=None):
        path,_=native.run_logged(ctl+list(args),commands,label,env,min(deadline,limit or deadline))
        return native.read_json(path)
    argv=[str(service),"--ignored","--exact",SELECTOR,"--test-threads","1"]
    clock=out/"fixture-start.json"
    with (out/"service.log").open("xb") as log:
        start=now()
        proc=subprocess.Popen(argv,cwd=ROOT,env=env,stdin=subprocess.DEVNULL,stdout=log,stderr=log,
                              creationflags=subprocess.CREATE_NO_WINDOW)
        try:
            native.write_json(clock,{"schema":"airk.private.cal100-fixture-start/1","pid":proc.pid,"start_ms":start,
                                     "deadline_ms":start+10800000,"fixture_root":str(work),"input_audit_sha256":AUDIT_SHA,
                                     "command":argv})
            native.wait_observed(out,lambda: True if (work/"ready.json").is_file() else None,proc,deadline,"timing service readiness")
            ready=native.read_json(work/"ready.json")
            instance=ready["health"]["instance"]
            require(instance not in (root["health"]["instance"],linked(plan["latest_retirement"]["ready"])["health"]["instance"]) and
                    ready["health"]["process_id"]==proc.pid and
                    ready["health"]["executable_sha256"]==plan["service"]["sha256"] and
                    all(ready[k]==root[k] for k in ("inputs_sha256","control_root","profiles")),
                    "different process or installation")
            predecessor_status=linked(plan["installation"]["status"])
            for name in ("focus","context"):
                wait_stage(work,proc,ready,name,predecessor_status[name]["active_profile"],0,
                           min(deadline,time.monotonic()+120))
            before=cli("before-rebind","arbiter","status-legacy")
            require(before["configuration"]==linked(plan["installation"]["status"])["configuration"],
                    "native saved state differs from latest TRAIN closure")
            profiles={"focus":audit["profiles"]["arbiter-focus-qwen3-reranker-06b"],
                      "context":audit["profiles"]["arbiter-context-qwen3-17b"]}
            for name,manifest in profiles.items():
                label="enable-"+name
                stageenv=env|{"AIRK_ARBITER_EVALUATION_SCOPES":audit["inputs"]["scopes"]["path"],
                              "AIRK_ARBITER_EVALUATION_SCOPES_SHA256":audit["inputs"]["scopes"]["sha256"],
                              "AIRK_ARBITER_EVALUATION_PROFILE":manifest,"AIRK_ARBITER_EVALUATION_STAGE":name,
                              "AIRK_ARBITER_EVALUATION_ENABLED":"true","AIRK_ARBITER_EVALUATION_CONTROL_PREFIX":label}
                native.run_logged(native.ignored(service,"emit_private_arbiter_configuration"),commands,"emit-"+name,
                                  stageenv,deadline)
                cfg=cli("configure-"+name,"arbiter","configure","--legacy","--file",work/(label+".configure.json"))
                warm_deadline=min(deadline,time.monotonic()+120)
                revision=cfg["configuration"]["revision"]
                wait_stage(work,proc,ready,name,manifest,revision,warm_deadline)
                reads=0
                def settled_stages():
                    nonlocal reads
                    reads+=1
                    current=cli(f"settlement-{name}-{reads}","arbiter","status-legacy",limit=warm_deadline)
                    require(current["configuration"]["revision"]==revision,"unexpected intervening configuration")
                    return True if all(current[k]["availability"]=="ready" and current[k]["queued"]==0 and
                                       current[k]["running"]==0 for k in profiles) else None
                native.wait_observed(work,settled_stages,proc,warm_deadline,"original stages idle before next operation",
                                     file_changes=0x18)
            stage=cli("stage-before","arbiter","status-legacy")
            require(all(stage[k]["active_profile"]==v and stage[k]["availability"]=="ready" for k,v in profiles.items()),
                    "original CPU4 pipeline unavailable")
            bindings=native.read_json(work/"enable-context.bindings.json")
            verify_saved_binding(linked(plan["original_bindings"]),bindings)
            row,=bindings["bindings"]
            for i in range(plan["warm_count"]):
                body=native.tool_body(cli(f"warm-search-{i}","tools","call","airk_search","--index",row["index_id"],"--args",
                         json.dumps(plan["warm_search"],separators=(",",":")),limit=time.monotonic()+125))
                require(body["publication"]["token"]==row["publication"]["token"] and body["semantic_readiness"]["pipeline_digest"]==audit["pipeline"]
                        and body["execution"]["incomplete"] is False,"original warm source/pipeline differs")
            base={"schema":"airk.private.cal100-live-fixture/1","fixture_root":str(work),"ready":link(work/"ready.json"),
                  "bindings":link(work/"enable-context.bindings.json"),"answer":link(work/"context-request.json"),
                  "start_receipt":link(clock),"timing_plan":reference,"timing_service_pid":proc.pid}
            fingerprints=set()
            passes=[]
            for i in range(2):
                req=out/f"pass-{i}-request.json"
                native.write_json(req,base|{"timing_pass":i})
                transcript=out/f"pass-{i}.jsonl"
                limit=min(1800,deadline-time.monotonic())
                require(limit>125,"incomplete original timing window")
                cmd=[plan["node"]["path"],HERE/"worker.cjs",req,sha(req),12,limit]
                launch=env|{"AIRK_CAL100_LIVE_REQUEST":str(req),"AIRK_CAL100_PYTHON":sys.executable}
                result=owned_module().run_evaluation_fixture(cmd,ROOT,transcript,limit,launch)
                receipt=out/f"pass-{i}.owned.json"
                native.write_json(receipt,result)
                require(owned_module().successful(result) and result["owned_job_final_active_processes"]==0,
                        "timing pass failed under original clock or custody")
                summary(transcript,12,plan["question_ids"],instance,row["publication"],fingerprints)
                passes.append({"transcript":link(transcript),"owned":link(receipt)})
            final=cli("stage-after","arbiter","status-legacy")
            require(final["configuration"]==stage["configuration"],"stage configuration changed across timing arms")
        except Exception as error:
            if proc.poll() is None and (work/"ready.json").is_file() and deadline-time.monotonic()>=20:
                try:
                    cli("failure-stage-at-shutdown","arbiter","status-legacy",limit=time.monotonic()+5)
                except Exception as diagnostic:
                    error.add_note(f"failure-stage observation failed: {diagnostic}")
            raise
        finally:
            def shutdown():
                if proc.poll() is None and (work/"ready.json").is_file():
                    cli("shutdown","service","shutdown")
            def wait():
                require(proc.wait(timeout=max(0,min(120,deadline-time.monotonic())))==0,"native shutdown refused")
            settle_service(sys.exception(),shutdown,wait,lambda e:native.write_json(out/"service-failure.json",e))
    closure=native.read_json(work/"closed.json")
    require(closure["instance"]==instance and closure["shutdown_completed"] and closure["service_lease_reacquired"],
            "native installation did not close")
    dispatch=read_journal(work,profiles["focus"],setup_only=False)
    require(len(fingerprints)==8 and dispatch["context_dispatches"]==0 and
            check_cal_sources()==plan["original_source_counts"],"incomplete original timing population")
    for src in plan["sources"]:
        pin(src["path"],src["sha256"])
    from results import paired
    report=paired(plan,out,passes,dispatch)
    native.write_json(out/"measured.json",report)
    native.write_json(out/"timing-closure.json",{"schema":"airk.private.cal-phase12-timing-closure/1",
                      "plan":reference,"passes":passes,"closed":link(work/"closed.json"),"dispatch":dispatch,
                      "instance":instance,"elapsed_ms":now()-plan["start_ms"],"calibration_fitted":False,
                      "measured":link(out/"measured.json")})


def main():
    parser=argparse.ArgumentParser(description=__doc__)
    parser.add_argument("--output",type=Path,required=True)
    parser.add_argument("--node",type=Path,required=True)
    parser.add_argument("--predecessor",type=Path,required=True)
    parser.add_argument("--predecessor-sha256",required=True)
    parser.add_argument("--excluded-mask",type=lambda x:int(x,0),default=0)
    parser.add_argument("--owned-plan-sha256")
    args=parser.parse_args()
    out=args.output
    require(os.name=="nt" and sys.flags.no_site and sys.dont_write_bytecode and out.resolve(strict=True)==out and
            out.parent==CAMPAIGN and out.name.startswith("cal100-focus1024-timing-ab-r"),"fresh private native timing root required")
    path=out/"plan.json"
    if args.owned_plan_sha256:
        ref=pin(path,args.owned_plan_sha256)
        with native.capped_worker_job(out):
            execute(out,linked(ref),ref)
        return
    require(not any(out.iterdir()),"timing attempt already started")
    previous=pin(args.predecessor,args.predecessor_sha256)
    require(args.predecessor.name=="owned.json" and args.predecessor.parent.parent==CAMPAIGN and
            ("TRAIN_Q598" in args.predecessor.parent.name or args.predecessor.parent.name.startswith("train-q598-closed-control-recovery-r")),
            "latest completed Q598 predecessor required")
    acquired=out/"control-mutex-acquired.json"
    terminal=out/"outer-owned.json"
    try:
        with InstallationMutex():
            native.write_json(acquired,{"name":MUTEX_NAME,"owner_pid":os.getpid(),"at_ms":now()})
            plan=plan_for(out,args.node,previous,args.excluded_mask,acquired)
            keeper=runpy.run_path(plan["custody_owner"]["path"])["custody"]
            previous_state=linked(plan["previous_control_after"])
            immutable=previous_state["immutable"]
            before=keeper(out,immutable,True)
            require(linked(before)["files"]==previous_state["files"],"physical control changed after predecessor retirement")
            plan["control_before"]=before
            native.write_json(path,plan)
            cmd=[sys.executable,"-I","-B","-S",Path(__file__).resolve(),"--output",out,"--node",args.node,
                 "--predecessor",args.predecessor,"--predecessor-sha256",args.predecessor_sha256,"--owned-plan-sha256",sha(path)]
            result=owned_module().run_evaluation_fixture(cmd,ROOT,out/"outer.log",plan["diagnostic_wall_ms"]/1000,os.environ)
            native.write_json(out/"process-owned.json",result)
            result["control_after"]=keeper(out,immutable,False)
            result["timing_plan"]=link(path)
            native.write_json(terminal,result)
    finally:
        if acquired.exists():
            native.write_json(out/"control-mutex-released.json",{"name":MUTEX_NAME,"owner_pid":os.getpid(),"at_ms":now(),
                              "acquired":link(acquired),"outer":link(terminal) if terminal.is_file() else None})
    require(owned_module().successful(result) and result["owned_job_final_active_processes"]==0 and
            (out/"timing-closure.json").is_file(),"original timing study incomplete")


if __name__=="__main__":
    main()
