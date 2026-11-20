# SPDX-License-Identifier: Apache-2.0
"""Exercise the compiled private Windows service resource-policy test in an isolated D: Job."""
import ctypes
import json
import os
from pathlib import Path
import runpy
import sys
import time


ROOT = Path(__file__).resolve().parents[4]
CAMPAIGN = ROOT / "artifacts/v190/final-plan/exec-20261116T052942Z"
SELECTOR = "configured_query_capacity_updates_live_worker_and_reads_through_matching_channels"
TEST = "target/debug/deps/service_runtime-642ed2a8f2fae56a.exe"
SERVICE = "target/debug/airk.exe"
PINS = {
    TEST: "70bf41765cec29e1faecc72531c8b357c8c14a2688e1767f6fda39c7f49e11cb",
    SERVICE: "5758f19e0290cdfa11cdbdd49fac8d436a0ea85c087c554db6d694b1bf3f0fcd",
    "crates/airk-cli/tests/service_runtime.rs": "2e1a9671b8aacb545e2e25c85898335f52e0e2742ea0f21e18bed82181a80b08",
    "target/debug/.fingerprint/airk-cli-642ed2a8f2fae56a/test-integration-test-service_runtime.json":
        "6d14d7cab92c689b41814e838c12eb5788f041d60bc7f6f716531762f9240411",
}


def require(ok, message):
    if not ok:
        raise ValueError(message)


CONHOST = Path("C:/Windows/System32/conhost.exe")
CONHOST_SHA = "dd5c3514132d92052004d302c66c756ffb6beef572de6538b4fc55f62579eb11"


def member_role(image):
    path = Path(image["path"])
    if image["sha256"] == PINS[SERVICE] and path.samefile(ROOT/SERVICE):
        return "service"
    if image["sha256"] == CONHOST_SHA and path.samefile(CONHOST):
        return "console_host"
    return "other"


def passed_run(owned, stable, log, sid):
    expected = {"exit_code":0,"timed_out":False,"log_oversized":False,"owned_descendants_settled":True,
                "owned_job_forced_termination":False,"owned_job_final_active_processes":0,"owned_job_final_member_pids":[],
                "parent_assigned_to_job":False,"child_assigned_before_resume":True,"owned_total_processes":3,
                "whole_process_deadline_not_refreshed":True}
    closure = owned["handle_close_proof"]
    observed = list(owned["before_GO"]["service_and_worker"].values())
    return (stable and all(owned[k] == v for k,v in expected.items()) and closure["settled"] and not closure["forced"]
            and closure["final_member_pids"] == [] and closure["final_active"] == 0 and len(observed) == 2
            and {member_role(member["image"]) for member in observed} == {"service","console_host"}
            and all(member["user_sid"] == member["default_owner_sid"] == sid for member in observed)
            and f"test {SELECTOR} ... ok" in log and "test result: ok. 1 passed; 0 failed;" in log)


def main(output, reconcile=False):
    c = runpy.run_path(str(ROOT/"scripts/cuda-component-execution.py"))
    b = c["base"]()
    parent, owner = b.helpers({"ownership_helper_root":str(b.MAINTAINED_HELPER_ROOT)})
    require(os.name == "nt" and sys.flags.isolated and sys.flags.no_site and sys.dont_write_bytecode
            and parent.worker.windows_sid() == c["token_owner"](ctypes.c_void_p(-1)) == b.SID,
            "original human owner and isolated Python required")
    require(output.parent == CAMPAIGN and not output.exists() and
            output.name.startswith("windows-native-resource-policy-private-r"), "fresh D: campaign output required")
    guard = {"checked_guard": b.pin(ROOT/"scripts/dev/assert-human-execution.ps1"),
             "powershell": b.pin(Path("C:/Program Files/PowerShell/7/pwsh.exe"))}
    c["guard_many"](guard,[ROOT/k for k in PINS],True)
    pins = {k:b.pin(ROOT/k) for k in PINS}
    require(all(pins[k]["sha256"] == expected for k,expected in PINS.items()), "pinned original image or source changed")
    c["mkdir"](guard,output)
    if reconcile:
        historical = CAMPAIGN/"windows-native-resource-policy-private-r3/execution.json"
        historic = b.pin(historical)
        require(historic["sha256"] == "7416bc0d4a10a0dc758b98605855e8b5fa344652630f843d2153bab8b449f508",
                "original native service receipt changed")
        prior = b.read(historic["path"])
        log = Path(prior["log"]["path"])
        require(log == historical.parent/"native.log" and b.pin(log) == prior["log"]
                and prior["command"] == [str(ROOT/TEST),"--exact",SELECTOR,"--nocapture","--test-threads=1"]
                and prior["inputs"] == pins and prior["post_inputs_unchanged"] and b.pin(CONHOST)["sha256"] == CONHOST_SHA,
                "original source, native command or console host changed")
        require(passed_run(prior["owned"],True,log.read_text(encoding="utf-8"),b.SID),"actual native service evidence refused")
        c["write_new"](guard,output/"execution.json",{"passed":True,"original":historic,"log":prior["log"],
                    "consumer":b.pin(Path(__file__).resolve()),"native_reruns":0,"member_roles":["service","console_host"],
                    "scope":prior["scope"],"installed_pair_exercised":False,"model_forward_requested":False})
        return
    temp = output/"temp"
    c["mkdir"](guard,temp)
    stdin, stdout = output/"stdin", output/"native.log"
    c["guard_many"](guard,[stdin,stdout])
    with stdin.open("xb"):
        pass
    command = [str(ROOT/TEST),"--exact",SELECTOR,"--nocapture","--test-threads=1"]
    environment = os.environ | {k:str(temp) for k in ("TMP","TEMP","TMPDIR","AIRK_NATIVE_FIXTURE_TEMP")}
    deadline = time.monotonic()+180
    def suspended(api,job,info,child_deadline):
        proof = parent.suspended_authentication(api,info,deadline,child_deadline,b.SID,pins[TEST],owner.image_proof)
        require(c["token_owner"](info.process) == b.SID and owner.job_snapshot(api,job) == ([info.pid],1,1),
                "unexpected suspended image owner or Job membership")
        return proof
    def observe(api,job,info,expiry):
        # Windows creates a console host in the same Job as the actual foreground service.
        from ctypes import wintypes
        api.OpenProcess.argtypes = [wintypes.DWORD,wintypes.BOOL,wintypes.DWORD]
        api.OpenProcess.restype = wintypes.HANDLE
        seen = {}
        while time.monotonic() < expiry:
            current,_,total = owner.job_snapshot(api,job)
            require(total <= 3,"unexpected additional process in private service fixture")
            for pid in current:
                if pid == info.pid or pid in seen:
                    continue
                handle = api.OpenProcess(0x1000,False,pid)
                require(handle,"could not authenticate native service child")
                try:
                    name = ctypes.create_unicode_buffer(32768)
                    size = wintypes.DWORD(len(name))
                    require(api.QueryFullProcessImageNameW(handle,0,name,ctypes.byref(size)),
                            "native service member image path unavailable")
                    actual = Path(name.value).resolve(strict=True)
                    image = b.pin(actual)
                    role = member_role(image)
                    if role == "service":
                        require(image["sha256"] == pins[SERVICE]["sha256"],"observed service bytes differ")
                    user,default = parent.worker.windows_sid(handle),c["token_owner"](handle)
                    require(user == default == b.SID,"foreign native service worker or default owner")
                    seen[pid] = {"image":image,"role":role,"user_sid":user,"default_owner_sid":default}
                finally:
                    api.CloseHandle(handle)
            if api.WaitForSingleObject(info.process,25) == 0:
                break
        return {"service_and_worker":seen}, []
    with stdin.open("rb") as source, stdout.open("xb") as sink:
        owned = owner.run_windows_owned_training(command,ROOT,sink,deadline-time.monotonic(),environment,
            source,6<<30,before_resume=suspended,after_resume=observe,absolute_deadline=deadline)
    c["guard_many"](guard,list(output.rglob("*")),True)
    log = stdout.read_text(encoding="utf-8",errors="strict")
    stable = all(b.pin(ROOT/k) == v for k,v in pins.items())
    passed = passed_run(owned,stable,log,b.SID)
    receipt = {"scope":"native private service resource setting","passed":passed,"command":command,
               "checked_source":b.pin(Path(__file__).resolve()),"inputs":pins,"post_inputs_unchanged":stable,
               "temp":str(temp),"owned":owned,"log":b.pin(stdout),"installed_pair_exercised":False,
               "model_forward_requested":False}
    c["write_new"](guard,output/"execution.json",receipt)
    require(passed,"original isolated native resource test failed; see retained output")


if __name__=="__main__":
    main(Path(sys.argv[1]),sys.argv[2:] == ["--reconcile-r3"])
