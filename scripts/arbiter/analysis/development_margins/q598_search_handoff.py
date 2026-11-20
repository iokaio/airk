# SPDX-License-Identifier: Apache-2.0
"""Resume Q598 from the closed revision-13 Search owner and observe startup warm before setup."""
import ctypes
from ctypes import wintypes
import json
import msvcrt
import os
from pathlib import Path

OUTER = "056fe3e1c3103a60c1b2e88303ef491e89f059fbfbba539ae6a377768e0156fe"
STATUS = "30dacd28b6d15f240791d29fdbf42734be68dc9bdb85674780bb2d4d3298b84d"


def predecessor(reuse, prior, digest, base):
    e, read, require = reuse["E"], reuse["read"], reuse["require"]
    root = e / "cal100-search-mode-timing-r1"
    require(prior == root/"outer-owned.json" and digest == OUTER, "foreign Search predecessor")
    original = read(e/"compact-development-TRAIN_Q598-focus1024-run-r1/owned.json",
                    "6866fe743068d17a73d36022ac1fe82f85bbcea5467b301da2a12dfd89447cd8")
    require(original["exit_code"] == 1 and reuse["settled"](original) and
            not any((e/"compact-development-TRAIN_Q598-focus1024-run-r1/commands").glob("answer*")),
            "original Q598 outcome changed")
    recovery = read(e/"train-q598-closed-control-recovery-r2/owned.json",
                    "e07b84a680a7d4c53b92ae91e02de2feed727566b78e87de0abaa14558b752c8")
    require(recovery["exit_code"] == 0 and reuse["settled"](recovery), "Q598 recovery did not close")
    outer = read(prior, digest)
    release = read(root/"control-mutex-released.json","55de694ebd4dc114c96893098d8985f2697fc7a88cbcfb6b22ce8db71a13401b")
    custody = read(root.with_name(root.name+"-custody.json"),"bc13a8ec0926d7046c9f706b2f9f381ea54ad5d065cf0a1b6607b824c7baabd3")
    require(reuse["settled"](outer) and outer["exit_code"] == 0 and not outer["timed_out"] and
            release["outer"]["sha256"] == digest and custody["exit_code"] == 0 and not custody["source_drift"],
            "Search job, mutex or custody changed")
    ready=read(root/"fixture/ready.json","96076effdb35d0d717234af9451e3dbbe2e664cf891fd603aec66719aa3f1f19")
    closed=read(root/"fixture/closed.json","36ff78713d3b7d50221bf2cee559d0e5064fd39f0fcdea5fc7265d68f055d143")
    status=read(root/"commands/stage-after.stdout",STATUS)
    require(Path(ready["control_root"]).samefile(reuse["CONTROL"]) and Path(closed["control_root"]).samefile(reuse["CONTROL"])
            and closed["instance"] == ready["health"]["instance"] and closed["shutdown_completed"] and closed["service_lease_reacquired"]
            and status["configuration"]["revision"] == 13 and all(status[k]["running"] == status[k]["queued"] == 0 for k in ("focus","context")),
            "wrong Search service, revision or native lease")
    after = reuse["linked"](outer["control_after"])
    configuration = reuse["CONTROL"] / "arbiter-v3/configuration.json"
    require(read(configuration,after["files"]["arbiter-v3/configuration.json"]) == status["configuration"],
            "Search stage after-state differs from current physical installation")
    return {k: {f:p[f] for f in ("path","sha256")} for k,p in [
        ("ready",base["ready"]),("closed",base["closed"]),("status",reuse["link"](root/"commands/stage-after.stdout"))]}


def published(path):
    """Read the native handoff only after the original producer releases its writer."""
    api = ctypes.WinDLL("kernel32",use_last_error=True)
    open_file = api.CreateFileW
    open_file.argtypes = [wintypes.LPCWSTR,wintypes.DWORD,wintypes.DWORD,wintypes.LPVOID,
                          wintypes.DWORD,wintypes.DWORD,wintypes.HANDLE]
    open_file.restype = wintypes.HANDLE
    # Deny concurrent writers while consuming the complete JSON publication.
    handle=open_file(str(path),0x80000000,1,None,3,0x80,None)
    if handle == ctypes.c_void_p(-1).value:
        error=ctypes.get_last_error()
        if error in (2,32,33): return None
        raise ctypes.WinError(error)
    try:
        fd=msvcrt.open_osfhandle(handle,os.O_RDONLY|os.O_BINARY)
    except BaseException:
        api.CloseHandle.argtypes=[wintypes.HANDLE]
        api.CloseHandle(handle)
        raise
    with os.fdopen(fd,"rb") as file:
        return json.load(file)


def before_setup(native, output, restart, control):
    # The original executor injects its native observation API. Extend only its
    # first readiness wait, before it can begin source setup or configure stages.
    wait = native["wait_observed"]
    used = False

    def observed(folder, callback, process, deadline, label, **kwargs):
        nonlocal used
        result = wait(folder,callback,process,deadline,label,**kwargs)
        if label != "pilot observer readiness":
            return result
        if used or folder != output:
            raise ValueError("unexpected second or foreign setup")
        used = True
        work = folder/"fixture"
        def load(path):
            return json.loads(path.read_bytes())
        ready=wait(work,lambda:published(work/"ready.json"),process,deadline,
                   "Q598 completed native readiness publication",file_changes=0x19)
        anchor=load(Path(restart["ready"]["path"]))
        config=load(Path(restart["status"]["path"]))["configuration"]
        saved=wait(work,lambda:published(work/"restart-owner.json"),process,deadline,
                   "Q598 completed original restart publication",file_changes=0x19)
        if not (ready["health"]["process_id"] == process.pid and ready["health"]["instance"] != anchor["health"]["instance"]
                and ready["health"]["executable_sha256"] == anchor["health"]["executable_sha256"] and ready["profiles"] == anchor["profiles"]
                and Path(ready["control_root"]).samefile(control) and saved["configuration"] == config
                and Path(saved["control"]).samefile(control) and not (work/"control").exists()):
            raise ValueError("actual restored setup differs")
        expected={k:config[k]["selected_profile"] for k in ("focus","context") if config[k]["enabled"]}
        def warm():
            path=work/"warm-completions.jsonl"
            if not path.exists(): return None
            if path.stat().st_size > 8<<20: raise ValueError("warm observer exceeded bound")
            raw=path.read_bytes()
            if not raw.endswith(b"\n"): return None
            rows=[json.loads(line) for line in raw.splitlines()]
            stages=[r["stage"] for r in rows]
            if len(set(stages)) != len(stages) or any(r["observer_id"] != ready["warm_observer_id"] or
                    r["accepted_configure_revision"] != 0 or r["profile_manifest"] != expected.get(r["stage"])
                    or r["outcome"] != "ready" for r in rows):
                raise ValueError("foreign or failed original rehydration")
            return rows if set(stages)==set(expected) else None
        rows=wait(work,warm,process,deadline,"Q598 original enabled stage rehydration",file_changes=0x18) if expected else []
        native["write_json"](output/"startup-warm-before-setup.json",
                             {"instance":ready["health"]["instance"],"configuration_revision":config["revision"],"completions":rows})
        return result
    def logged(argv, logs, label, env, deadline, *args, **kwargs):
        if label == "shutdown" and not (logs/"reuse-final-status.stdout").is_file():
            if list(argv[-2:]) != ["service","shutdown"] or not Path(argv[4]).samefile(control):
                raise ValueError("foreign shutdown command")
            try:
                native["run_logged"]([*argv[:-2],"arbiter","status-legacy"], logs,
                                     "failure-stage-at-shutdown", env, deadline)
            finally:
                shutdown = native["run_logged"](argv,logs,label,env,deadline,*args,**kwargs)
            return shutdown
        return native["run_logged"](argv,logs,label,env,deadline,*args,**kwargs)
    return dict(native,wait_observed=observed,run_logged=logged)
