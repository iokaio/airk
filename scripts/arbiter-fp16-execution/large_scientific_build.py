# SPDX-License-Identifier: Apache-2.0
"""Run one reviewed Large Direct native Cargo phase inside a four-GiB Windows Job."""
import argparse
import hashlib
import json
import os
from pathlib import Path
import runpy
import shutil
import subprocess
import sys
import time

HERE = Path(__file__).resolve()
ROOT = HERE.parents[2]
CAMPAIGN = ROOT / "artifacts/v190/final-plan/exec-20261116T052942Z"
PREVIOUS = CAMPAIGN / "windows-large-direct-fp32-scientific-checks-r1"
TEMPLATES = {
    "clippy":"6db9d1f1b2f79593b67e42bf9e06e4d9d45ce0ae9f2a7744fc2408525385fbc0",
    "nextest":"738fa8baed421605a3f25288934db5eb823f24d6e5206eb8f0afd68d2df302bd",
    "image":"77e0ffd00e47e1ac024210b8a8b099dd3fb95c598bc461e20701f59277b5678c",
}
HOST = 4 << 30


def require(condition, reason):
    if not condition:
        raise ValueError(reason)


def await_tool_exit(expires, expected_child):
    """Wait for the original nested Job's compiler helpers under the existing deadline."""
    import ctypes
    from ctypes import wintypes
    api = ctypes.WinDLL("kernel32", use_last_error=True)
    query = api.QueryInformationJobObject
    query.argtypes = (wintypes.HANDLE, ctypes.c_int, ctypes.c_void_p, wintypes.DWORD, ctypes.POINTER(wintypes.DWORD))
    open_process = api.OpenProcess
    open_process.argtypes = (wintypes.DWORD, wintypes.BOOL, wintypes.DWORD)
    open_process.restype = wintypes.HANDLE
    image = api.QueryFullProcessImageNameW
    image.argtypes = (wintypes.HANDLE, wintypes.DWORD, wintypes.LPWSTR, ctypes.POINTER(wintypes.DWORD))
    close = api.CloseHandle
    close.argtypes = (wintypes.HANDLE,)
    wait = api.WaitForSingleObject
    wait.argtypes = (wintypes.HANDLE, wintypes.DWORD)
    console = Path(os.environ["SystemRoot"]) / "System32/conhost.exe"
    observed = {}
    while time.monotonic() < expires:
        data = ctypes.create_string_buffer(8 + 256 * ctypes.sizeof(ctypes.c_size_t))
        returned = wintypes.DWORD()
        if not query(None, 3, data, len(data), ctypes.byref(returned)):
            raise ctypes.WinError(ctypes.get_last_error())
        count = int.from_bytes(data.raw[4:8], "little")
        require(count <= 256 and returned.value >= 8 + count * ctypes.sizeof(ctypes.c_size_t), "truncated original Job member census")
        pids = list((ctypes.c_size_t * count).from_buffer(data, 8))
        require(os.getpid() in pids, "current worker is outside its reported Job")
        pending = False
        for pid in pids:
            if pid == os.getpid():
                continue
            handle = open_process(0x1000 | 0x100000, False, pid)
            if not handle:
                if ctypes.get_last_error() == 87:
                    continue
                raise ctypes.WinError(ctypes.get_last_error())
            try:
                name, length = ctypes.create_unicode_buffer(32768), wintypes.DWORD(32768)
                if not image(handle, 0, name, ctypes.byref(length)):
                    raise ctypes.WinError(ctypes.get_last_error())
                observed[pid] = name.value
                if Path(name.value) == console:
                    continue
                pending = True
                require(wait(handle, min(500, max(0, int((expires - time.monotonic()) * 1000)))) in (0, 258),
                        "native helper wait failed")
            finally:
                close(handle)
        if not pending:
            return {"original_cargo_child": expected_child, "observed_members": observed, "remaining_job_members":pids}
    raise TimeoutError("native helper still active at original deadline")


def phase_argv(phase, directory):
    template = PREVIOUS / f"large-{phase}-r1.json"
    raw = template.read_bytes()
    require(hashlib.sha256(raw).hexdigest() == TEMPLATES[phase], "original reviewed Cargo template differs")
    command = json.loads(raw)["command"]
    if phase == "clippy":
        require(Path(command[0]).resolve() == Path(sys.executable).resolve() and command[1:5] == ["-I","-B","-S","scripts/dev-check.py"]
                and "--offline" in command and Path(command[command.index("--cargo")+1]) == Path(os.environ["CARGO"]),
                "original locked Clippy wrapper/pinned Cargo required")
    else:
        require(Path(command[0]) == Path(os.environ["CARGO"]) and "--locked" in command and "--offline" in command
                and "--timings" in command, "original pinned Cargo/lock/offline/timings required")
    if phase == "nextest":
        require(command[-2] == "-E" and "large_kv_expansion_preserves_four_consecutive_heads" in command[-1],
                "wrong original Large regression roster")
        if directory.name.startswith("windows-r6-chronological-checks-"):
            command[-1] = "test(chronological_cal_rejects_foreign_audit_before_opening_file) | test(compact_cal_population_rejects_reordered_and_duplicate_ordinals) | test(protected_efficient_mha_binds_the_model_graph_and_half_weight_identity) | test(protected_efficient_mha_requires_every_model_specific_cuda_attention_and_projection) | test(original_compact_bounded_options_refused_by_strict_input_plan)"
        else:
            command[-1] += " | test(large_train_q543_requires_original_compiler_and_allowance)"
        if directory.name.startswith("windows-large-direct-timing-checks-"):
            command[-1] += " | test(compact_fp16_timing_fixes_warm_population_and_mixed_cycles)"
    return [sys.executable,"-I","-B","-S",str(ROOT/"scripts/dev/receipt_runner.py"),"--groups","native",
            "--out",str(directory),"--target-dir",str(ROOT/"target"),"--fixture-base",str(directory/"native-fixtures"),
            f"large-{phase}-r1","--",*command]


def run(args):
    shared = runpy.run_path(str(ROOT/"scripts/arbiter/evaluation/run_compact_answer.py"))
    owner = shared["owner_module"]()
    directory = owner.plain(args.output)
    require(directory.parent == CAMPAIGN and (directory.name.startswith("windows-r6-chronological-checks-") or directory.name.startswith(("windows-large-train-q543-scientific-checks-", "windows-large-direct-timing-checks-"))),
            "fresh Large scientific receipt directory required")
    require(Path(os.environ["CARGO_HOME"]) == CAMPAIGN/"windows-offline-cargo-r1/home" and
            os.environ.get("CARGO_BUILD_JOBS") == "1" and os.environ.get("CARGO_NET_OFFLINE") == "true" and
            Path(os.environ["TEMP"]).is_relative_to(directory), "isolated D Cargo/temporary storage and single worker required")
    phase_dir = owner.plain(directory/f"owned-{args.phase}")
    cmd = phase_argv(args.phase,directory)
    if args.worker:
        require(os.environ["AIRK_LARGE_CARGO_PHASE"] == str(phase_dir), "original owned Cargo parent required")
        cap_path = ROOT/"scripts/arbiter/analysis/public_source_admission/aws_normal_service.py"
        require(hashlib.sha256(cap_path.read_bytes()).hexdigest() ==
                "bf2850980a395de97a9cae888f910fe7e95f1f9f679a72641c32940a8ac497f1",
                "original nested Windows resource owner changed")
        cap = runpy.run_path(str(cap_path))["capped"]
        free_floor = max(100 << 30, 1_233_246_265_344-(120 << 30))
        with cap(shared,phase_dir,HOST) as (member, _):
            child = subprocess.Popen(cmd,cwd=ROOT)
            member(child.pid)
            while child.poll() is None:
                require(shutil.disk_usage(ROOT).free >= free_floor+(512 << 20), "D tooling or free capacity expired")
                try:
                    child.wait(timeout=0.3)
                except subprocess.TimeoutExpired:
                    continue
            require(child.returncode == 0, "original Cargo phase refused")
            drained = await_tool_exit(float(os.environ["AIRK_LARGE_CARGO_EXPIRES"]), child.pid)
            shared["write_json"](phase_dir/"tool-exits.json",drained)
        return
    require(not any(phase_dir.iterdir()), "fresh outer Cargo phase directory required")
    original = [sys.executable,"-I","-B","-S",str(Path(__file__).resolve()),*sys.argv[1:],"--worker"]
    wall = min(1800,int(1795194600-time.time()))
    require(wall >= 120,"campaign deadline leaves insufficient native time")
    result = owner.run_evaluation_fixture(original,ROOT,phase_dir/"outer.log",wall,
                                          os.environ | {"AIRK_LARGE_CARGO_PHASE":str(phase_dir), "AIRK_LARGE_CARGO_EXPIRES":str(time.monotonic()+wall)})
    shared["write_json"](phase_dir/"execution.json",result)
    require(result["exit_code"] == 0 and result["owned_descendants_settled"] and result["owned_job_final_active_processes"] == 0
            and not result["timed_out"] and not result["owned_job_forced_termination"], "Cargo phase did not settle")
    limits = json.loads((phase_dir/"limits.json").read_bytes())
    require(limits["host_cap"] == HOST and 0 < limits["peak_job_bytes"] <= HOST and limits["below_normal"],
            "four-GiB Job not verified")
    receipt = json.loads((directory/f"large-{args.phase}-r1.json").read_bytes())
    require(receipt["passed"] and receipt["inputs_unchanged"] and receipt["exit_code"] == 0
            and Path(receipt["target_directory"]) == ROOT/"target", "scoped Cargo receipt differs")
    print(json.dumps({"phase":args.phase,"limits":limits,"cargo":receipt["passed"]}))


if __name__ == "__main__":
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("--phase",choices=TEMPLATES,required=True)
    parser.add_argument("--output",type=Path,required=True)
    parser.add_argument("--worker",action="store_true")
    run(parser.parse_args())
