# SPDX-License-Identifier: Apache-2.0
"""Reopen the actual seven-Search retirement before a candidate-service CAL capture."""
import hashlib
import json
from pathlib import Path
import runpy

HERE=Path(__file__).resolve().parent
REPO=HERE.parents[5]
E=REPO/"artifacts/v190/final-plan/exec-20261116T052942Z"
RUN=E/"cal100-fused-canonical-search-r1"
Q598=E/"compact-development-TRAIN_Q598-focus1024-run-r2"
OLD=E/"cal100-search-mode-timing-r1"
AB=E/"cal100-focus1024-timing-ab-r2"
QSHA="f45abb479f15087f860e0a0de0261f7ac7ff4044e88947fb0f2378a5ee4bc771"
IDLE="30dacd28b6d15f240791d29fdbf42734be68dc9bdb85674780bb2d4d3298b84d"
LABELS=["warm","hybrid-1","q97","q98","q99","q100","hybrid-6"]
DISPATCH=HERE.parent/"dispatch_proof.py"
PRODUCER=HERE.parent/"fused_search/compare.py"


def require(ok, message):
    if not ok: raise ValueError(message)


def seal(path,digest=None):
    require(path.resolve(strict=True)==path and path.is_file(),"canonical file required")
    with path.open("rb") as stream:
        observed=hashlib.file_digest(stream,"sha256").hexdigest()
    require(digest is None or observed==digest,"changed "+path.name)
    return {"path":str(path),"bytes":path.stat().st_size,"sha256":observed}


def read(pin,path):
    require(pin==seal(path,pin["sha256"]),"foreign receipt")
    return json.loads(path.read_bytes())


def evidence(outer_sha,release_sha,comparison_sha):
    outer=seal(RUN/"outer-owned.json",outer_sha)
    release=seal(RUN/"control-mutex-released.json",release_sha)
    comparison=seal(RUN/"comparison.json",comparison_sha)
    pins={k:seal(RUN/name) for k,name in {"idle":"commands/stage-after.stdout","ready":"fixture/ready.json",
          "closed":"fixture/closed.json","plan":"plan.json","control_before":"control-before.json",
          "control_after":"control-after.json"}.items()}
    return dict(pins,custody=seal(RUN.with_name(RUN.name+"-custody.json")),retired=outer,mutex=release,comparison=comparison,previous=seal(Q598/"owned.json",QSHA),
                sources=[seal(PRODUCER),seal(DISPATCH,"75b370b0446e0be1946896cb8978e8028aed0fb939320d4c72426f40386ea027")])


def body(envelope):
    require(envelope.get("isError") is not True, "native Search returned error")
    if "structuredContent" in envelope:return envelope["structuredContent"]
    require(len(envelope["content"])==1 and envelope["content"][0]["type"]=="text","missing native response")
    return json.loads(envelope["content"][0]["text"])


def equivalent(a,b):
    keys=("collection","generation","execution","publication","guidance","hits","truncated","semantic_readiness","discovery_readiness")
    focus=lambda r:{k:v for k,v in r["focus"]["receipt"].items() if k not in ("scope","input_digest","work")}
    return (all(a[k]==b[k] for k in keys) and focus(a)==focus(b)
            and a["scope_observation"]["members"]==b["scope_observation"]["members"]
            and a["source_verification"]["counts"]==b["source_verification"]["counts"])


def search_rows(plan,comparison,cli,index_id):
    old=read(seal(OLD/"result.json","0a7c570316df152092a8c8d58ed96155fa8420ea91d0f2baea455b76fd6cf3f4"),OLD/"result.json")
    report_path=AB.with_name(AB.name+"-postclosure-analysis-r1.json")
    report=read(seal(report_path,"d2bb6cba0e02213b8040409435ab2e05c229bf0f4a4e2092320be54b00873ab5"),report_path)
    require([r["case"] for r in comparison["cases"]]==LABELS,"seven original Search calls required")
    for row in comparison["cases"]:
        label=row["case"]
        key="hybrid-1" if label=="warm" else label
        case=plan["cases"][key]
        if key.startswith("hybrid"):
            pin=old["modes"][int(key[-1])]["stdout"]
            require(case["reference"]==pin,"wrong historical hybrid Search")
            original_path=OLD/"commands"/("search-"+key[-1]+".stdout")
            original=body(read(pin,original_path))
            old_command=json.loads(original_path.with_suffix(".command.json").read_bytes())
            require(old_command["exit_code"]==0 and json.loads(old_command["argv"][-1])==case["args"],"original hybrid query differs")
        else:
            ordinal=int(key[1:])-1
            choices=[(i,m) for i,p in enumerate(report["observed"]["passes"]) for m in p["calls"]
                     if m["ordinal"]==ordinal and m["arm"]=="full" and m["tool"]=="search"]
            i,m=choices[0]
            pin=report["passes"][i]["transcript"]
            path=AB/f"pass-{i}.jsonl"
            require(case["reference_pass"]==pin and pin==seal(path,pin["sha256"]),"foreign native comparison transcript")
            events=[json.loads(line) for line in path.read_bytes().splitlines()]
            j=next(j for j,e in enumerate(events) if e["kind"]=="timing_exchange" and
                   (e["ordinal"],e["exchange"])==(ordinal,m["exchange"]))+1
            raw=events[j]
            require(case["reference_index"]==j and raw["kind"]=="native_exchange" and raw["argv"][2]=="search"
                    and json.loads(raw["argv"][4])==case["args"],"wrong original search argument or ordinal")
            original=body(json.loads(raw["result"]["stdout"]))
        current=body(read(row["stdout"],RUN/"commands"/(label+".stdout")))
        command=json.loads((RUN/"commands"/(label+".command.json")).read_bytes())
        args=command["argv"]
        require(command["exit_code"]==0 and len(args)==12 and args[0]==cli and args[9]==index_id and args[1:4]==["--output","json","--service-runtime-dir"] and
                Path(args[4]).samefile(E/"cal100-focus1024-capture-r3/fixture/control") and args[5:9]==["tools","call","airk_search","--index"] and
                args[-2]=="--args" and json.loads(args[-1])==case["args"],"native search command changed")
        require(not row["different_fields"] and equivalent(current,original),"original Search stable outputs differ")


def verify(record, installation, audit_pin, *, live):
    require(record==evidence(record["retired"]["sha256"],record["mutex"]["sha256"],record["comparison"]["sha256"]),
            "different candidate Search evidence")
    get=lambda key,rel:read(record[key],RUN/rel)
    owner=get("retired","outer-owned.json")
    custody=read(record["custody"],RUN.with_name(RUN.name+"-custody.json"))
    require(custody["schema"]=="airk.private.cal-timing-custody/1" and custody["exit_code"]==0 and
            custody["sid"]=="S-1-5-21-3678370870-3858642417-826968105-1002" and not custody["source_drift"] and
            custody["output_entries"]>0 and custody["physical_entries_after"]>0,"incomplete candidate output custody")
    release=get("mutex","control-mutex-released.json")
    plan=get("plan","plan.json")
    comparison=get("comparison","comparison.json")
    before=get("control_before","control-before.json")
    after=get("control_after","control-after.json")
    ready=get("ready","fixture/ready.json")
    closed=get("closed","fixture/closed.json")
    status=get("idle","commands/stage-after.stdout")
    prev=read(record["previous"],Q598/"owned.json")
    old_after=prev["shared_installation"]["control_after"]
    short=lambda p:{k:p[k] for k in ("path","sha256")}
    require(plan["source"]==seal(PRODUCER) and plan["previous"]==record["previous"] and plan["control_before"]==short(record["control_before"])
            and old_after["sha256"]==record["control_before"]["sha256"] and owner["control_after"]==short(record["control_after"])
            and owner["exit_code"]==0 and not owner["timed_out"] and not owner["owned_job_forced_termination"]
            and owner["owned_descendants_settled"] and not owner["owned_job_final_member_pids"]
            and owner["owned_job_final_active_processes"]==0 and record["plan"]["sha256"] in owner["argv"],
            "candidate owner or actual stopped predecessor differs")
    acquired=read(plan["mutex"],RUN/"control-mutex-acquired.json")
    require(release["outer"]==record["retired"] and release["acquired"]==plan["mutex"] and
            release["name"]==acquired["name"]==r"Local\AIRK-20261116-Focus1024-Reuse" and
            release["owner_pid"]==acquired["owner_pid"] and release["at_ms"]>=acquired["at_ms"],"unclosed Search mutex")
    require(plan["audit"]==audit_pin and installation["status"]==record["idle"] and
            all(plan["ticket"][k]==installation[k] for k in ("ready","closed")),"different installed epoch")
    audit=read(audit_pin,E/"cal100-fused-candidate-launch-r1/cal100-launch-audit.json")
    old=read(plan["ticket"]["status"],OLD/"commands/stage-after.stdout")
    anchor=read(installation["ready"],E/"cal100-focus1024-capture-r3/fixture/ready.json")
    require(plan["ticket"]["status"]["sha256"]==IDLE and ready["health"]["executable_sha256"]==audit["inputs"]["service"]["sha256"] and
            ready["health"]["instance"]==closed["instance"]!=anchor["health"]["instance"] and
            all(ready[k]==anchor[k] for k in ("control_root","inputs_sha256","profiles")) and
            closed["service_lease_reacquired"] and closed["shutdown_completed"] and comparison["stage"]==record["idle"]
            and comparison["fixture"]["ready.json"]==record["ready"] and comparison["fixture"]["closed.json"]==record["closed"],
            "wrong native service, closure or stage")
    require(status["configuration"]==old["configuration"] and all(status[k]["running"]==status[k]["queued"]==0 and
            status[k]["availability"]=="ready" and status[k]["active_profile"]==old[k]["active_profile"] for k in ("focus","context")),
            "actual candidate stage after-state differs")
    original_binding=read(plan["original_bindings"],E/"cal100-focus1024-capture-r3/fixture/enable-context.bindings.json")
    actual_binding=read(comparison["fixture"]["enable-context.bindings.json"],RUN/"fixture/enable-context.bindings.json")
    a,=original_binding["bindings"]
    b,=actual_binding["bindings"]
    old_status,new_status=(row["status_result"]["structuredContent"] for row in (a,b))
    require(actual_binding["instance"]==closed["instance"] and all(a[k]==b[k] for k in
            ("definition","index_id","policy","publication","source_root","source_roots")) and
            all(old_status[k]==new_status[k] for k in ("files","indexed_vectors","publication","semantic_readiness")) and
            old_status["scope_observation"]["members"]==new_status["scope_observation"]["members"] and
            a["window_scope_selection"]["project_ids"]==b["window_scope_selection"]["project_ids"],"candidate CAL binding or publication differs")
    dispatch=runpy.run_path(str(DISPATCH))["read_journal"](RUN/"fixture",old["focus"]["active_profile"],setup_only=False)
    require(dispatch==comparison["dispatch"] and dispatch["context_dispatches"]==dispatch["failed_focus_returns"]==0 and
            dispatch["successful_focus_returns"]==sum(row["focus_returns"] for row in comparison["cases"]),"incomplete actual Search dispatch")
    returns=[json.loads(line) for line in (RUN/"fixture/composed-observer.jsonl").read_bytes().splitlines()]
    last=0
    for row in comparison["cases"]:
        lo,hi=row["start_ms"],row["end_ms"]
        require(last<=lo<=hi and row["focus_returns"]==sum(e["kind"]=="score_returned" and lo<=e["unix_ms"]<=hi for e in returns),
                "native returns not aligned to real Search interval")
        last=hi
    search_rows(plan,comparison,audit["inputs"]["cli"]["path"],b["index_id"])
    control=E/"cal100-focus1024-capture-r3/fixture/control"
    require(before["control_root"]==after["control_root"]==str(control) and before["immutable"]==after["immutable"] and
            before["files"]["arbiter-v3/configuration.json"]==after["files"]["arbiter-v3/configuration.json"],"Search changed CAL configuration")
    if live:
        actual={p.relative_to(control).as_posix():seal(p)["sha256"] for p in control.rglob("*") if p.is_file()}
        require(actual==after["files"] and sorted(p.relative_to(control).as_posix() for p in control.rglob("*") if p.is_dir())==sorted(after["directories"]),
                "physical installation changed after Search retirement")
    return record["idle"]
