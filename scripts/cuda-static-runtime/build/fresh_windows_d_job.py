# SPDX-License-Identifier: Apache-2.0
"""Own the single prospective CU18 build from reviewed inputs through the original archive."""
import argparse
import ctypes
import datetime as dt
import hashlib
import json
import os
from pathlib import Path
import runpy
import shutil
import subprocess
import sys
import time

HERE = Path(__file__).resolve().parent
d = runpy.run_path(str(HERE / "fresh_windows_d.py"))
ROOT, OUT, E, GIB = (d[key] for key in ("REPO", "OUTPUT", "CAMPAIGN", "GIB"))
PLAN = E / "windows-cu18-reviewed-build-r1"
RUN = E / "windows-cu18-native-build-r1"
CONFIG = PLAN / "configure-recipe.json"
BINDING = PLAN / "review.json"
HIST_ARCHIVE = E / "fresh-cuda-aggregate-cu15-r1.json"
HIST_ARCHIVE_SHA = "c15530192116ce2886c3c1b8deec5f2fedafec78997e6e301b38136d745dc9d6"
HOST_CAP, DISK_CAP = 32 * GIB, 12 * GIB
require = d["require"]


def runtime():
    return runpy.run_path(str(ROOT / "scripts/arbiter/evaluation/windows_cpu_ort.py"))["runtime"]()


def sources():
    return [Path(__file__).resolve(), HERE / "fresh_windows_d.py", ROOT / "scripts/cuda-runtime-environment.py",
            *[HERE / name for name in d["SOURCE_SHA"]], ROOT / "scripts/cuda-component-execution.py",
            ROOT / "scripts/dev/assert-human-execution.ps1", ROOT / "scripts/cuda-native-execution.py",
            ROOT / "scripts/arbiter/evaluation/windows_cpu_ort.py",
            ROOT / "scripts/arbiter/analysis/public_source_admission/aws_normal_service.py",
            ROOT / "scripts/arbiter/evaluation/selected_goal/cal_capture/timing/affinity.py",
            ROOT / "scripts/cuda-component-execution/ballast_host_commit.py"]


def git_bytes(args):
    git = Path("C:/Program Files/Git/cmd/git.exe")
    return subprocess.check_output([str(git), "--no-optional-locks", "-C", str(ROOT), *args], timeout=30)


def signed_sources(head, base):
    message = git_bytes(["show", "-s", "--format=%B", head])
    require(b"\nSigned-off-by:" in message, "DCO reviewed commit required")
    rows = []
    for path in sources():
        key = path.relative_to(ROOT).as_posix()
        original = git_bytes(["cat-file", "blob", f"{head}:{key}"])
        pin = base.pin(path)
        require(hashlib.sha256(original).hexdigest() == pin["sha256"], "reviewed source differs: " + key)
        rows.append(pin)
    return rows


def prepare():
    c, base, _, _, guard = runtime()
    require(not PLAN.exists() and not OUT.exists() and not RUN.exists(), "fresh D build/plan required")
    head = git_bytes(["rev-parse", "--verify", "HEAD"]).decode().strip()
    owners = signed_sources(head, base)
    reviewed = dt.datetime.now(dt.timezone.utc)
    recipe = d["current_recipe"](head, reviewed.isoformat())
    d["cutoff"](reviewed, reviewed)
    compiler = d["owners"]()[1]
    pins = compiler.load(compiler.OWNERS["pin_reader"], "cu18_pins")
    compiler.verify_frozen_inputs(pins, recipe, time.monotonic()+600)
    archive = json.loads(d["read"](HIST_ARCHIVE, HIST_ARCHIVE_SHA))
    for pin in (*archive["owners"].values(), archive["archiver"]):
        pins.check_pin(pin)
    c["mkdir"](guard, PLAN)
    c["write_new"](guard, CONFIG, recipe)
    c["write_new"](guard, BINDING, {"schema":"airk.private.cuda-build-admission/1", "head":head,
                  "configure":base.pin(CONFIG), "owners":owners, "original_configure":base.pin(d["ORIGINAL"]),
                  "original_archive":base.pin(HIST_ARCHIVE), "output":str(OUT), "wall_seconds":10800,
                  "archive_seconds":300, "terminal":d["END"].isoformat(), "host_cap":HOST_CAP,
                  "disk_reservation":DISK_CAP})
    return base.pin(BINDING)


def checked_plan(sha, base):
    value = json.loads(d["read"](BINDING, sha))
    require(value["owners"] == signed_sources(value["head"], base) and value["configure"] == base.pin(CONFIG)
            and value["output"] == str(OUT), "prospective source/recipe binding differs")
    body = d["check_recipe"](CONFIG, value["configure"]["sha256"])
    require(body["head"] == value["head"], "reviewed head differs")
    return value, body


def worker(sha):
    c, base, _, _, guard = runtime()
    inside = ctypes.c_int()
    require(ctypes.windll.kernel32.IsProcessInJob(ctypes.c_void_p(-1),None,ctypes.byref(inside)) and inside.value,
            "resource-bound Windows Job required")
    binding, recipe = checked_plan(sha, base)
    import psutil
    started = json.loads((RUN/"admission.json").read_bytes())
    outside = psutil.Process().parent()
    require(outside is not None and started["owner_pid"] == outside.pid and
            abs(started["owner_created"]-outside.create_time()) < 0.01 and
            started["argv"][1:] == ["-I","-B",str(Path(__file__).resolve()),"worker","--sha256",sha],
            "genuine original resource supervisor required")
    require(not OUT.exists() and RUN.is_dir() and (RUN/"admission.json").is_file(), "owned original launch required")
    configured = d["execute"]("configure", CONFIG, binding["configure"]["sha256"])
    compiled = d["execute"]("compile", Path(configured["path"]), configured["sha256"])
    archive = json.loads(d["read"](HIST_ARCHIVE, HIST_ARCHIVE_SHA))
    archive["compile_receipt"] = compiled
    path = RUN / "archive-recipe.json"
    c["write_new"](guard, path, archive)
    result = d["execute"]("aggregate", path, base.pin(path)["sha256"])
    checked_plan(sha, base)
    c["write_new"](guard, RUN/"result.json", {"configured":configured,"compiled":compiled,"archive":result})


def verify_outer(facts, deadline):
    c, base, _, _, guard = runtime()
    final = RUN/"execution.json"
    receipt_path = RUN/"result.json"
    admission_path = RUN/"admission.json"
    paths = [BINDING, CONFIG, final, receipt_path, admission_path]
    c["guard_many"](guard,paths,True,deadline=deadline)
    before = [base.pin(path) for path in paths]
    execution = json.loads(d["read"](final,before[2]["sha256"]))
    binding, recipe = checked_plan(execution["binding_sha256"],base)
    require(before[0]["sha256"] == execution["binding_sha256"] and binding["configure"] == before[1]
            and facts["configure"]["recipe"] == before[1] and recipe == facts["configure_recipe"],
            "original D resource owner or configure source differs")
    outputs = [base.pin(OUT / name) for name in ("fresh-configure.json","fresh-compile.json","fresh-aggregate.json")]
    result = json.loads(d["read"](receipt_path,before[3]["sha256"]))
    require(execution["passed"] is True and execution["outputs"] == outputs
            and execution["worker_result"] == before[3] and execution["admission"] == before[4]
            and result == dict(zip(("configured","compiled","archive"), outputs, strict=True))
            and outputs[-1] == facts["aggregate_receipt"], "D build output does not join original owned run")
    admission = json.loads(d["read"](admission_path,before[4]["sha256"]))
    require(admission["argv"] == [recipe["tools"]["python"]["path"],"-I","-B",str(Path(__file__).resolve()),
                                 "worker","--sha256",before[0]["sha256"]]
            and admission["host_cap"] == HOST_CAP and admission["disk_reservation"] == DISK_CAP,
            "D original launch arguments or reservations differ")
    job = execution["job"]
    grant = job["suspended_assignment"]
    require(job["exit_code"] == 0 and not job["timed_out"] and not job["log_oversized"]
            and job["owned_descendants_settled"] and not job["owned_job_forced_termination"]
            and job["owned_job_final_active_processes"] == 0 and job["owned_job_final_member_pids"] == []
            and job["child_assigned_before_resume"] and job["whole_process_deadline_not_refreshed"]
            and grant["host_cap"] == HOST_CAP and grant["below_normal"] and
            len(set(grant["placement"]["logical_cpus"])) == 8, "D original CUDA Job or affinity did not qualify")
    require(time.monotonic() < deadline and before == [base.pin(path) for path in paths],
            "D custody inputs changed or verification expired")
    return before[2]


def choose_cores(physical, caches, busy, excluded):
    require(len(busy) == 64 and len(physical) == 32 and 0 <= excluded < 1 << 64,
            "unexpected Windows processor group or exclusions")
    forbidden = sum(mask for mask in physical if mask & excluded)
    cal_domains = sum(cache for cache in caches if cache & forbidden)
    candidates = []
    for mask in physical:
        if mask & forbidden:
            continue
        ids = [i for i in range(64) if mask & (1 << i)]
        require(len(ids) == 2, "expected two logical siblings per physical core")
        logical = min(ids, key=lambda i:(busy[i],i))
        candidates.append((bool(mask & cal_domains), sum(busy[i] for i in ids), logical, mask))
    require(len(candidates) >= 8, "eight disjoint eligible physical cores required")
    selected = sorted(candidates)[:8]
    return {"logical_cpus":[row[2] for row in selected],"physical_siblings":[row[3] for row in selected],
            "excluded_siblings":forbidden,"excluded_l3_domains":cal_domains}


def native_placement(excluded):
    affinity = runpy.run_path(str(ROOT/"scripts/arbiter/evaluation/selected_goal/cal_capture/timing/affinity.py"))
    first = affinity["load_counters"]()
    time.sleep(1)
    last = affinity["load_counters"]()
    busy = [1-(b[0]-a[0])/(b[1]-a[1]) if b[1]>a[1] else 1 for a,b in zip(first,last,strict=True)]
    return choose_cores(affinity["relation"](0),affinity["relation"](2),busy,excluded)


def live_tools(recipe):
    rows = [*recipe["compiler_tools"], *recipe["native_inputs"],
            *(item for item in recipe["tools"].values() if isinstance(item, dict))]
    return {Path(row["path"]):row for row in rows if row["path"].lower().endswith(".exe")}


def launch(args):
    import psutil
    from ctypes import byref, sizeof
    c, base, parent, owner, guard = runtime()
    _, recipe = checked_plan(args.sha256, base)
    require(not RUN.exists() and not OUT.exists(), "preserve existing build or attempted run")
    require(args.other_host_gib >= 0 and args.other_disk_gib >= 0, "negative concurrent reservation")
    host = runpy.run_path(str(ROOT/"scripts/cuda-component-execution/ballast_host_commit.py"))["host"]()
    available = min(host["physical_available_bytes"], host["commit_limit_bytes"] - host["commit_bytes"])
    require(available >= HOST_CAP + (args.other_host_gib+8)*GIB, "aggregate physical/commit headroom insufficient")
    free = shutil.disk_usage(ROOT).free
    reserved = args.other_disk_gib * GIB
    d["disk_admission"](free, DISK_CAP, reserved)
    start_utc = dt.datetime.now(dt.timezone.utc)
    deadline = time.monotonic() + (d["cutoff"](start_utc,start_utc) - start_utc).total_seconds()
    c["mkdir"](guard, RUN)
    scratch = RUN / "tmp"
    c["mkdir"](guard, scratch)
    env = dict(recipe["environment"])
    env.update(TEMP=str(scratch),TMP=str(scratch),TMPDIR=str(scratch),PSModuleAnalysisCachePath=str(scratch/"ps-cache"))
    argsv = [sys.executable,"-I","-B",str(Path(__file__).resolve()),"worker","--sha256",args.sha256]
    c["write_new"](guard,RUN/"admission.json",{"start":start_utc.isoformat(),"deadline":deadline,
                   "owner_pid":os.getpid(),"owner_created":psutil.Process().create_time(),
                   "resources":host,"other_host_gib":args.other_host_gib,"other_disk_gib":args.other_disk_gib,
                   "volume_free":free,"host_cap":HOST_CAP,"disk_reservation":DISK_CAP,"argv":argsv})
    placement = native_placement(args.exclude_mask)
    mask = sum(1 << i for i in placement["logical_cpus"])
    expected_tools = live_tools(recipe)
    py = base.pin(Path(sys.executable))
    require(py["sha256"] == recipe["tools"]["python"]["sha256"], "worker interpreter differs")
    limits_type = runpy.run_path(str(ROOT/"scripts/arbiter/analysis/public_source_admission/aws_normal_service.py"))["Limits"]
    def before(api,job,info,until):
        proof = parent.suspended_authentication(api, info, deadline, until, base.SID, py, owner.image_proof)
        require(c["token_owner"](info.process)==base.SID, "foreign default owner")
        limits = limits_type()
        def query():
            require(api.QueryInformationJobObject(job,9,byref(limits),sizeof(limits),None),"Job limits query failed")
        query()
        limits.basic.flags |= 0x20 | 0x10
        limits.basic.affinity = mask
        limits.basic.priority = 0x4000
        require(api.SetInformationJobObject(job,9,byref(limits),sizeof(limits)), "below-normal Job placement refused")
        query()
        require(limits.job == limits.process == HOST_CAP and limits.basic.priority==0x4000 and
                limits.basic.affinity == mask, "Job bounds differ")
        return dict(proof, host_cap=HOST_CAP, below_normal=True, placement=placement)

    peak = 0
    tools = {}
    def observe(api,job,info,until):
        nonlocal peak
        while api.WaitForSingleObject(info.process,1000)==258:
            require(time.monotonic()<until,"original CUDA deadline")
            available = shutil.disk_usage(ROOT).free
            growth = max(0,free-available)
            peak = max(peak,growth)
            require(growth < DISK_CAP - (256 << 20), "reserved D growth reached safety margin")
            d["disk_admission"](available,0,reserved)
            members,_,_ = owner.job_snapshot(api,job)
            for pid in members:
                try:
                    proc = psutil.Process(pid)
                    name = proc.name().lower()
                    if name not in {"cmake.exe","ninja.exe","cl.exe","nvcc.exe","link.exe","lib.exe","ptxas.exe"}:
                        continue
                    values = {k.upper():v for k,v in proc.environ().items()}
                    for key in ("TMP","TEMP","TMPDIR"):
                        require(Path(values[key]) == OUT/"tmp" or
                                Path(values[key]).is_relative_to(OUT/"gpu-aggregate"), "native child scratch outside D build")
                    require(Path(values["CUDA_CACHE_PATH"]) == OUT/"cuda-cache", "native CUDA cache outside D build")
                    image = Path(proc.exe()).resolve(strict=True)
                    require(image in expected_tools, "unreviewed native compiler image: "+str(image))
                    if name not in tools:
                        observed = base.pin(image)
                        require(observed["sha256"] == expected_tools[image]["sha256"], "native tool bytes differ")
                        tools[name] = observed
                    require(Path(tools[name]["path"]) == image, "compiler identity changed")
                except psutil.NoSuchProcess:
                    continue
        return {"observed_native_tools":tools,"sampled_max_volume_delta":peak},[]
    logfile, stdin = RUN/"worker.log", RUN/"stdin"
    c["guard_many"](guard,[logfile,stdin])
    result = {"passed":False}
    try:
        with stdin.open("xb"):
            pass
        with stdin.open("rb") as inp, logfile.open("xb") as out:
            result["job"] = owner.run_windows_owned_training(argsv,ROOT,out,deadline-time.monotonic(),env,inp,HOST_CAP,
                              before_resume=before,after_resume=observe,absolute_deadline=deadline)
        job = result["job"]
        require(job["exit_code"] == 0 and job["owned_descendants_settled"] and not job["timed_out"]
                and not job["owned_job_forced_termination"] and job["owned_job_final_active_processes"]==0
                and job["owned_job_final_member_pids"]==[], "CUDA build failed or did not close naturally")
        require({"cmake.exe","ninja.exe","cl.exe","nvcc.exe","lib.exe"} <= set(tools), "actual native tool observations missing")
        result["outputs"] = [base.pin(OUT / name) for name in ("fresh-configure.json","fresh-compile.json","fresh-aggregate.json")]
        checked_plan(args.sha256,base)
        result["worker_result"] = base.pin(RUN/"result.json")
        result["admission"] = base.pin(RUN/"admission.json")
        result["passed"] = True
    finally:
        result.update(binding_sha256=args.sha256,sampled_max_volume_delta=peak,observed_native_tools=tools)
        c["guard_many"](guard,[p for p in (stdin,logfile) if p.is_file()],True)
        c["write_new"](guard,RUN/"execution.json",result)
    return base.pin(RUN/"execution.json")


if __name__ == "__main__":
    parser = argparse.ArgumentParser(description=__doc__)
    sub = parser.add_subparsers(dest="mode",required=True)
    sub.add_parser("prepare")
    for name in ("worker","launch"):
        opts = sub.add_parser(name)
        opts.add_argument("--sha256",required=True)
        if name == "launch":
            opts.add_argument("--other-host-gib",type=int,required=True)
            opts.add_argument("--other-disk-gib",type=int,required=True)
            opts.add_argument("--exclude-mask",type=lambda v:int(v,0),required=True)
    args = parser.parse_args()
    if args.mode == "prepare":
        print(json.dumps(prepare()))
    elif args.mode == "worker":
        worker(args.sha256)
    else:
        print(json.dumps(launch(args)))
