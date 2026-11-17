# SPDX-License-Identifier: Apache-2.0
"""Attach only to the owned static vertex; interrupt the exact superseded client after dedup proof."""
import ctypes
import hashlib
import importlib.util
import json
from pathlib import Path
import re
import subprocess
import time

ROOT = Path(__file__).resolve().parents[3]
BASE = Path(__file__).resolve().parent
OUTPUT = BASE / "linux-static-donor-r1"
DERIVATIVE = BASE / "linux-container-qualification-r2/Dockerfile.local-qualification"
ORIGINAL_REF = "desktop-linux/desktop-linux/lw5phixc05c3iw4ozvtbl1udy"
OPERATION = "RUN python3 scripts/provision-static-onnx-runtime.py --destination /build/static-onnx-runtime --download --download-tools --build --parallel 2"
OUTPUT.mkdir(exist_ok=False)
record = {"schema": "airk-owned-static-donor/1", "qualification": False,
          "scope": "Canonical static-inference vertex only; no source/Cargo/test/release stage. Root-directed interruption of superseded qualification, not a native failure.",
          "original_build_ref": ORIGINAL_REF, "passed": False}


def retain():
    (OUTPUT / "receipt.json").write_text(json.dumps(record, indent=2) + "\n")


def static_processes():
    command = "ps -eo pid,etimes,args | grep -F 'python3 scripts/provision-static-onnx-runtime.py --destination /build/static-onnx-runtime --download --download-tools --build --parallel 2'"
    observed = subprocess.check_output(["wsl", "-d", "docker-desktop", "--", "sh", "-c", command], text=True)
    expression = r"^\s*(\d+)\s+(\d+)\s+python3 scripts/provision-static-onnx-runtime\.py --destination /build/static-onnx-runtime --download --download-tools --build --parallel 2\s*$"
    return [{"pid": int(match[1]), "elapsed_seconds": int(match[2])} for line in observed.splitlines()
            if (match := re.fullmatch(expression, line))]


def static_vertices(path):
    selected = set()
    for line in path.read_text(errors="replace").splitlines():
        try:
            value = json.loads(line)
        except ValueError:
            continue
        if OPERATION in value.get("name", ""):
            selected.add(value["id"])
        for vertex in value.get("vertexes", []):
            if OPERATION in vertex.get("name", ""):
                selected.add(vertex.get("digest", vertex.get("id")))
    return sorted(value for value in selected if value)


backend_spec = importlib.util.spec_from_file_location("static_donor_owned_processes", ROOT / "scripts/owned-runtime-processes.py")
processes = importlib.util.module_from_spec(backend_spec)
backend_spec.loader.exec_module(processes)
backend = processes._backend()
backend.kernel.TerminateProcess.argtypes = [backend.w.HANDLE, backend.w.UINT]
backend.kernel.TerminateProcess.restype = backend.w.BOOL
handle = backend.kernel.OpenProcess(0x1000 | 0x100000 | 0x0001, False, 107808)
if not handle:
    raise ctypes.WinError(ctypes.get_last_error())
owner = processes._WindowsOwner(backend, 107808, handle)
donor = None
try:
    observed = owner.observation()
    assert not owner.gone() and observed["user"] == backend.user
    assert Path(observed["path"]).name.lower() == "docker-buildx.exe"
    command = "Get-CimInstance Win32_Process | Where-Object { $_.ProcessId -in 107808,193688,103904 } | Select-Object ProcessId,ParentProcessId,Name,CommandLine | ConvertTo-Json -Compress"
    rows = json.loads(subprocess.check_output(["powershell", "-NoProfile", "-Command", command], text=True))
    by_id = {row["ProcessId"]: row for row in rows}
    assert by_id[107808]["ParentProcessId"] == 193688 and by_id[193688]["ParentProcessId"] == 103904
    assert "run-linux-container-qualification-r1.py --execute --label linux-container-qualification-r2" in by_id[103904]["CommandLine"]
    for pid in (107808, 193688):
        assert str(DERIVATIVE) in by_id[pid]["CommandLine"] and "--target smoke" in by_id[pid]["CommandLine"]
        assert "--tag airk:local-1814-1295b7ec92cc" in by_id[pid]["CommandLine"]
    record["held_qualification_client"] = observed
    record["ancestry"] = rows
    original_processes = static_processes()
    assert len(original_processes) == 1 and original_processes[0]["pid"] == 18249
    record["original_static_process"] = original_processes[0]
    retain()

    # This observer is a separate read-only history client. Its timeout closes
    # only that observer, never the build solve or provisioner.
    history_path = OUTPUT / "original-static-vertex.jsonl"
    with history_path.open("wb") as output:
        try:
            subprocess.run(["docker", "buildx", "history", "logs", "--builder", "desktop-linux", "--progress", "rawjson", ORIGINAL_REF],
                           stdout=output, stderr=subprocess.STDOUT, timeout=5)
        except subprocess.TimeoutExpired:
            pass
    original_vertices = static_vertices(history_path)
    assert len(original_vertices) == 1, "Cannot establish the original exact static vertex; do not interrupt"
    record["original_static_vertex"] = original_vertices[0]
    record["command"] = ["docker", "buildx", "build", "--builder", "desktop-linux", "--platform", "linux/amd64",
                         "--file", str(DERIVATIVE), "--target", "static-inference", "--progress", "rawjson",
                         "--output", "type=cacheonly", "--metadata-file", str(OUTPUT / "build-metadata.json"), "."]
    started = time.monotonic()
    with (OUTPUT / "static-donor.jsonl").open("wb") as output:
        donor = subprocess.Popen(record["command"], cwd=ROOT, stdout=output, stderr=subprocess.STDOUT)
        record["donor_client_pid"] = donor.pid
        retain()
        deadline = time.monotonic() + 30
        while time.monotonic() < deadline:
            if donor.poll() is not None:
                raise RuntimeError("Static donor ended before attachment proof; do not interrupt")
            vertices = static_vertices(OUTPUT / "static-donor.jsonl")
            if original_vertices[0] in vertices:
                live = static_processes()
                if len(live) == 1 and live[0]["pid"] == original_processes[0]["pid"]:
                    record["deduplication"] = {"vertex": original_vertices[0], "same_single_provisioner": live[0],
                                               "proof": "Both BuildKit progress streams identify the same content vertex; exact original provisioner remains the sole matching process."}
                    break
            time.sleep(0.1)
        else:
            raise RuntimeError("Deduplication not proven; do not interrupt qualification or provisioner")
        assert owner.observation() == observed and not owner.gone()
        # Only the held process that was proved to be this run's solve client.
        # The shared, deduplicated static provisioner is not signaled.
        if not backend.kernel.TerminateProcess(owner.handle, 130):
            raise ctypes.WinError(ctypes.get_last_error())
        record["qualification_interruption"] = {"exit_code_requested": 130, "reason": "root-directed superseded source qualification", "signals_to_static_provisioner": 0}
        retain()
        print("Same static vertex/provisioner proven; superseded qualification client interrupted, static-only donor continues.", flush=True)
        code = donor.wait()
    record["exit_code"] = code
    record["elapsed_seconds"] = time.monotonic() - started
    record["static_vertices"] = static_vertices(OUTPUT / "static-donor.jsonl")
    record["passed"] = code == 0
    record["cache_commit_basis"] = "Successful static-inference-only BuildKit solve/cache export" if code == 0 else "not established"
    if code:
        raise RuntimeError("Static-only donor failed; retain logs, no Cargo qualification or retry")
except BaseException as error:
    record["failure"] = {"type": type(error).__name__, "message": str(error)}
    raise
finally:
    owner.close()
    retain()
