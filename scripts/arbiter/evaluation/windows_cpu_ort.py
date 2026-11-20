# SPDX-License-Identifier: Apache-2.0
"""Run the stock Windows CPU ORT producer with authenticated offline inputs in a private D: Job."""
import argparse
import ctypes
import hashlib
import json
import os
from pathlib import Path
import runpy
import shutil
import sys
import time


ROOT = Path(__file__).resolve().parents[3]
E = ROOT/"artifacts/v190/final-plan/exec-20261116T052942Z"
DEST = E/"o"
OUT = E/"windows-fresh-cpu-ort-r1"
HIST = ROOT/"artifacts/advanced-split/ort130-x64-owned-r5"
PRODUCER = ROOT/"scripts/provision-static-onnx-runtime.py"
TARGET = "x86_64-pc-windows-msvc"
GIB = 1 << 30
LIMIT = 5 * GIB
STOP = LIMIT - 320 * (1 << 20)


def require(ok, detail):
    if not ok:
        raise ValueError(detail)


def size(root):
    total = 0
    for parent, dirs, files in os.walk(root):
        total += 4096 * (1 + len(dirs))
        for file in files:
            try:
                n = (Path(parent)/file).stat().st_size
            except FileNotFoundError:
                continue
            total += ((n + 4095)//4096)*4096
    return total


def runtime():
    c = runpy.run_path(str(ROOT/"scripts/cuda-component-execution.py"))
    b = c["base"]()
    parent, owner = b.helpers({"ownership_helper_root":str(b.MAINTAINED_HELPER_ROOT)})
    require(parent.worker.windows_sid() == c["token_owner"](ctypes.c_void_p(-1)) == b.SID,"wrong build owner")
    guard = {"checked_guard":b.pin(ROOT/"scripts/dev/assert-human-execution.ps1"),
             "powershell":b.pin(Path("C:/Program Files/PowerShell/7/pwsh.exe"))}
    return c,b,parent,owner,guard


def verify_seed(path):
    stock = runpy.run_path(str(PRODUCER))
    stock["validate_lock"]()
    stock["owned_destination"](path,TARGET)
    stock["verify_source"](path,TARGET)
    lock = stock["LOCK"]
    rows = [("source.tar.gz",lock["source"],{"source":True})]
    rows.extend((r["name"]+".zip",r,{}) for r in stock["selected_dependencies"](TARGET))
    rows.extend((r["cache_name"],r,{"notice":True}) for r in lock["additional_notices"])
    for name,row,kind in rows:
        stock["fetch"](path/"downloads"/name,row,False,**kind)
    return {"source_inventory":stock["digest"](path/"source-files.json"),"locked_inputs":len(rows)}


def worker(control):
    c,b,_,_,guard = runtime()
    stock = runpy.run_path(str(PRODUCER))
    stock["validate_lock"]()
    require(Path(os.environ["TEMP"]) == OUT/"temp","private D: scratch required")
    original_tools = json.loads((HIST/"build-receipt.json").read_bytes())["toolchain"]
    if control:
        c["mkdir_siblings"](guard,[OUT/"commands",OUT/"logs"],deadline=time.monotonic()+20)
        actual = stock["windows_command"]([original_tools["compiler"]["path"],"/help"],original_tools,OUT,"vcvars-test")
        result = stock["run_owned"](actual,OUT,OUT/"vcvars.log",30)
        c["write_new"](guard,OUT/"vcvars.json",result)
        require(stock["successful"](result),"stock nested vcvars and actual cl.exe diagnostic failed")
        return
    transfers = {"p":(1,"0a86efb10844d53416fe0ef29721f203233b81da994211455b87aaeb2e652b19"),
                 "q":(2,"41aa9909245ee64106000ba522b0d04e3c7ecf3d9f71ef33ecbbf4d176b08fa7")}
    if DEST.name in transfers:
        ordinal,expected = transfers[DEST.name]
        transfer = b.pin(E/f"windows-cpu-ort-input-transfer-r{ordinal}.json")
        require(transfer["sha256"] == expected,
                "authenticated source transfer differs")
        require(verify_seed(DEST)["locked_inputs"] == 16 and not (DEST/"build-attempt.json").exists(),
                "transferred stock source closure changed or destination already attempted")
    else:
        require(not DEST.exists(),"fresh original stock destination required")
        prepare_seed(c,guard,stock)
    produce(c,b,guard,stock)


def prepare_seed(c,guard,stock):
    c["guard"](guard,DEST)
    stock["owned_destination"](DEST,TARGET,create=True)
    c["guard_many"](guard,[DEST,DEST/"owner.json"],True)
    cache = DEST/"downloads"
    c["mkdir"](guard,cache)
    lock = stock["LOCK"]
    rows = [("source.tar.gz",lock["source"],{"source":True})]
    rows.extend((v["name"]+".zip",v,{}) for v in stock["selected_dependencies"](TARGET))
    rows.extend((v["cache_name"],v,{"notice":True}) for v in lock["additional_notices"])
    inputs = [HIST/"downloads"/name for name,_,_ in rows]
    c["guard_many"](guard,inputs,True)
    c["guard_many"](guard,[cache/name for name,_,_ in rows])
    for name, row, kind in rows:
        old, new = HIST/"downloads"/name,cache/name
        stock["fetch"](old,row,False,**kind)
        with old.open("rb") as original,new.open("xb") as target:
            shutil.copyfileobj(original,target,1<<20)
        stock["fetch"](new,row,False,**kind)
    c["guard_many"](guard,[cache/name for name,_,_ in rows],True)


def produce(c,b,guard,stock):
    tools = json.loads((HIST/"build-receipt.json").read_bytes())["toolchain"]
    for key in ("cmake","ninja","patch","vcvars","python"):
        require(b.pin(Path(tools[key]["path"]))["sha256"] == tools[key]["sha256"],"original tool changed: "+key)
    # Stock preparation, fresh configure/build, lib aggregation and additively verified handoff.
    args = ["--target",TARGET,"--destination",str(DEST),"--build","--parallel","2","--cmake",tools["cmake"]["path"],
            "--ninja",tools["ninja"]["path"],"--patch",tools["patch"]["path"],"--environment-file",str(OUT/"cpu-environment.json")]
    c["write_new"](guard,OUT/"producer-command.json",{"argv":[sys.executable,str(PRODUCER),*args],
                   "fresh_source":True,"prior_build_used":False,"producer_sha256":b.pin(PRODUCER)["sha256"]})
    require(stock["main"](args) == 0,"stock CPU ORT producer failed")


LIBSHA = "69317eda3d44fd723b959c87d5a3b0617e3b7be70b02ff3f51ec45288ec559d2"


def expected_tools():
    tools = json.loads((HIST/"build-receipt.json").read_bytes())["toolchain"]
    bindir = Path(tools["compiler"]["path"]).parent
    return {"cmake.exe":(Path(tools["cmake"]["path"]),tools["cmake"]["sha256"]),
            "ninja.exe":(Path(tools["ninja"]["path"]),tools["ninja"]["sha256"]),
            "cl.exe":(bindir/"cl.exe","e87e78b3ef6580b75aebda843f8a22e874612d9f766344046551399f898eb226"),
            "link.exe":(bindir/"link.exe","49a5df25c3182a22684375b1d62a025016deffccea247894fe5ca2c85610c6a6"),
            "lib.exe":(bindir/"lib.exe",LIBSHA)}


def actual_tool(name,observed,tools,base):
    raw = Path(observed)
    expected,sha = tools[name]
    require(raw.is_absolute() and raw.samefile(expected),"unexpected native compiler/tool file identity")
    canonical = raw.resolve(strict=True)
    require(canonical == expected.resolve(strict=True),"observed tool is outside the toolchain")
    pinned = base.pin(canonical)
    require(pinned["sha256"] == sha,"observed compiler/tool digest differs")
    return pinned


def launch(cal_pid,control):
    import psutil
    c,b,parent,owner,guard = runtime()
    require(not OUT.exists() and not (os.environ.get("ORT_LIB_LOCATION") or os.environ.get("CMAKE_GENERATOR")),
            "fresh scoped output and clean original environment required")
    require(DEST.drive == OUT.drive == "D:" and shutil.disk_usage(E).free > LIMIT+8*GIB,"insufficient D: admission")
    affinity = runpy.run_path(str(ROOT/"scripts/arbiter/analysis/development_margins/observe_cal_thread_affinity.py"))
    observed = affinity["observe"](cal_pid,10)
    cpus = affinity["pick_cpus"](observed,2)
    mask = sum(1 << i for i in cpus)
    cap = runpy.run_path(str(ROOT/"scripts/arbiter/analysis/public_source_admission/aws_normal_service.py"))["Limits"]
    scratch = OUT/"temp"
    c["mkdir"](guard,OUT)
    c["mkdir"](guard,scratch)
    source = b.pin(Path(__file__).resolve())
    python = b.pin(Path(sys.executable))
    require(python["sha256"]=="d932e5e2f324d57f392e8fd063dcf6d0185be8a664c57c6d24e7762ed02c28ca","wrong Python")
    memory = psutil.virtual_memory().available
    require(memory >= 70*GIB,"insufficient physical host capacity for concurrent reservations")
    placement = {"cal_pid":cal_pid,"cpu_sample":observed,"cpus":cpus,"host_bytes":6*GIB,"disk_limit":LIMIT,
                 "scratch":str(scratch),"memory_available":memory}
    c["write_new"](guard,OUT/"admission.json",placement)
    deadline = time.monotonic()+10800
    def suspended(api,job,info,until):
        expected = python
        if control and DEST == E/"q":
            process = psutil.Process(info.pid)
            observed_exe = process.exe()
            expected = actual_tool("lib.exe",observed_exe,tools,b)
            actual_env = {k.upper():v for k,v in process.environ().items() if k.upper() in ("TMP","TEMP","TMPDIR")}
            require(set(actual_env)=={"TMP","TEMP","TMPDIR"} and all(Path(v)==scratch for v in actual_env.values()),
                    "native lib diagnostic scratch differs")
            pins["lib.exe"] = {"pid":info.pid,"image":expected,"scratch":actual_env,"observed_exe":observed_exe}
        proof = parent.suspended_authentication(api,info,deadline,until,b.SID,expected,owner.image_proof)
        require(c["token_owner"](info.process)==b.SID,"wrong child default owner")
        limits = cap()
        def query():
            require(api.QueryInformationJobObject(job,9,ctypes.byref(limits),ctypes.sizeof(limits),None),"failed to observe Job")
        query()
        limits.basic.flags |= 0x10 | 0x20 | 0x8
        limits.basic.active_limit = 16
        limits.basic.affinity, limits.basic.priority = mask, 0x4000
        require(api.SetInformationJobObject(job,9,ctypes.byref(limits),ctypes.sizeof(limits)),"failed to restrict build")
        query()
        require(limits.job == limits.process == 6*GIB and limits.basic.affinity == mask and
                limits.basic.priority == 0x4000 and limits.basic.active_limit == 16,"actual Windows CPU/memory/membership limits differ")
        return proof
    tools = expected_tools()
    previous = 0
    if DEST == E/"q" and not control:
        prior_roots = [E/"o",E/"p",*E.glob("windows-cpu-ort-*-control-r*"),*E.glob("windows-fresh-cpu-ort-r[12]")]
        previous = sum(size(d) for d in prior_roots) + sum(p.stat().st_size for p in E.glob("windows-cpu-ort-input-transfer-r*.json"))
    pins, maximum, stage = {},0,None
    def observe(api,job,info,until):
        nonlocal maximum,stage
        last = 0
        while api.WaitForSingleObject(info.process,250) == 258:
            require(time.monotonic() < until,"stock producer exceeded original wall deadline")
            members,_,_ = owner.job_snapshot(api,job)
            for pid in members:
                if pid == info.pid:
                    continue
                try:
                    process = psutil.Process(pid)
                    exe = process.exe()
                    name = Path(exe).name.lower()
                    if name in pins or name not in ("cl.exe","cmake.exe","ninja.exe","lib.exe","link.exe"):
                        continue
                    env = {k.upper():v for k,v in process.environ().items() if k.upper() in ("TMP","TEMP","TMPDIR")}
                    require(set(env)=={"TMP","TEMP","TMPDIR"} and all(Path(v)==scratch for v in env.values()),
                            name+" inherited non-D scratch")
                    require(set(process.cpu_affinity())==set(cpus) and process.nice()==psutil.BELOW_NORMAL_PRIORITY_CLASS,
                            name+" escaped quiet-core/priority limits")
                    pins[name] = {"pid":pid,"observed_exe":exe,"image":actual_tool(name,exe,tools,b),"scratch":env,"affinity":cpus}
                except (psutil.NoSuchProcess,ProcessLookupError):
                    continue
            if time.monotonic()-last > 3:
                maximum = max(maximum,size(DEST)+size(OUT))
                require(maximum < STOP and previous + maximum < LIMIT - (128 << 20),
                        "CPU ORT aggregate writes reached the disk stop watermark")
                last = time.monotonic()
                stage = "build" if (DEST/"build-attempt.json").exists() else "prepare"
        return {"compiler_processes":pins,"max_allocated_estimate":maximum,"stage":stage},[]
    env = {k:v for k,v in os.environ.items() if not k.startswith(("CUDA_","ORT_","AIRK_","CMAKE_"))}
    env.update(TEMP=str(scratch),TMP=str(scratch),TMPDIR=str(scratch),VSCMD_SKIP_SENDTELEMETRY="1",
               CMAKE_GENERATOR_NO_COMPILER_ENV="1",PYTHONDONTWRITEBYTECODE="1",CARGO_NET_OFFLINE="true")
    cmd = [sys.executable,"-I","-B","-S",str(Path(__file__).resolve()),"--worker","--attempt",str(3 if DEST == E/"q" else 2 if DEST == E/"p" else 1)]
    if control:
        cmd.append("--control")
        if DEST == E/"q":
            # CMake generated this exact short-path librarian spelling before the original observation failed.
            import re
            rules = (E/"p/b/Release/CMakeFiles/rules.ninja").read_text(encoding="utf-8")
            alias = re.search(r"(C:\\PROGRA~1\\[^\s]+?\\lib\.exe) /nologo",rules,re.I).group(1)
            require(Path(alias) != Path(alias).resolve(strict=True),"real DOS alias required by this diagnostic")
            actual_tool("lib.exe",alias,tools,b)
            library = next((HIST/"aggregate").glob("*.lib"))
            cmd = [alias,"/nologo","/list",str(library)]
    empty,log = OUT/"stdin",OUT/"native.log"
    c["guard_many"](guard,[empty,log])
    with empty.open("xb"):
        pass
    result = {"passed":False,"argv":cmd,"source":source}
    try:
        with empty.open("rb") as inp,log.open("xb") as sink:
            job = owner.run_windows_owned_training(cmd,ROOT,sink,deadline-time.monotonic(),env,inp,6*GIB,
                      before_resume=suspended,after_resume=observe,absolute_deadline=deadline)
        result["job"] = job
        require(job["exit_code"] == 0 and not job["timed_out"] and not job["owned_job_forced_termination"] and
                job["owned_descendants_settled"] and job["owned_job_final_active_processes"]==0 and
                job["owned_job_final_member_pids"]==[],"stock native build refused or failed to drain")
        if not control:
            require({"cmake.exe","cl.exe","ninja.exe","lib.exe"} <= pins.keys(),"missing actual compiler scratch observations")
        require(previous+size(DEST)+size(OUT)<LIMIT and b.pin(Path(__file__).resolve())==source,"disk limit or source changed")
        result["passed"] = True
    except BaseException as error:
        result["error"] = repr(error)
        raise
    finally:
        result.update(compiler_processes=pins,peak_estimated_allocated=maximum,previous_attempt_allocated=previous,log=b.pin(log),source_check_required=True)
        c["write_new"](guard,OUT/"execution.json",result)
    require(result["passed"],"stock CPU ORT requires successful source and archive audit")


if __name__ == "__main__":
    parser=argparse.ArgumentParser(description=__doc__)
    parser.add_argument("--worker",action="store_true")
    parser.add_argument("--cal-pid",type=int)
    parser.add_argument("--attempt",type=int,choices=(1,2,3),default=1)
    parser.add_argument("--control",action="store_true")
    parser.add_argument("--verify-seed",choices=("o","p","q"))
    args=parser.parse_args()
    if args.verify_seed:
        print(json.dumps(verify_seed(E/args.verify_seed)))
        sys.exit(0)
    if args.attempt==2:
        DEST,OUT=E/"p",E/"windows-fresh-cpu-ort-r2"
    if args.attempt==3:
        DEST,OUT=E/"q",E/"windows-fresh-cpu-ort-r3"
    if args.control:
        OUT=E/("windows-cpu-ort-lib-alias-control-r1" if args.attempt==3 else "windows-cpu-ort-vcvars-control-r2")
    worker(args.control) if args.worker else launch(args.cal_pid,args.control)
