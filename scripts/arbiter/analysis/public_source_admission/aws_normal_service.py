# SPDX-License-Identifier: Apache-2.0
"""Run source discovery using the unchanged installed Windows AIRK pair on a disposable control root."""
import argparse
from contextlib import contextmanager
import ctypes
from ctypes import wintypes
import hashlib
import json
import os
from pathlib import Path
import runpy
import subprocess
import sys
import time

HERE = Path(__file__).resolve().parent
menu = runpy.run_path(str(HERE / "verify_aws_natural_menu.py"))
n = menu["native"]
require = n["require"]
stage = runpy.run_path(str(HERE / "aws_publication_setup.py"))
RECORD = Path("C:/Users/tdotn/AppData/Local/AIRK Service/program/install-record.json")
RECORD_SHA = "fd5fb19af6de75825aa9fa0a0cd9867802b17ea42988296138b10631b5baf48a"
EXPANDED = "b34960a9d776ed0aeb645954fe877529989a433da8238799ff647489736c24e9"
WALL = {"pilot": 240, "scaling": 600, "full": 4 * 3600}
# Both values are process and whole-job ceilings. Full execution requires separate resource admission.
HOST = {"pilot": 4 << 30, "scaling": 2 << 30, "full": 6 << 30}


class Basic(ctypes.Structure):
    _fields_ = [("process_time", ctypes.c_int64), ("job_time", ctypes.c_int64),
                ("flags", wintypes.DWORD), ("working_min", ctypes.c_size_t), ("working_max", ctypes.c_size_t),
                ("active_limit", wintypes.DWORD), ("affinity", ctypes.c_size_t),
                ("priority", wintypes.DWORD), ("scheduling", wintypes.DWORD)]


class Limits(ctypes.Structure):
    _fields_ = [("basic", Basic), ("io", ctypes.c_uint64 * 6), ("process", ctypes.c_size_t),
                ("job", ctypes.c_size_t), ("peak_process", ctypes.c_size_t), ("peak_job", ctypes.c_size_t)]


@contextmanager
def capped(w, output, maximum):
    api = ctypes.WinDLL("kernel32", use_last_error=True)
    ptr = ctypes.c_void_p
    specs = {"CreateJobObjectW": (ptr, [ptr, ptr]), "GetCurrentProcess": (ptr, []),
             "GetProcessAffinityMask": (wintypes.BOOL, [ptr, ptr, ptr]),
             "AssignProcessToJobObject": (wintypes.BOOL, [ptr, ptr]),
             "SetInformationJobObject": (wintypes.BOOL, [ptr, wintypes.DWORD, ptr, wintypes.DWORD]),
             "QueryInformationJobObject": (wintypes.BOOL, [ptr, wintypes.DWORD, ptr, wintypes.DWORD, ptr]),
             "CloseHandle": (wintypes.BOOL, [ptr]), "OpenProcess": (ptr, [wintypes.DWORD, wintypes.BOOL, wintypes.DWORD]),
             "IsProcessInJob": (wintypes.BOOL, [ptr, ptr, ptr])}
    for key, (result, arguments) in specs.items():
        fn = getattr(api, key)
        fn.restype, fn.argtypes = result, arguments
    def checked(result):
        if not result:
            raise ctypes.WinError(ctypes.get_last_error())
        return result
    proc = api.GetCurrentProcess()
    process_mask, system_mask = ctypes.c_size_t(), ctypes.c_size_t()
    checked(api.GetProcessAffinityMask(proc, ctypes.byref(process_mask), ctypes.byref(system_mask)))
    bits = [1 << bit for bit in range(64) if process_mask.value & (1 << bit)][:4]
    require(len(bits) == 4, "four allowed CPUs required")
    quota = Limits()
    quota.basic.flags = 0x100 | 0x200 | 0x10 | 0x20
    quota.basic.affinity = sum(bits)
    quota.basic.priority = 0x4000
    quota.process = quota.job = maximum
    handle = checked(api.CreateJobObjectW(None, None))
    try:
        checked(api.SetInformationJobObject(handle, 9, ctypes.byref(quota), ctypes.sizeof(quota)))
        checked(api.AssignProcessToJobObject(handle, proc))
        try:
            def member(pid):
                process = checked(api.OpenProcess(0x1000, False, pid))
                try:
                    contained = wintypes.BOOL()
                    checked(api.IsProcessInJob(process, handle, ctypes.byref(contained)))
                    require(contained.value, "service escaped source Job")
                finally:
                    api.CloseHandle(process)
            def sample():
                actual = Limits()
                checked(api.QueryInformationJobObject(handle, 9, ctypes.byref(actual), ctypes.sizeof(actual), None))
                return {"peak_job_bytes": actual.peak_job, "peak_process_bytes": actual.peak_process}
            yield member, sample
        finally:
            actual = Limits()
            checked(api.QueryInformationJobObject(handle, 9, ctypes.byref(actual), ctypes.sizeof(actual), None))
            require(actual.job == actual.process == maximum and actual.basic.affinity == sum(bits)
                    and actual.basic.priority == 0x4000, "native source Job limits differ")
            w["write_json"](output / "limits.json", {"host_cap": maximum, "peak_job_bytes": actual.peak_job,
                        "peak_process_bytes": actual.peak_process, "cpu_mask": sum(bits), "below_normal": True})
    finally:
        api.CloseHandle(handle)


def images():
    n["checked"](RECORD, RECORD_SHA)
    record = json.loads(RECORD.read_bytes())
    rows = record["executables"]
    paths = {}
    for name in ("airk", "airkctl"):
        path = RECORD.parent / rows[name]["path"]
        n["checked"](path, rows[name]["sha256"])
        require(path.stat().st_size == rows[name]["bytes"], "installed image size changed")
        paths[name] = path
    return paths, rows


def execute(w, owner, args, output, deadline, member, sample):
    paths, pins = images()
    runs = output / "commands"
    runs.mkdir()
    control = output / "control"
    index_store = output / "index"
    index_store.mkdir()
    env = {k: v for k, v in os.environ.items() if not k.upper().startswith(("AIRK_", "RUST_LOG", "PYTHON"))}
    n["checked"](n["CAMPAIGN"] / "aws-expanded-source-verification-r1.private.json", EXPANDED)
    inventory = n["CAMPAIGN"] / menu["INVENTORY"]
    n["checked"](inventory, menu["INVENTORY_SHA"])
    rootname, cases, members = menu["inputs"](json.loads(inventory.read_bytes()))
    frozen = n["CAMPAIGN"] / rootname
    if args.mode == "pilot":
        # This isolated one-document smoke test makes no full-menu claim.
        first = sorted(path for path in members if path.endswith(".md"))[0]
        expected = members[first]["sha256"]
        tree = output / "pilot-source"
        tree.mkdir()
        body = (frozen / first).read_bytes()
        require(hashlib.sha256(body).hexdigest() == expected, "pilot source differs")
        (tree / "source.md").write_bytes(body)
    elif args.mode == "scaling":
        family = "amazon-pinpoint-developer-guide"
        tree = frozen / family
        subset = {path[len(family) + 1:]: row for path, row in members.items() if path.startswith(family + "/")}
        relative_paths = {p.relative_to(tree).as_posix() for p in tree.rglob("*") if p.is_file()}
        require(relative_paths == subset.keys() and len(relative_paths) == 282, "incomplete original scaling family")
        for path, row in subset.items():
            n["checked"](owner.plain(tree / path, regular=True), row["sha256"])
        require(sum((tree / path).stat().st_size for path in relative_paths) == 5355781, "unexpected source length")
        cases = [{"selector": {"family": family}, "cases": [], "members": subset}]
    else:
        tree, cases = stage["source_closure"](w, owner, menu)
    def command(label, *args, closing=False):
        return w["run_logged"]([paths["airkctl"], "--output", "json", "--service-runtime-dir", control, *args],
                               runs, label, env, deadline - (0 if closing else 25))[0]
    def cli(label, *args):
        return w["read_json"](command(label, *args))
    def tool(label, name, index, payload):
        return w["tool_body"](n["publication_read"](cli, w["read_json"], runs, label,
            ["tools", "call", name, "--index", index, "--args", json.dumps(payload)], deadline - 25))
    def start(label):
        result = cli(label, "service", "start", "--runtime-dir", control)
        try:
            require(result["status"] == "health", "native service health unavailable")
            health = result["observation"]
            require(health["executable_sha256"] == pins["airk"]["sha256"] and health["process_id"] > 0
                    and health["user"] == "S-1-5-21-3678370870-3858642417-826968105-1002",
                    "wrong admitted service process")
            member(health["process_id"])
            return health
        except Exception:
            stop(label + "-unaccepted-stop")
            raise
    def stop(label):
        command(label, "service", "stop", "--runtime-dir", control, closing=True)
    def wait(label, job):
        for i in range(WALL[args.mode]):
            result = cli(f"{label}-{i}", "jobs", "get", job["id"])["job"]
            if args.mode == "full" and i % 12 == 0:
                w["write_json"](runs / f"{label}-resource-{i}.json",
                                {"time_unix": time.time(), "native_state": result["state"], **sample()})
            require((result["id"], result["kind"]) == (job["id"], job["kind"]), "job identity changed")
            if result["state"] == "succeeded":
                require(result["outcome"]["state"] == "succeeded", "job did not succeed")
                return result
            require(result["state"] in ("pending", "running"), "source job failed")
            time.sleep(min(5, max(0, deadline - time.monotonic() - 25)))
        raise TimeoutError("source job not observed within deadline")
    started = start("start")
    try:
        result = command("register", "--root", tree, "--data-dir", index_store, "library", "setup",
                         "--name", "aws-source-discovery", "--watch", "off", "--apply")
        with result.open("rb") as stream:
            draft, applied = map(json.loads, stream)
        require("draft" in draft, "setup draft missing")
        reg = applied["receipt"]
        project, index = reg["request"]["registration"]["binding"], reg["index_id"]
        if args.mode == "full":
            quota = output / "native-store-quota.json"
            draft_file = output / "configure-quota.json"
            w["write_json"](quota, {"root_bytes": 8 << 30, "default_collection_bytes": 8 << 30, "collections": {}})
            w["write_json"](draft_file, {"operation": "quota_policy", "path": str(quota)})
            wait("quota-job", cli("set-store-quota", "--service-project", project, "configure",
                                  "--file", draft_file)["job"])
        off = output / "source-policy.json"
        w["write_json"](off, {"operation": "vectors", "enabled": False})
        wait("policy-job", cli("configure", "--service-project", project, "configure", "--file", off)["job"])
        job = wait("index-job", cli("index", "--service-project", project, "index")["job"])
        before = tool("before-restart", "airk_status", index, {"response_mode": "full", "include_status": True})
        token = before["publication"]["token"]
        require(before["discovery_readiness"]["state"] == "ready" and before["semantic_readiness"]["state"] == "disabled"
                and job["outcome"]["revision"] == token, "native source-only publication not ready")
    finally:
        stop("stop")
    reopened = start("restart")
    try:
        require(reopened["instance"] != started["instance"]
                and reopened["catalog_clock"]["namespace"] == started["catalog_clock"]["namespace"],
                "native catalog failed to reopen in a new process")
        # A fresh service incarnation issues a new in-memory grant. Pin the
        # second incarnation's complete scope for the entire census.
        after = tool("reopened-status", "airk_status", index, {"response_mode": "full", "include_status": True})
        old_scope = before["scope_observation"]
        new_scope = after["scope_observation"]
        require(after["publication"]["token"] == token and after["files"] == before["files"]
                and after["discovery_readiness"] == before["discovery_readiness"]
                and after["semantic_readiness"] == before["semantic_readiness"]
                and {k: v for k, v in old_scope.items() if k != "fingerprint"} ==
                    {k: v for k, v in new_scope.items() if k != "fingerprint"}, "durable scope changed")
        def same(label, name, payload):
            row = tool(label, name, index, payload)
            require(row["publication"]["token"] == token and row["scope_observation"] == new_scope,
                    "source publication changed across restart")
            return row
        if args.mode == "pilot":
            visible = same("pilot-list", "airk_list_files", {"prefix": "", "limit": 100, "response_mode": "full"})
            info = same("pilot-info", "airk_file_info", {"path": "source.md", "response_mode": "full"})
            require(visible["paths"] == ["source.md"] and not visible["truncated"] and info["content_hash"] == expected,
                    "pilot source failed original hash/native visibility")
            observations = {"files": 1, "original_source_path": first, "sha256": expected}
        else:
            observations = menu["observe"](menu["COHORT"], cases, tree, same)
    finally:
        stop("final-stop")
    images()
    w["write_json"](output / "source.private.json",
        {"mode": args.mode, "installed_record": RECORD_SHA, "executables": pins, "source_root": str(tree),
         "index": index, "registration": reg, "publication": token, "status": before, "observations": observations,
         "before": started, "reopened": reopened, "reopened_status": after, "scored_requests": 0, "ranker_observed": False})


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("--output", type=Path, required=True)
    parser.add_argument("--mode", choices=WALL, required=True)
    parser.add_argument("--worker", action="store_true")
    args = parser.parse_args()
    n["checked"](n["WRAPPER"], n["WRAPPER_SHA"])
    w = runpy.run_path(str(n["WRAPPER"]))
    owner = w["owner_module"]()
    output = owner.plain(args.output)
    require(output.parent == n["CAMPAIGN"] and output.name.startswith("aws-normal-" + args.mode + "-"),
            "D campaign path required")
    if args.worker:
        require(os.environ["AIRK_AWS_NORMAL_OWNER"] == str(output), "owned worker required")
        with capped(w, output, HOST[args.mode]) as custody:
            execute(w, owner, args, output, float(os.environ["AIRK_AWS_NORMAL_DEADLINE"]), *custody)
        return
    require(not output.exists(), "fresh private root required")
    output.mkdir()
    expiry = time.monotonic() + WALL[args.mode]
    result = owner.run_evaluation_fixture([sys.executable, "-I", "-B", "-S", __file__, *sys.argv[1:], "--worker"],
        n["ROOT"], output / "outer.log", WALL[args.mode],
        os.environ | {"AIRK_AWS_NORMAL_OWNER": str(output), "AIRK_AWS_NORMAL_DEADLINE": str(expiry)})
    w["write_json"](output / "owned.json", result)
    stage["require_settlement"](w, result)
    print(owner.digest(output / "source.private.json"))


if __name__ == "__main__":
    main()
