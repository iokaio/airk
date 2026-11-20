# SPDX-License-Identifier: Apache-2.0
"""Measure Windows topology and apply one predeclared mask to the owned timing service."""
import argparse
import ctypes as C
from ctypes import wintypes as W
import hashlib
import json
import os
import subprocess
from pathlib import Path
import runpy
import struct
import sys
import time

HERE = Path(__file__).resolve().parent
ROOT = HERE.parents[5]
CAMPAIGN = ROOT / "artifacts/v190/final-plan/exec-20261116T052942Z"
SID = "S-1-5-21-3678370870-3858642417-826968105-1002"
FULL = (1 << 64) - 1
ORDER = (("full","local4","local4","full"),("local4","full","full","local4"))


def require(value, reason):
    if not value:
        raise ValueError(reason)


def sha(path):
    with Path(path).open("rb") as src:
        return hashlib.file_digest(src,"sha256").hexdigest()


def api(name, result, *args):
    fn = getattr(C.WinDLL("kernel32", use_last_error=True),name)
    fn.argtypes,fn.restype = list(args), result
    return fn


def checked(success):
    if not success:
        raise C.WinError(C.get_last_error())
    return success


def relation(kind):
    fn = api("GetLogicalProcessorInformationEx",W.BOOL,W.DWORD,C.c_void_p,C.POINTER(W.DWORD))
    size = W.DWORD()
    fn(kind,None,C.byref(size))
    require(size.value > 0,"unavailable CPU topology")
    data = C.create_string_buffer(size.value)
    checked(fn(kind,data,C.byref(size)))
    pos, masks = 0,[]
    while pos < size.value:
        rel, length = struct.unpack_from("<II",data,pos)
        require(rel == kind and length > 8 and pos+length <= size.value,"invalid Windows topology record")
        if kind == 0:
            require(length >= 48 and struct.unpack_from("<H",data,pos+30)[0] == 1,"multiple processor groups")
            mask, group = struct.unpack_from("<QH",data,pos+32)
            require(group == 0 and mask != 0,"invalid physical core")
            masks.append(mask)
        elif data.raw[pos+8] == 3:
            require(length >= 56,"missing L3 cache mask")
            mask,group = struct.unpack_from("<QH",data,pos+40)
            require(group == 0 and mask != 0,"invalid last-level cache")
            masks.append(mask)
        pos += length
    return masks


def load_counters():
    class Cpu(C.Structure):
        _fields_ = [(k,C.c_int64) for k in ("idle","kernel","user","dpc","interrupt")] + [("count",W.ULONG)]
    info = C.WinDLL("ntdll").NtQuerySystemInformation
    info.argtypes,info.restype = [W.ULONG,C.c_void_p,W.ULONG,C.POINTER(W.ULONG)],W.LONG
    counters,length = (Cpu*64)(),W.ULONG()
    require(info(8,counters,C.sizeof(counters),C.byref(length)) == 0 and length.value == C.sizeof(counters),
            "expected one group of 64 logical CPUs")
    return [(x.idle,x.kernel+x.user) for x in counters]


def choose(cores,caches,busy,excluded=0):
    require(len(cores) == 32 and len(busy)==64 and sum(cores) == FULL,"unexpected Threadripper physical topology")
    options = []
    for cache in caches:
        local = []
        for core in cores:
            if core & cache == core and not core & excluded:
                members = [i for i in range(64) if core & (1 << i)]
                logical = min(members,key=lambda i:(busy[i],i))
                local.append((sum(busy[i] for i in members),logical,core))
        if len(local) >= 4:
            selected = sorted(local)[:4]
            options.append((sum(row[0] for row in selected),cache,selected))
    require(options,"no four available physical cores share one L3 cache")
    _,cache,selected = min(options)
    workers = [r[1] for r in selected]
    return {"full_mask_hex":hex(FULL),"local_mask_hex":hex(sum(1<<i for i in workers)),
            "logical_cpus":workers,"physical_sibling_masks":[hex(r[2]) for r in selected],
            "cache_mask_hex":hex(cache),"all_physical_masks":[hex(c) for c in cores],
            "all_cache_masks":[hex(c) for c in caches],"busy_fraction":busy,"excluded_mask_hex":hex(excluded)}


def sample(excluded):
    cores,caches = relation(0),relation(2)
    before = load_counters()
    time.sleep(1)
    after = load_counters()
    busy = [1-(end[0]-start[0])/(end[1]-start[1]) if end[1]>start[1] else 1.
            for start,end in zip(before,after,strict=True)]
    return choose(cores,caches,busy,excluded)


def background_sample():
    shell = Path(os.environ["ProgramFiles"])/"PowerShell/7/pwsh.exe"
    cmd = ("$ErrorActionPreference='Stop';$names=@('python','cargo','rustc','cl','link','ninja','node','vmmemWSL');"
           "$rows=@(Get-Process | Where-Object { $_.Name -in $names -or $_.Name -like 'fresh-*' } | "
           "Select-Object Id,Name,CPU,PrivateMemorySize64);ConvertTo-Json -InputObject $rows -Compress")
    data = subprocess.run([str(shell),"-NoProfile","-NonInteractive","-Command",cmd],capture_output=True,check=True,
                          timeout=10,creationflags=subprocess.CREATE_NO_WINDOW)
    return {"host_counters":load_counters(),"known_worker_processes":json.loads(data.stdout),"unix_ns":time.time_ns()}


def apply(args):
    plan_path = args.plan.resolve(strict=True)
    out = plan_path.parent
    require(out.parent == CAMPAIGN and out.name.startswith("cal100-focus1024-timing-ab-r") and
            plan_path == args.plan and plan_path.name=="plan.json" and sha(plan_path)==args.plan_sha256,
            "wrong diagnostic plan or location")
    plan = json.loads(plan_path.read_bytes())
    require(plan["schema"]=="airk.private.cal-phase12-affinity-timing/1" and plan["output"]==str(out) and
            plan["affinity_helper"]["sha256"] == sha(__file__) and args.pass_index in (0,1) and
            96 <= args.ordinal <= 99 and ORDER[args.pass_index][args.ordinal-96] == args.arm,
            "different case, pass or scheduling plan")
    ready = json.loads((out/"fixture/ready.json").read_bytes())
    service = plan["service"]
    require(args.pid==ready["health"]["process_id"] and ready["health"]["executable_sha256"]==service["sha256"],
            "foreign PID or native image")
    handle = checked(api("OpenProcess",W.HANDLE,W.DWORD,W.BOOL,W.DWORD)(0x1200,False,args.pid))
    try:
        image,length = C.create_unicode_buffer(32768),W.DWORD(32768)
        checked(api("QueryFullProcessImageNameW",W.BOOL,W.HANDLE,W.DWORD,W.LPWSTR,C.POINTER(W.DWORD))(
            handle,0,image,C.byref(length)))
        require(Path(image.value) == Path(service["path"]) and Path(image.value).is_file(),"different live executable")
        conversion = runpy.run_path(str(ROOT/"scripts/arbiter-fp16-execution.py"))
        own = conversion["windows_owner"]()
        require(own["user_sid"]==own["owner_sid"]==SID and conversion["windows_owner"](process=handle)==own,
                "foreign native owner")
        job = W.BOOL()
        checked(api("IsProcessInJob",W.BOOL,W.HANDLE,W.HANDLE,C.POINTER(W.BOOL))(handle,None,C.byref(job)))
        require(bool(job.value),"live native timing process outside owned Job")
        getmask = api("GetProcessAffinityMask",W.BOOL,W.HANDLE,C.POINTER(C.c_size_t),C.POINTER(C.c_size_t))
        previous,system = C.c_size_t(),C.c_size_t()
        checked(getmask(handle,C.byref(previous),C.byref(system)))
        full = int(plan["topology"]["full_mask_hex"],16)
        local = int(plan["topology"]["local_mask_hex"],16)
        require(full==system.value==FULL and previous.value in (full,local) and local.bit_count()==4,
                "unexpected original scheduling mask")
        target = full if args.arm == "full" else local
        checked(api("SetProcessAffinityMask",W.BOOL,W.HANDLE,C.c_size_t)(handle,target))
        observed = C.c_size_t()
        checked(getmask(handle,C.byref(observed),C.byref(system)))
        require(observed.value==target,"Windows did not apply the preregistered core mask")
        return {"success":True,"pid":args.pid,"pass_index":args.pass_index,"ordinal":args.ordinal,"arm":args.arm,
                "full_mask_hex":hex(full),"previous_mask_hex":hex(previous.value),"observed_mask_hex":hex(observed.value),
                "service_instance":ready["health"]["instance"],"background":background_sample()}
    finally:
        checked(api("CloseHandle",W.BOOL,W.HANDLE)(handle))


if __name__ == "__main__":
    parser=argparse.ArgumentParser(description=__doc__)
    parser.add_argument("--plan",type=Path,required=True)
    parser.add_argument("--plan-sha256",required=True)
    parser.add_argument("--pid",type=int,required=True)
    parser.add_argument("--pass-index",type=int,required=True)
    parser.add_argument("--ordinal",type=int,required=True)
    parser.add_argument("--arm",choices=("full","local4"),required=True)
    args=parser.parse_args()
    require(os.name=="nt" and sys.flags.isolated and sys.flags.no_site and sys.dont_write_bytecode,"checked Windows Python required")
    print(json.dumps(apply(args),allow_nan=False))
