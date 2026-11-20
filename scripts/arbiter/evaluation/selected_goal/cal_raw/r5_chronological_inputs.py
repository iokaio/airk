# SPDX-License-Identifier: Apache-2.0
"""Prepare input-only native compiler batches from the original settled R5 lifetime(s)."""
import argparse
from collections import Counter
import json
import os
from pathlib import Path
import runpy
import subprocess
import sys

HERE = Path(__file__).resolve().parent
sys.path[:0] = [str(HERE), str(HERE.parent/"cal_certificates")]
from phase_reference import AUDIT, CAMPAIGN, MODELS, ORIGINAL
from reference_inputs import checked, digest, load_inputs, parsed, read, reference, require
from grouped_closure import TWO_PLAN, check_plan
from inspect_closed_lifetime import inspect
from phased_closure import phased_events

ROOT = CAMPAIGN/"cal100-focus1024-capture-r5"
PLAN = "55a89e5219cf6de0c3b59cbaa8a4a8a43dc0d097276368cfb5a28ee2fe6536bf"
ACQUIRED = "f0e60bafcd2e431f8daaa00b75b05b856506fb3afe08752fba48e9af8e324981"
FOCUS = "94c65d0c598b0b110cd25ddeb1c167ae730430197c0d5e6a59d79ee226c17c64"


def busy_mutex(plan):
    import ctypes
    from ctypes import wintypes
    pin = plan["installation_mutex"]
    record = read(pin,ROOT/"control-mutex-acquired.json")
    require(pin["sha256"] == ACQUIRED and record["name"] == r"Local\AIRK-20261116-Focus1024-Reuse"
            and record["owner_pid"] == 30592 and not (ROOT/"control-mutex-released.json").exists(),
            "original installation mutex owner differs or released")
    module = HERE.parent/"cal_capture/grouped/installation_mutex.py"
    [source] = [s for s in plan["sources"] if s["path"] == str(module)]
    require(digest(module.read_bytes()) == source["sha256"], "mutex owner source changed")
    api = ctypes.WinDLL("kernel32", use_last_error=True)
    api.OpenProcess.argtypes = [wintypes.DWORD,wintypes.BOOL,wintypes.DWORD]
    api.OpenProcess.restype = wintypes.HANDLE
    api.WaitForSingleObject.argtypes = [wintypes.HANDLE,wintypes.DWORD]
    times = [wintypes.FILETIME() for _ in range(4)]
    api.GetProcessTimes.argtypes = [wintypes.HANDLE]+[ctypes.POINTER(wintypes.FILETIME)]*4
    api.CloseHandle.argtypes = [wintypes.HANDLE]
    handle = api.OpenProcess(0x100000|0x1000,False,record["owner_pid"])
    require(bool(handle), "original mutex supervisor unavailable")
    try:
        require(api.WaitForSingleObject(handle,0) == 258, "original mutex supervisor exited")
        require(api.GetProcessTimes(handle,*(ctypes.byref(t) for t in times)), "unknown mutex supervisor birth")
        birth_ms = ((times[0].dwHighDateTime << 32) | times[0].dwLowDateTime)//10000 - 11644473600000
        require(0 < birth_ms <= record["at_ms"] <= plan["start_ms"], "reused supervisor process ID")
        try:
            with runpy.run_path(str(module))["InstallationMutex"]():
                raise ValueError("original installation mutex was available")
        except BlockingIOError:
            pass
    finally:
        api.CloseHandle(handle)
    return pin


def model_row(case, event, model):
    name, manifest = MODELS[model]
    require(case["question_id"] == event["id"], "changed question ID")
    records = [read(pin) for pin in case["captures"]]
    require(records == event["captured"] and len(records) <= 1, "wrong original native records")
    row = {"ordinal":case["ordinal"],"id":case["question_id"],"captures":case["captures"]}
    if event["answer"]["refused"] or not records:
        row["status"] = "no_native_assessment_request"
        return row
    record, = records
    request = record["request"]
    answer = event["answer"]["body"]
    require(record["Context_NN_dispatch"] is False and record["Focus_configuration"] == "ON"
            and record["actual_Context_profile"]["manifest"] == ORIGINAL
            and request["question"] == answer["question"] and request["scope"] == answer["assessment"]["scope"]
            and request["records"] == answer["records"][:len(request["records"])], "foreign original request")
    matches = [c for c in record["compiler_census"] if c["profile"] == manifest and c["model"] == name]
    require(len(matches)==1 and matches[0]["NN_invocations"] == 0, "original compiler census missing or scored")
    census = matches[0]
    remaining = min(request["remaining_ms"],record["remaining_ns_at_observation"]//1_000_000)
    row.update(roster=request["roster"],remaining_ms=remaining,remaining_input_tokens=request["remaining_input_tokens"])
    if census["fits_remaining_allowance"] is False or remaining <= 0:
        row["status"] = "compiler_refused_remaining_allowance"
    else:
        compiled = census["compiled"]
        require(census["fits_remaining_allowance"] is True and
                0 < len(compiled["input_ids"]) <= request["remaining_input_tokens"] and
                1 <= len(compiled["candidate_positions"]) == len(request["roster"]["cards"]) <= 8,
                "original compiler allowance differs")
        row.update(status="eligible",compiled=compiled)
    return row


def collect(plan, lifetimes, terminal):
    require(1 <= len(lifetimes) <= 2 and (len(lifetimes) == 2) == bool(terminal),
            "require one closed lifetime or both with full terminal proof")
    require(plan["schema"] == TWO_PLAN and plan["input_audit"]["sha256"] == AUDIT, "unexpected plan or launch")
    check_plan(plan)
    if terminal:
        outer, whole, release = terminal
        audit, ids, tasks, _, _, _, sources, index_root = load_inputs(plan["input_audit"],FOCUS)
        checked_ids = [row["id"] for row,_ in phased_events(ROOT,outer,whole,audit,AUDIT,ids,sources,index_root,
                                                            tasks,release_sha=release)]
        closure = read(reference(ROOT/"fixture-closure.json",whole))
        require(checked_ids==ids and lifetimes == [r["sha256"] for r in closure["lifetimes"]],
                "full native chronology or terminal lifetime pins differ")
    else:
        busy_mutex(plan)
    reports = []
    original = read(plan["installation"]["ready"])
    for i, lifetime_sha in enumerate(lifetimes):
        directory = ROOT/f"lifetime-{i:02}"
        life = read(reference(directory/"lifetime-closure.json",lifetime_sha))
        live = read(life["live_request"],directory/"live-request.json")
        ready = read(live["ready"],directory/"fixture/ready.json")
        closed = read(life["closed"],directory/"fixture/closed.json")
        require(life["instance"] == ready["health"]["instance"] == closed["instance"]
                and life["instance"] != original["health"]["instance"]
                and closed["shutdown_completed"] is True and closed["service_lease_reacquired"] is True
                and ready["health"]["executable_sha256"] == original["health"]["executable_sha256"],
                "native group failed to close on the installed service")
        require(read(life["restart"],directory/"restart-request.json") ==
                {k:{n:plan["installation"][k][n] for n in ("path","sha256")} for k in ("ready","closed","status")},
                "different installation restart")
        reports.extend(inspect(ROOT,(0,7)[i],PLAN,lifetime_sha,all_phases=True))
    return reports


def projections(plan, reports, phases):
    require(len(phases) == len(reports), "phase pin population differs")
    expected_phases = list(range(len(phases)))
    require(len(phases) in (7,13) and [r["phase"] for r in reports] == expected_phases,
            "incomplete or reordered lifetime phases")
    by_model = {model:[] for model in MODELS}
    for report in reports:
        n = report["phase"]
        require(report["plan_sha256"] == PLAN and report["input_audit_sha256"] == AUDIT
                and report["certificate_count"] == 0 and [r["question_id"] for r in report["rows"]] == plan["phase_ids"][n],
                "wrong original R5 phase evidence")
        data = (parsed(line) for line in checked(report["source_transcript"]).splitlines())
        evidence = (ev["evidence"] for ev in data if ev["kind"] == "cal_question")
        for case,ev in zip(report["rows"],evidence,strict=True):
            for model,rows in by_model.items():
                require(case["ordinal"] == len(rows), "wrong original ordinal")
                rows.append(model_row(case,ev,model))
    result = {}
    for model,rows in by_model.items():
        require(len(rows) == min(8*len(phases),100) and len({r["id"] for r in rows}) == len(rows),
                "incomplete population")
        result[model] = {"model":MODELS[model][0],"original_manifest":MODELS[model][1],
                         "launch_audit_sha256":AUDIT,"phase_plan_sha256":PLAN,"phases":phases,
                         "original_requests":len(rows),"statuses":dict(Counter(r["status"] for r in rows)),
                         "first_eligible_ordinal":next((r["ordinal"] for r in rows if r["status"]=="eligible"),None),
                         "NN_invocations":0,"rows":rows}
    return result


def check_outputs(out, names, *, created):
    shell = Path(os.environ["ProgramFiles"])/"PowerShell/7/pwsh.exe"
    guard = HERE.parents[3]/"dev/assert-human-execution.ps1"
    root = HERE.parents[4]
    env = dict(os.environ,AIRK_R5_OUTPUT=str(out),AIRK_R5_NAMES=json.dumps(names),AIRK_R5_ROOT=str(root),
               AIRK_R5_GUARD=str(guard),AIRK_R5_CHECK="created" if created else "parent")
    script = """
$ErrorActionPreference='Stop'
. $env:AIRK_R5_GUARD
$sid='S-1-5-21-3678370870-3858642417-826968105-1002'
$null=Assert-AirkHumanExecution $sid
foreach($name in ($env:AIRK_R5_NAMES|ConvertFrom-Json)){
    $p=Join-Path $env:AIRK_R5_OUTPUT $name
    if($env:AIRK_R5_CHECK -eq 'created'){$null=Assert-AirkCreatedOutput $p $env:AIRK_R5_ROOT $sid}
    else{$null=Assert-AirkOutputParent $p $env:AIRK_R5_ROOT $sid}
}
"""
    subprocess.run([str(shell),"-NoLogo","-NoProfile","-NonInteractive","-Command",script],check=True,env=env)


def compile_native(out, pins):
    from types import SimpleNamespace
    from phase_reference import digest as file_digest
    import run_compiler_batch
    source = Path(run_compiler_batch.__file__).resolve()
    require(file_digest(source) == "c7d7d8f27a603691171a1bdb15855709132dbee9cdc41d3fba2ab7a6b3aaed98",
            "original native compiler owner changed")
    image = CAMPAIGN/"windows-cu15-scientific-provider-stage-r1/fresh-cu15-compact-full-cutlass-main-r1-inference.exe"
    args = SimpleNamespace(image=image,image_sha256="b3d20acfea363da2afd1780078e07f46f4c3bcd52da5515aa4ed29c0cc192351",
                           log=out/"native.log",**{m:(r["path"],r["sha256"]) for m,r in pins.items()})
    result = run_compiler_batch.execute(args)
    check_outputs(out,["native.log"],created=True)
    receipt = out/"native.json"
    data = json.dumps(result,allow_nan=False,separators=(",",":")).encode()
    with receipt.open("xb") as file:
        file.write(data)
    check_outputs(out,["native.json"],created=True)
    require(result["passed"] is True and result["NN_invocations"] == 0,
            "native compiler check failed; see retained native receipt")
    return reference(receipt,digest(data))


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("--lifetime-sha256",action="append",required=True)
    parser.add_argument("--terminal",nargs=3,metavar=("OUTER_SHA","FIXTURE_SHA","RELEASE_SHA"))
    parser.add_argument("--output",type=Path,required=True)
    parser.add_argument("--compile",action="store_true",help="recompile both model projections with the original pinned CPU-only native selector")
    args=parser.parse_args()
    require(sys.flags.isolated and sys.flags.no_site and sys.dont_write_bytecode, "use checked Python -I -B -S")
    out=args.output
    require(out.is_absolute() and out.resolve(strict=True)==out and out.parent==CAMPAIGN
            and not any(out.iterdir()), "empty guarded campaign output required")
    outputs = ["compact.json","large.json","projection-receipt.json"] + (["native.json","native.log"] if args.compile else [])
    check_outputs(out,outputs,created=False)
    plan=read(reference(ROOT/"lifetime-plan.json",PLAN))
    reports=collect(plan,args.lifetime_sha256,args.terminal)
    phases=[]
    for report in reports:
        path=out/f"phase-{report['phase']:02}-independent-inputs.json"
        check_outputs(out,[path.name],created=False)
        raw=json.dumps(report,allow_nan=False,separators=(",",":")).encode()
        with path.open("xb") as file:
            file.write(raw)
        check_outputs(out,[path.name],created=True)
        pin={"path":str(path),"sha256":digest(raw)}
        require(read({**pin,"bytes":len(raw)})==report, "independent phase report changed before projection")
        phases.append(pin)
    batches=projections(plan,reports,phases)
    if not args.terminal:
        busy_mutex(plan)
    pins={}
    for model,data in batches.items():
        path=out/f"{model}.json"
        raw=json.dumps(data,ensure_ascii=False,allow_nan=False,separators=(",",":")).encode()
        require(len(raw)<=128<<20, "native compiler projection exceeds input limit")
        with path.open("xb") as file:
            file.write(raw)
        check_outputs(out,[path.name],created=True)
        pins[model]=reference(path,digest(raw))
    receipt = {"plan_sha256":PLAN,"lifetimes":args.lifetime_sha256,"terminal":args.terminal,
               "acquired_sha256":ACQUIRED,"models":pins,"original_requests":len(batches["compact"]["rows"]),
               "NN_invocations":0,"gain_certificates":0,"source_sha256":digest(Path(__file__).read_bytes())}
    if args.compile:
        receipt["original_compiler_verification"] = compile_native(out,pins)
        if not args.terminal:
            busy_mutex(plan)
    with (out/"projection-receipt.json").open("x",encoding="utf-8") as file:
        json.dump(receipt,file,allow_nan=False)
    check_outputs(out,["projection-receipt.json"],created=True)
    print(json.dumps(receipt))


if __name__=="__main__":
    main()
