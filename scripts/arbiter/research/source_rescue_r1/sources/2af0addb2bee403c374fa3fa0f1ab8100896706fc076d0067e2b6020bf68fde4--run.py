# SPDX-License-Identifier: Apache-2.0
"""One reviewed private cold-only diagnostic. No actual run without exact plan hash."""
import argparse
import hashlib
import importlib.util
import json
import os
from pathlib import Path
import subprocess
import time
import capture


def sha(path):
    with Path(path).open("rb") as source:return hashlib.file_digest(source,"sha256").hexdigest()


def load(candidate,name):
    spec=importlib.util.spec_from_file_location("private_cold_"+name.replace("-","_"),candidate/"scripts"/(name+".py"))
    module=importlib.util.module_from_spec(spec);spec.loader.exec_module(module);return module


def source_pins(plan):
    candidate=Path(plan["candidate"])
    if subprocess.check_output(["git","-C",str(candidate),"rev-parse","HEAD"],text=True).strip()!=plan["commit"]:raise ValueError("Candidate commit changed")
    if subprocess.check_output(["git","-C",str(candidate),"status","--porcelain"],text=True):raise ValueError("Candidate source is not clean")
    frozen=json.loads(Path(plan["candidate_input_pins"]).read_bytes())
    for name,expected in frozen["sources"].items():
        if sha(candidate/name)!=expected:raise ValueError("Candidate source changed: "+name)
    for name,expected in frozen["compiled_outputs"].items():
        if sha(candidate/"packages/vscode/out"/name)!=expected:raise ValueError("Compiled output changed: "+name)
    for pin in plan["pins"].values():
        if sha(pin["path"])!=pin["sha256"]:raise ValueError("Diagnostic input pin changed: "+pin["path"])


def main():
    ap=argparse.ArgumentParser(description=__doc__);ap.add_argument("--plan",type=Path,required=True);ap.add_argument("--plan-sha256",required=True);args=ap.parse_args()
    if sha(args.plan)!=args.plan_sha256:raise ValueError("Reviewed plan changed")
    plan=json.loads(args.plan.read_bytes());candidate=Path(plan["candidate"])
    processes=load(candidate,"owned-runtime-processes")
    if processes._backend().user!=plan["human_sid"]:raise ValueError("Execution identity differs from reviewed human")
    source_pins(plan)
    if Path(os.environ.get("AIRK_PRIVATE_CENSUS_BASE","")).resolve()!=Path(plan["fixture_base"]):raise ValueError("Exact approved private fixture base required")
    processes._safe(Path(plan["fixture_base"]),directory=True)
    package=load(candidate,"package-native");runtime=Path(plan["runtime"])
    files,_=package.validate_runtime(runtime,"1.8.207","x86_64-pc-windows-msvc",plan["C7"])
    for name,expected in plan["signed_images"].items():
        if files[name]!=expected:raise ValueError("Signed runtime image changed")
    output=Path(plan["output"])
    if output.exists() or output.is_symlink():raise FileExistsError("Diagnostic output must be fresh")
    output.mkdir()
    clients=load(candidate,"check-service-clients");recorder=capture.Capture(output/"events.jsonl")
    result,primary=None,None
    try:
        with capture.observe_clients(clients,processes,recorder,plan["human_sid"]):
            result=clients.qualify_cold(runtime)  # Exact frozen two-phase entrypoint.
    except BaseException as error:
        primary=error
    verification_errors=[]
    try:
        source_pins(plan)
        if package.inventory(runtime)!=files:raise ValueError("Signed runtime changed during diagnostic")
    except BaseException as error:
        verification_errors.append(capture.failure(error))
    recorder.emit("final_clock",elapsed_seconds=time.monotonic()-recorder.origin,
                  native_error=capture.failure(primary) if primary else None)
    recorder.close()
    handles=[r for r in recorder.rows if r["kind"]=="native_process_handle_after_stop"]
    custody_complete=bool(handles) and all(r["handle_signaled"] and isinstance(r["native_stop"],dict)
        and r["native_stop"].get("record",{}).get("phase")=="complete" and r["source_files"]==[] for r in handles)
    summary={"schema":"airk.private.signed-cold-diagnostic/1","native_result":result,
        "native_error":capture.failure(primary) if primary else None,"native_error_rewritten":False,
        "capture_complete":not recorder.errors,"capture_errors":recorder.errors,
        "source_verification_errors":verification_errors,"custody_complete":custody_complete,
        "all_native_process_handle_observations":handles,"signals_sent":0,
        "case_clock":"Original qualify_cold240s per case; original60s read deadline after admission; unchanged41 typed-attempt policy; SDK125s transport clamped by existing caller",
        "final_clock":[r for r in recorder.rows if r["kind"]=="final_clock"],
        "plan_sha256":args.plan_sha256,"capture_source_sha256":sha(Path(capture.__file__)),
        "scope":"Cold-only actual fixture diagnostic, not passing full signed preflight, model qualification or delivery acceptance",
        "passed":primary is None and not recorder.errors and not verification_errors and custody_complete}
    with (output/"summary.json").open("x",encoding="utf8") as f:json.dump(summary,f,indent=2)
    print(json.dumps({"diagnostic":str(output),"native_error_type":type(primary).__name__ if primary else None,
                      "capture_complete":not recorder.errors,"custody_complete":custody_complete}))
    if primary is not None:raise primary
    if not summary["passed"]:raise RuntimeError("Private diagnostic observations incomplete; retain original evidence")

if __name__=="__main__":main()
