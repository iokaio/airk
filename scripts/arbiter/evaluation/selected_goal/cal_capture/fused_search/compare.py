# SPDX-License-Identifier: Apache-2.0
"""Compare the rebuilt canonical service with the same original native Search inputs."""
import argparse
import json
import os
from pathlib import Path
import re
import shutil
import runpy
import subprocess
import sys
import time

sys.path.insert(0,str(Path(__file__).resolve().parent))
from stage import CAMPAIGN, HERE, OUTPUT, ROOT, SELECTOR, link, linked, native, pin, require
from run import InstallationMutex, MUTEX_NAME, now, owned_module, read_journal, require_parent_owns, verify_saved_binding, wait_stage
from controller import NODE_SHA
from group_service import settle_service

OLD=CAMPAIGN/"cal100-search-mode-timing-r1"
AB=CAMPAIGN/"cal100-focus1024-timing-ab-r2"
RETIRED=CAMPAIGN/"compact-development-TRAIN_Q598-focus1024-run-r2"/"owned.json"
QSHA="f45abb479f15087f860e0a0de0261f7ac7ff4044e88947fb0f2378a5ee4bc771"
BASE_SHA="d2bb6cba0e02213b8040409435ab2e05c229bf0f4a4e2092320be54b00873ab5"
OLD_SHA="0a7c570316df152092a8c8d58ed96155fa8420ea91d0f2baea455b76fd6cf3f4"
REUSE=ROOT/"scripts/arbiter/analysis/development_margins/reuse_installation.py"
OUT=CAMPAIGN/"cal100-fused-canonical-search-r1"


def records(path):
    return [json.loads(line) for line in Path(path).read_bytes().splitlines()]


def queries(report):
    all_cases={}
    old=linked(pin(OLD/"result.json",OLD_SHA))
    old_plan=native.read_json(OLD/"plan.json")
    for n in (1,6):
        row=old["modes"][n]
        all_cases[f"hybrid-{n}"]={"args":old_plan["query"]|{"mode":"hybrid"},"reference":row["stdout"],
            "elapsed_s":(row["end_ms"]-row["start_ms"])/1000,"other_s":row["other_seconds"]}
    for i,epoch in enumerate(report["observed"]["passes"]):
        events=records(report["passes"][i]["transcript"]["path"])
        for n in range(96,100):
            marker=next((r for r in epoch["calls"] if r["ordinal"]==n and r["arm"]=="full" and r["tool"]=="search"),None)
            if marker:
                j=next(j for j,e in enumerate(events) if e["kind"]=="timing_exchange" and e["ordinal"]==n and
                       e["exchange"]==marker["exchange"])
                event=events[j+1]
                require(event["kind"]=="native_exchange" and event["argv"][2]=="search","unaligned original Search")
                all_cases[f"q{n+1}"]={"args":json.loads(event["argv"][4]),"reference_pass":report["passes"][i]["transcript"],
                    "reference_index":j+1,"elapsed_s":marker["cli_elapsed_us"]/1e6,
                    "other_s":(marker["cli_elapsed_us"]-marker["native_inference_us"])/1e6}
    require(set(all_cases)=={"q97","q98","q99","q100","hybrid-1","hybrid-6"},"incomplete reference menu")
    return all_cases


def stable(body):
    keys=("collection","generation","execution","publication","guidance","hits","truncated","semantic_readiness","discovery_readiness")
    receipt=body["focus"]["receipt"]
    result={k:body[k] for k in keys}
    result["focus"]={k:v for k,v in receipt.items() if k not in ("scope","input_digest","work")}
    ids=receipt["original_order"]
    require(len(set(ids))==len(ids),"nonunique Focus candidate identities")
    ordinal={candidate:i for i,candidate in enumerate(ids)}
    for key in ("original_order","ranked_order"):
        result["focus"][key]=[ordinal[candidate] for candidate in receipt[key]]
    for key in ("presentations","scores"):
        result["focus"][key]=[entry|{"candidate":ordinal[entry["candidate"]]} for entry in receipt[key]]
    result["focus"]["work"]={k:v for k,v in receipt["work"].items() if not k.endswith("_us")}
    result["physical_scope_members"]=body["scope_observation"]["members"]
    result["source_counts"]=body["source_verification"]["counts"]
    return result


def reference_body(case):
    if "reference" in case:
        return native.tool_body(linked(case["reference"]))
    src=case["reference_pass"]
    pin(src["path"],src["sha256"])
    event=records(src["path"])[case["reference_index"]]
    return native.tool_body(json.loads(event["result"]["stdout"]))


def run_child(plan):
    require_parent_owns(linked(plan["mutex"]),os.getppid())
    audit=linked(plan["audit"])
    original=linked(plan["ticket"]["ready"])
    work,commands=OUT/"fixture",OUT/"commands"
    commands.mkdir()
    env={k:v for k,v in os.environ.items() if not k.upper().startswith(("AIRK_ARBITER_EVALUATION_","AIRK_SELECTED_","AIRK_NATIVE_","NODE_"))}
    env.update(audit["fixture_environment"])
    req=OUT/"restart-request.json"
    native.write_json(req,{k:{n:r[n] for n in ("path","sha256")} for k,r in plan["ticket"].items()})
    env.update(AIRK_ARBITER_EVALUATION_WORK=str(work),AIRK_ARBITER_EVALUATION_RESTART=str(req),
               AIRK_ARBITER_EVALUATION_RESTART_SHA256=link(req)["sha256"],AIRK_NATIVE_WARM_OBSERVER="true",
               AIRK_NATIVE_DISPATCH_OBSERVER="true",AIRK_MCP_RESPONSE_MODE="full",AIRK_PRIVATE_VECTOR_POINTER_TIMING="1",
               RUST_TEST_NOCAPTURE="1")
    legacy=linked(plan["legacy_audit"])
    env["PATH"]=str(Path(legacy["inputs"]["service"]["path"]).parent)+os.pathsep+env["PATH"]
    end=time.monotonic()+(plan["start_ms"]+600000-now())/1000
    service=audit["inputs"]["service"]["path"]
    ctl=[audit["inputs"]["cli"]["path"],"--output","json","--service-runtime-dir",original["control_root"]]
    def cli(label,*args):
        p,_=native.run_logged(ctl+list(args),commands,label,env,end)
        return native.read_json(p)
    observed=[]
    with (OUT/"service.log").open("xb") as log:
        proc=subprocess.Popen([service,"--ignored","--exact",SELECTOR,"--test-threads","1","--nocapture"],
                              cwd=ROOT,env=env,stdout=log,stderr=log,stdin=subprocess.DEVNULL,creationflags=subprocess.CREATE_NO_WINDOW)
        try:
            def published():
                try:
                    return native.read_json(work/"ready.json")
                except FileNotFoundError:
                    return None
                except PermissionError as error:
                    if error.winerror==32:return None
                    raise
            ready=native.wait_observed(OUT,published,proc,end,"fresh canonical service",file_changes=0x1b)
            require(ready["health"]["process_id"]==proc.pid and ready["health"]["executable_sha256"]==audit["inputs"]["service"]["sha256"]
                    and all(ready[k]==original[k] for k in ("inputs_sha256","profiles","control_root")),"wrong service or profile")
            historical=linked(plan["ticket"]["status"])
            for name in ("focus","context"):
                wait_stage(work,proc,ready,name,historical[name]["active_profile"],0,min(end,time.monotonic()+120))
            before=cli("stage-before","arbiter","status-legacy")
            require(before["configuration"]==historical["configuration"] and all(before[k]["availability"]=="ready" and
                    before[k]["active_profile"]==historical[k]["active_profile"] for k in ("focus","context")),"different enrollment")
            scopes=audit["inputs"]["scopes"]
            emit=env|{"AIRK_ARBITER_EVALUATION_SCOPES":scopes["path"],"AIRK_ARBITER_EVALUATION_SCOPES_SHA256":scopes["sha256"],
                     "AIRK_ARBITER_EVALUATION_PROFILE":before["context"]["active_profile"],"AIRK_ARBITER_EVALUATION_STAGE":"context",
                     "AIRK_ARBITER_EVALUATION_ENABLED":"true","AIRK_ARBITER_EVALUATION_CONTROL_PREFIX":"enable-context"}
            native.run_logged(native.ignored(service,"emit_private_arbiter_configuration"),commands,"bind-original-cal",emit,end)
            binding=native.read_json(work/"enable-context.bindings.json")
            verify_saved_binding(linked(plan["original_bindings"]),binding)
            row,=binding["bindings"]
            for label in ["warm","hybrid-1","q97","q98","q99","q100","hybrid-6"]:
                case=plan["cases"]["hybrid-1" if label=="warm" else label]
                lo=now()
                if label.startswith("q"):
                    helper=plan["window_helper"]
                    pin(helper["path"],helper["sha256"])
                    path,_=native.run_logged([plan["node"]["path"],helper["path"],str(OUT/"plan.json"),link(OUT/"plan.json")["sha256"],label],
                                             commands,label,env,end)
                    exchange,=[e for e in native.read_json(path) if e.get("tool")=="search"]
                    current=native.tool_body(json.loads(exchange["result"]["stdout"]))
                    lo,hi=exchange["lo"],exchange["hi"]
                else:
                    current=native.tool_body(cli(label,"tools","call","airk_search","--index",row["index_id"],"--args",
                                  json.dumps(case["args"],separators=(",",":"))))
                    hi=now()
                source=reference_body(case)
                a,b=stable(current),stable(source)
                differences=[k for k in a if a[k]!=b[k]]
                # Preserve observed output and every differing stable field before final judgment.
                observed.append({"case":label,"start_ms":lo,"end_ms":hi,"stdout":link(commands/(label+".stdout")),
                    "different_fields":differences,"old_other_s":case["other_s"],"old_elapsed_s":case["elapsed_s"]})
            final=cli("stage-after","arbiter","status-legacy")
            require(final["configuration"]==before["configuration"] and all(final[k]["availability"]=="ready" and
                    final[k]["running"]==final[k]["queued"]==0 for k in ("focus","context")),"native stages changed")
        except Exception as error:
            if proc.poll() is None and (work/"ready.json").is_file() and end-time.monotonic()>15:
                try: cli("failure-stage-at-shutdown","arbiter","status-legacy")
                except Exception as failure: error.add_note(str(failure))
            raise
        finally:
            def shutdown():
                if proc.poll() is None and (work/"ready.json").is_file():cli("shutdown","service","shutdown")
            def wait(): require(proc.wait(timeout=min(120,max(1,end-time.monotonic())))==0,"native service failed")
            settle_service(sys.exception(),shutdown,wait,lambda e:native.write_json(OUT/"service-failure.json",e))
    retired=native.read_json(work/"closed.json")
    require(retired["instance"]==ready["health"]["instance"] and retired["service_lease_reacquired"] and retired["shutdown_completed"],"native closure missing")
    dispatch=read_journal(work,before["focus"]["active_profile"],setup_only=False)
    marks=re.findall(rb"AIRK_VECTOR_POINTER_TIMING wanted=(\d+) ranked=(\d+) pointer_count=(\d+) elapsed_ns=(\d+)",(OUT/"service.log").read_bytes())
    returns=[r for r in records(work/"composed-observer.jsonl") if r["kind"]=="score_returned"]
    for i,row in enumerate(observed):
        hits=[r for r in returns if row["start_ms"]<=r["unix_ms"]<=row["end_ms"]]
        total=(row["end_ms"]-row["start_ms"])/1000
        inference=sum(r["work"]["inference_us"] for r in hits)/1e6
        row.update(focus_returns=len(hits),new_elapsed_s=total,new_inference_s=inference,new_other_s=total-inference,
                   elapsed_saving=row["old_elapsed_s"]-total,other_saving=row["old_other_s"]-total+inference)
        if len(marks)==len(observed):
            row["wanted"],row["ranked"],row["pointer_count"],row["pointer_ns"]=(int(v) for v in marks[i])
    native.write_json(OUT/"comparison.json",{"cases":observed,"dispatch":dispatch,"pointer_markers":len(marks),
                     "fixture":{k:link(work/k) for k in ("ready.json","enable-context.bindings.json","closed.json")},
                     "stage":link(commands/"stage-after.stdout")})
    require(sum(r["focus_returns"] for r in observed)==dispatch["successful_focus_returns"] and dispatch["failed_focus_returns"]==0
            and all(not r["different_fields"] for r in observed),"paired canonical outputs differ")
    require(len(marks)==len(observed) and all(r["wanted"]==400 and r["ranked"]>0 and r["pointer_count"]>0 for r in observed),
            "real hybrid candidate observation missing")


def main():
    ap=argparse.ArgumentParser(description=__doc__)
    ap.add_argument("--audit-sha",required=True)
    ap.add_argument("--owned")
    args=ap.parse_args()
    require(OUT.is_dir(),"checked output directory missing")
    if args.owned:
        with native.capped_worker_job(OUT):
            run_child(linked(pin(OUT/"plan.json",args.owned)))
        return
    require(not any(OUT.iterdir()),"attempt already exists")
    audit=pin(OUTPUT/"cal100-launch-audit.json",args.audit_sha)
    provenance=native.read_json(OUTPUT/"provenance.json")
    require(provenance["audit"]["sha256"]==args.audit_sha and linked(audit)["inputs"]["service"]==provenance["image"],"different candidate")
    report=linked(pin(AB.with_name(AB.name+"-postclosure-analysis-r1.json"),BASE_SHA))
    prior=linked(pin(RETIRED,QSHA))
    original=linked(report["original_plan"])
    reuse=runpy.run_path(str(pin(REUSE,"e3989a250b10c4ee945423105af278fcb61dfc219262c3af0fb20c5fa5b91a60")["path"]))
    verify_file=REUSE.with_name("q598_closed_restart.py")
    verify=runpy.run_path(str(pin(verify_file,"d7db2362db47d613c22fa4ca61922898d67bd2cce80c76542d692b5a347c4234")["path"]))["verify"]
    with InstallationMutex():
        acquired=OUT/"control-mutex-acquired.json"
        native.write_json(acquired,{"owner_pid":os.getpid(),"name":MUTEX_NAME,"at_ms":now()})
        previous=verify(reuse,RETIRED,QSHA)
        require(Path(previous["path"])==OLD/"outer-owned.json" and previous["sha256"]=="056fe3e1c3103a60c1b2e88303ef491e89f059fbfbba539ae6a377768e0156fe","unexpected latest idle predecessor")
        latest=linked(prior["shared_installation"]["control_after"])
        before=reuse["custody"](OUT,latest["immutable"],True)
        require(linked(before)["files"]==latest["files"],"intervening installation mutation")
        plan={"schema":"airk.private.cal-fused-search-comparison/1","audit":audit,"legacy_audit":provenance["historical"],
              "node":pin(Path(shutil.which("node")).resolve(),NODE_SHA),"source":link(Path(__file__).resolve()),"window_helper":link(HERE/"window_search.cjs"),"mutex":link(acquired),"cases":queries(report),
              "original_bindings":original["original_bindings"],"previous":pin(RETIRED,QSHA),"control_before":before,
              "ticket":original["installation"]|{"status":pin(OLD/"commands/stage-after.stdout","30dacd28b6d15f240791d29fdbf42734be68dc9bdb85674780bb2d4d3298b84d")},
              "start_ms":now()}
        native.write_json(OUT/"plan.json",plan)
        owned=owned_module().run_evaluation_fixture([sys.executable,"-I","-B","-S",Path(__file__).resolve(),
                   "--audit-sha",args.audit_sha,"--owned",link(OUT/"plan.json")["sha256"]],ROOT,OUT/"outer.log",600,os.environ)
        native.write_json(OUT/"process-owned.json",owned)
        owned["control_after"]=reuse["custody"](OUT,latest["immutable"],False)
        native.write_json(OUT/"outer-owned.json",owned)
    native.write_json(OUT/"control-mutex-released.json",{"name":MUTEX_NAME,"owner_pid":os.getpid(),"at_ms":now(),
                     "acquired":link(acquired),"outer":link(OUT/"outer-owned.json")})
    require(owned_module().successful(owned),"candidate owner refused")


if __name__=="__main__":
    main()
