# SPDX-License-Identifier: Apache-2.0
"""Run the ordinary Windows split CPU product builder with the reviewed static ORT environment."""
import argparse
import ctypes
import hashlib
import json
import os
from pathlib import Path
import runpy
import shutil
import stat
import subprocess
import sys
import time

ROOT = Path(__file__).resolve().parents[3]
E = ROOT/"artifacts/v190/final-plan/exec-20261116T052942Z"
SRC = ROOT/"scripts/build-service-runtime.py"
TOOLS = ROOT/"scripts/arbiter/evaluation/windows_cpu_ort.py"
ADMIN = E/"windows-allow-gpu-installed-r1/admin-inventory.json"
GIB = 1 << 30
FIRST = E/"windows-fresh-cpu-product-r1"
OUT = E/"windows-fresh-cpu-product-r2"
VOLUME_BASE = 1_233_246_265_344


def budget(stage, allowance, volume_free):
    require(stage < allowance-(128<<20), "CPU runtime and ORT outputs reached the stage allowance")
    growth = max(0, VOLUME_BASE-volume_free)
    require(growth < 120*GIB-(128<<20), "shared Windows tooling reached its original volume budget")
    require(volume_free > 100*GIB+max(0,182*GIB-growth), "campaign volume floor or outstanding reservations do not fit")
    return growth


def require(ok, why):
    if not ok:
        raise ValueError(why)


def allocated(root, counted_links=None):
    total = 4096
    pending = [root]
    while pending:
        try:
            with os.scandir(pending.pop()) as children:
                for entry in children:
                    info = entry.stat(follow_symlinks=False)
                    require(not info.st_file_attributes & stat.FILE_ATTRIBUTE_REPARSE_POINT,
                            "unexpected reparse point in monitored product root")
                    if stat.S_ISDIR(info.st_mode):
                        pending.append(entry.path)
                        total += 4096
                    else:
                        if counted_links is not None:
                            # Windows DirEntry.stat does not populate file IDs or link counts.
                            actual = os.stat(entry.path,follow_symlinks=False)
                            if actual.st_nlink > 1:
                                identity = (actual.st_dev,actual.st_ino)
                                if identity in counted_links:
                                    continue
                                counted_links.add(identity)
                            info = actual
                        total += (info.st_size+4095)//4096*4096
        except FileNotFoundError:
            # Cargo replaces transient files and incremental directories atomically.
            continue
    return total


def warm_usage(target):
    counted_links = set()
    return sum(allocated(target/name,counted_links) for name in ("debug","cargo-timings")) + sum(
        (p.stat().st_size+4095)//4096*4096 for p in target.iterdir() if p.is_file())


def database():
    root = Path("C:/Users/tdotn/.cargo")
    paths = sorted([*root.glob(".global-cache*"),*root.glob(".package-cache*")])
    return {p.name:[p.stat().st_size,p.stat().st_mtime_ns,hashlib.sha256(p.read_bytes()).hexdigest()] for p in paths}


def suspended_owner(c,b,parent,owner,python,deadline,cores,host_limit):
    mask = sum(1 << n for n in cores)
    cap = runpy.run_path(str(ROOT/"scripts/arbiter/analysis/public_source_admission/aws_normal_service.py"))["Limits"]
    def suspended(api,job,info,until):
        p = parent.suspended_authentication(api,info,deadline,until,b.SID,python,owner.image_proof)
        require(c["token_owner"](info.process)==b.SID,"wrong suspended image owner")
        limits=cap()
        def query():
            require(api.QueryInformationJobObject(job,9,ctypes.byref(limits),ctypes.sizeof(limits),None),"Job limit query failed")
        query()
        limits.basic.flags |= 0x10 | 0x20 | 0x8
        limits.basic.active_limit=32
        limits.basic.affinity,limits.basic.priority=mask,0x4000
        require(api.SetInformationJobObject(job,9,ctypes.byref(limits),ctypes.sizeof(limits)),"Job constraints refused")
        query()
        require(limits.job==limits.process==host_limit and limits.basic.active_limit==32 and
                limits.basic.affinity==mask and limits.basic.priority==0x4000,"actual Job limits differ")
        return dict(p,host_bytes=host_limit,affinity=cores,below_normal=True,active_process_limit=32)
    return suspended


def quiet_cpus(cal_pid):
    if cal_pid is None:
        sampler = runpy.run_path(str(ROOT/"scripts/arbiter/evaluation/selected_goal/cal_capture/timing/affinity.py"))
        placement = sampler["sample"](0)
        return placement, placement["logical_cpus"][:2]
    sampler = runpy.run_path(str(ROOT/"scripts/arbiter/analysis/development_margins/observe_cal_thread_affinity.py"))
    placement = sampler["observe"](cal_pid,10)
    return placement, sampler["pick_cpus"](placement,2)


def launch(args):
    import psutil
    ort = runpy.run_path(str(TOOLS))
    c,b,parent,owner,guard = ort["runtime"]()
    cargo = E/"windows-offline-cargo-r1"
    require(Path(os.environ["CARGO_HOME"]) == cargo/"home" and os.environ.get("CARGO_NET_OFFLINE") == "true"
            and b.pin(cargo/"execution.json")["sha256"] == "25cc58193438fbef6882cedc7640ef6111cc83452c5ae40b1f5d2c7024f3d72a",
            "verified shared D-only Cargo required")
    custody = E/args.custody
    require(custody.parent == E and custody.name in ("windows-cpu-ort-custody-r3.json","windows-cpu-ort-post-reclaim-r3.json")
            and b.pin(custody)["sha256"] == args.custody_sha256,"original CPU source custody required")
    target = ROOT/"target"
    require(not OUT.exists() and (target/"debug").is_dir(),"fresh product output and original warm target required")
    require(b.pin(FIRST/"execution.json")["sha256"]=="c1964d03c3ad207f9c31bd07da18c35f1accd39f1a10fbc0f64a7896e1e25c8e",
            "wrong original combined-budget refusal")
    stores = [FIRST,E/"o",E/"p",E/"q",*E.glob("windows-fresh-cpu-ort-r*"),*E.glob("windows-cpu-ort-*-control-r*")]
    allowance = 6*GIB - sum(allocated(path) for path in stores)
    require(allowance > 0,"original combined 6GiB cannot fit any product output")
    require(args.other_host_bytes >= 0,"invalid concurrent reservation")
    host = runpy.run_path(str(ROOT/"scripts/cuda-component-execution/ballast_host_commit.py"))["host"]()
    require(min(host["physical_available_bytes"],host["commit_limit_bytes"]-host["commit_bytes"]) >= args.other_host_bytes+14*GIB,
            "physical/commit reservation including candidate and system floor does not fit")
    require(not any(p.info["name"].lower() in ("cargo.exe","rustc.exe") for p in psutil.process_iter(["name"])),
            "another native target owner is active")
    placement_pid = args.cal_pid
    placement,cores = quiet_cpus(placement_pid)
    budget(0,allowance,shutil.disk_usage(ROOT).free)
    scratch = OUT/"temp"
    staging_parent = OUT/"staging"
    c["mkdir"](guard,OUT)
    c["mkdir"](guard,scratch)
    c["mkdir"](guard,staging_parent)
    original = runpy.run_path(str(SRC))
    source = original["source_identity"]()
    static = runpy.run_path(str(ROOT/"scripts/static-runtime-environment.py"))
    env = {k:v for k,v in os.environ.items() if not k.startswith(("CUDA_","ORT_","AIRK_","CMAKE_"))}
    env.update(static["load_environment_file"](E/"windows-fresh-cpu-ort-r3/cpu-environment.json",E/"q",ort["TARGET"]))
    env.update(TEMP=str(scratch),TMP=str(scratch),TMPDIR=str(scratch),CARGO_TARGET_DIR=str(target),
               CARGO_BUILD_JOBS="2",CARGO_INCREMENTAL="1",VSCMD_SKIP_SENDTELEMETRY="1",PYTHONDONTWRITEBYTECODE="1",
               AIRK_BUNDLED_MODEL_DIR=str(ROOT/"artifacts/indexing-plan/minilm-fp16-build-inputs-r1"))
    command = [sys.executable,"-I","-B","-S",str(SRC),"--destination",str(OUT/"runtime"),"--profile","debug",
               "--admin-inventory",str(ADMIN),"--staging-parent",str(staging_parent)]
    proof_paths = [Path(__file__).resolve(),Path(__file__).with_name("audit-windows-product-target.ps1"),TOOLS,SRC,ADMIN,custody,FIRST/"execution.json",cargo/"execution.json",E/"windows-fresh-cpu-ort-r3/cpu-environment.json"]
    pins = [b.pin(p) for p in proof_paths]
    deadline = time.monotonic()+args.seconds
    suspended = suspended_owner(c,b,parent,owner,b.pin(Path(sys.executable)),deadline,cores,6*GIB)
    native = set()
    audit = Path(__file__).with_name("audit-windows-product-target.ps1")
    def custody_tree(kind):
        return json.loads(subprocess.check_output([guard["powershell"]["path"],"-NoLogo","-NoProfile","-NonInteractive",
                          "-File",str(audit),"-Kind",kind],text=True,timeout=300))
    custody_before = custody_tree("target")
    baseline = warm_usage(target)
    before_db = database()
    highwater = 0
    tooling_highwater = 0
    checked_stages = set()
    def observe(api,job,info,until):
        nonlocal highwater, tooling_highwater
        clock = 0
        while api.WaitForSingleObject(info.process,200)==258:
            require(time.monotonic()<until,"original product deadline expired")
            if time.monotonic() >= clock:
                growth = allocated(OUT)
                tooling_highwater = max(tooling_highwater,budget(growth,allowance,shutil.disk_usage(ROOT).free))
                highwater = max(highwater,growth)
                for stage in staging_parent.iterdir():
                    if stage.name.startswith(".airk-runtime-stage-") and stage not in checked_stages:
                        checked_stages.add(stage)
                        c["guard"](guard,stage/"airk.exe")
                members,_,_ = owner.job_snapshot(api,job)
                for pid in members:
                    try:
                        proc = psutil.Process(pid)
                        name = proc.name().lower()
                        if name in native or name not in ("cargo.exe","rustc.exe","link.exe"):
                            continue
                        actual = {k.upper():v for k,v in proc.environ().items()}
                        require(all(Path(actual[k])==scratch for k in ("TMP","TEMP","TMPDIR")) and
                                Path(actual["CARGO_HOME"]) == cargo/"home","C scratch or default Cargo home reached live compiler")
                        if name in ("cargo.exe","rustc.exe"):
                            require(Path(proc.exe()).samefile(env["CARGO" if name=="cargo.exe" else "RUSTC"]),"wrong compiler image")
                        native.add(name)
                    except psutil.NoSuchProcess:
                        continue
                clock = time.monotonic()+1
        return {"highwater":highwater,"native_tools":sorted(native)},[]
    log,stdin=OUT/"build.log",OUT/"stdin"
    c["guard_many"](guard,[log,stdin])
    result={"passed":False,"source_identity":source,"argv":command,"inputs":pins,"host":host,"cpus":cores,
            "placement_sample":placement,"placement_observed_pid":placement_pid,"concurrent_cal_pid":args.cal_pid,"other_host_bytes":args.other_host_bytes,"combined_limit":6*GIB,"remaining_storage":allowance,"baseline_target":baseline}
    try:
        with stdin.open("xb"):
            pass
        with stdin.open("rb") as inp,log.open("xb") as out:
            job=owner.run_windows_owned_training(command,ROOT,out,deadline-time.monotonic(),env,inp,6*GIB,
                before_resume=suspended,after_resume=observe,absolute_deadline=deadline)
        result["job"]=job
        require(job["exit_code"]==0 and not job["timed_out"] and not job["owned_job_forced_termination"] and
                job["owned_descendants_settled"] and job["owned_job_final_active_processes"]==0 and
                job["owned_job_final_member_pids"]==[],"original product build refused or failed to settle")
        require(database()==before_db and original["source_identity"]()==source,"C cache or original source changed")
        for pin in pins:
            b.pinned(pin)
        files=[p for p in (OUT/"runtime").rglob("*") if p.is_file()]
        c["guard_many"](guard,files,True)
        result["files"]=[b.pin(p) for p in files]
        require((OUT/"runtime/airk.exe").is_file() and (OUT/"runtime/airkctl.exe").is_file(),"split product missing")
        result["custody_before"] = custody_before
        require(len(checked_stages)==1,"original guarded stock staging child was not observed")
        result["custody_after"] = [custody_tree("target"),custody_tree("product")]
        result["target_growth_bytes"] = warm_usage(target)-baseline
        result["product_allocated_upper_bytes"] = allocated(OUT)
        require(result["product_allocated_upper_bytes"] <= allowance,"combined output exceeds original allocation")
        result["passed"]=True
    except BaseException as error:
        result["error"]=repr(error)
        raise
    finally:
        result.update(highwater=highwater,tooling_highwater_bytes=tooling_highwater,native_tools=sorted(native),log=b.pin(log),c_cache_before=before_db,c_cache_after=database())
        c["write_new"](guard,OUT/"execution.json",result)


if __name__=="__main__":
    p=argparse.ArgumentParser(description=__doc__)
    p.add_argument("--cal-pid",type=int,help="Actual current CAL service when running")
    p.add_argument("--other-host-bytes",required=True,type=int)
    p.add_argument("--custody",required=True)
    p.add_argument("--custody-sha256",required=True)
    p.add_argument("--seconds",type=int,default=3600)
    launch(p.parse_args())
