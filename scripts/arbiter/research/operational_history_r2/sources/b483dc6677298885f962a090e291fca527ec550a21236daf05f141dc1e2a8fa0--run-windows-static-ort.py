# SPDX-License-Identifier: Apache-2.0
"""One owned Windows C++ configure/build; no Cargo or installation effects."""
import ctypes
from ctypes import wintypes
import hashlib
import json
import os
from pathlib import Path
import shutil
import subprocess
import time

BASE = Path(__file__).resolve().parent
SOURCE = BASE / "onnxruntime-source"
BUILD = BASE / "b" / "Release"
VS = Path("C:/Program Files/Microsoft Visual Studio/2022/Community")
CMAKE = VS / "Common7/IDE/CommonExtensions/Microsoft/CMake/CMake/bin/cmake.exe"
NINJA = VS / "Common7/IDE/CommonExtensions/Microsoft/CMake/Ninja/ninja.exe"
VCVARS = VS / "VC/Auxiliary/Build/vcvars64.bat"
PATCH = Path("C:/Program Files/Git/usr/bin/patch.exe")
REVISION = "89746dc19a0a1ae59ebf4b16df9acab8f99f3925"

def digest(path):
    with path.open("rb") as stream:
        return hashlib.file_digest(stream, "sha256").hexdigest()

def owned_job():
    class BasicLimit(ctypes.Structure):
        _fields_ = [("per_process", ctypes.c_longlong), ("per_job", ctypes.c_longlong),
                    ("flags", wintypes.DWORD), ("min_working", ctypes.c_size_t),
                    ("max_working", ctypes.c_size_t), ("active_limit", wintypes.DWORD),
                    ("affinity", ctypes.c_size_t), ("priority", wintypes.DWORD), ("scheduling", wintypes.DWORD)]
    class IO(ctypes.Structure):
        _fields_ = [(name, ctypes.c_ulonglong) for name in ("read_ops", "write_ops", "other_ops", "read_bytes", "write_bytes", "other_bytes")]
    class Extended(ctypes.Structure):
        _fields_ = [("basic", BasicLimit), ("io", IO), ("process_memory", ctypes.c_size_t),
                    ("job_memory", ctypes.c_size_t), ("peak_process", ctypes.c_size_t), ("peak_job", ctypes.c_size_t)]
    class Accounting(ctypes.Structure):
        _fields_ = [(name, ctypes.c_longlong) for name in ("user", "kernel", "period_user", "period_kernel")] + [(name, wintypes.DWORD) for name in ("faults", "total", "active", "terminated")]
    api = ctypes.WinDLL("kernel32", use_last_error=True)
    api.CreateJobObjectW.argtypes = [ctypes.c_void_p, wintypes.LPCWSTR]
    api.CreateJobObjectW.restype = wintypes.HANDLE
    api.GetCurrentProcess.restype = wintypes.HANDLE
    api.SetInformationJobObject.argtypes = [wintypes.HANDLE, ctypes.c_int, ctypes.c_void_p, wintypes.DWORD]
    api.AssignProcessToJobObject.argtypes = [wintypes.HANDLE, wintypes.HANDLE]
    api.QueryInformationJobObject.argtypes = [wintypes.HANDLE, ctypes.c_int, ctypes.c_void_p, wintypes.DWORD, ctypes.c_void_p]
    api.TerminateJobObject.argtypes = [wintypes.HANDLE, wintypes.UINT]
    api.CloseHandle.argtypes = [wintypes.HANDLE]
    handle = api.CreateJobObjectW(None, None)
    if not handle:
        raise ctypes.WinError(ctypes.get_last_error())
    settings = Extended()
    settings.basic.flags = 0x2000  # KILL_ON_JOB_CLOSE, this runner and its descendants only.
    if not api.SetInformationJobObject(handle, 9, ctypes.byref(settings), ctypes.sizeof(settings)) or not api.AssignProcessToJobObject(handle, api.GetCurrentProcess()):
        error = ctypes.get_last_error()
        api.CloseHandle(handle)
        raise ctypes.WinError(error)
    def active():
        value = Accounting()
        if not api.QueryInformationJobObject(handle, 1, ctypes.byref(value), ctypes.sizeof(value), None):
            raise ctypes.WinError(ctypes.get_last_error())
        return value.active
    return api, handle, active

assert os.name == "nt"
assert subprocess.run(["git", "-c", "safe.directory=" + SOURCE.as_posix(), "-C", str(SOURCE), "rev-parse", "HEAD"], capture_output=True, check=True).stdout.decode().strip() == REVISION
assert (SOURCE / "VERSION_NUMBER").read_text().strip() == "1.22.1"
assert not BUILD.exists(), "This campaign never overwrites or retries a prior configure/build."
for program in (CMAKE, NINJA, VCVARS, PATCH):
    assert program.is_file(), str(program)
dependencies = json.loads((BASE / "cpu-dependencies.json").read_text())
for row in dependencies["dependencies"]:
    original = BASE / "dependency-mirror" / row["mirror_relative_path"]
    assert digest(original) == row["sha256"]
    destination = SOURCE / "mirror" / row["mirror_relative_path"]
    assert not destination.exists()
    destination.parent.mkdir(parents=True, exist_ok=True)
    shutil.copyfile(original, destination)

BUILD.mkdir(parents=True)
configure = [str(CMAKE), "-S", str(SOURCE / "cmake"), "-B", str(BUILD), "-G", "Ninja",
             "-DCMAKE_BUILD_TYPE=Release", "-DCMAKE_MAKE_PROGRAM=" + str(NINJA),
             "-DCMAKE_SYSTEM_VERSION=10.0.26100.0", "-DCMAKE_MSVC_RUNTIME_LIBRARY=MultiThreadedDLL",
             "-DONNX_USE_MSVC_STATIC_RUNTIME=OFF", "-Dprotobuf_MSVC_STATIC_RUNTIME=OFF", "-DABSL_MSVC_STATIC_RUNTIME=OFF",
             "-DFETCHCONTENT_TRY_FIND_PACKAGE_MODE=NEVER", "-DPatch_EXECUTABLE=" + str(PATCH),
             "-Donnxruntime_BUILD_SHARED_LIB=OFF", "-Donnxruntime_BUILD_UNIT_TESTS=OFF", "-Donnxruntime_BUILD_BENCHMARKS=OFF",
             "-Donnxruntime_BUILD_FOR_NATIVE_MACHINE=OFF", "-Donnxruntime_MINIMAL_BUILD=OFF",
             "-Donnxruntime_DISABLE_CONTRIB_OPS=OFF", "-Donnxruntime_DISABLE_ML_OPS=OFF",
             "-Donnxruntime_ENABLE_PYTHON=OFF", "-Donnxruntime_USE_CUDA=OFF", "-Donnxruntime_USE_DML=OFF",
             "-Donnxruntime_USE_VCPKG=OFF", "-Donnxruntime_USE_TELEMETRY=OFF", "-Donnxruntime_ENABLE_LTO=OFF"]
build = [str(CMAKE), "--build", str(BUILD), "--config", "Release", "--parallel", "2"]
receipt = {"schema": 1, "upstream_revision": REVISION, "ort_version": "1.22.1", "configure": configure,
           "build": build, "cpu_parallelism": 2, "cargo_executed": False, "source_production_edits": False,
           "dependency_manifest_sha256": digest(BASE / "cpu-dependencies.json"), "commands": [], "result": "running"}
receipt_file = BASE / "windows-static-build-result.json"
def save():
    receipt_file.write_text(json.dumps(receipt, indent=2) + "\n")
save()
api, job, active = owned_job()
started = time.monotonic()
failure = None
try:
    for stage, command, timeout in (("configure", configure, 600), ("build", build, 10800)):
        script = BASE / ("windows-static-" + stage + ".cmd")
        # Fixed reviewed paths/argv only. vcvars modifies this owned child environment.
        script.write_text('@echo off\r\ncall "' + str(VCVARS) + '" 10.0.26100.0 > "' + str(BASE / (stage + "-toolchain.log")) + '" 2>&1\r\nif errorlevel 1 exit /b %errorlevel%\r\n' + subprocess.list2cmdline(command) + '\r\nexit /b %errorlevel%\r\n', newline="")
        logfile = BASE / ("windows-static-" + stage + ".log")
        turn = {"stage": stage, "script": str(script), "log": str(logfile), "started_unix": time.time()}
        receipt["commands"].append(turn)
        save()
        print("Starting " + stage + ": " + str(logfile), flush=True)
        stage_started = time.monotonic()
        with logfile.open("wb") as output:
            completed = subprocess.run(["cmd.exe", "/d", "/c", str(script)], cwd=BASE, stdout=output, stderr=subprocess.STDOUT, timeout=timeout, creationflags=subprocess.CREATE_NO_WINDOW)
        turn.update({"exit_code": completed.returncode, "elapsed_seconds": round(time.monotonic() - stage_started, 3)})
        save()
        if completed.returncode:
            raise RuntimeError(stage + " failed; campaign stops without retry")
    libraries = []
    for directory in [BUILD, BUILD / "_deps"]:
        candidates = directory.glob("*.lib") if directory == BUILD else directory.rglob("*.lib")
        for library in candidates:
            libraries.append({"path": str(library.relative_to(BUILD)), "bytes": library.stat().st_size, "sha256": digest(library)})
    receipt["libraries"] = libraries
    receipt["result"] = "libraries_built_not_yet_model_or_static_link_qualified"
except Exception as error:
    receipt["result"] = "failed"
    receipt["error"] = str(error)
    failure = error
finally:
    receipt["elapsed_seconds"] = round(time.monotonic() - started, 3)
    end = time.monotonic() + 10
    while active() > 1 and time.monotonic() < end:
        time.sleep(.1)
    receipt["owned_job_active_processes_including_runner"] = active()
    save()
    print(json.dumps({"result": receipt["result"], "elapsed_seconds": receipt["elapsed_seconds"], "owned_job_active_processes_including_runner": active()}), flush=True)
    if active() > 1:
        receipt["owned_job_forced_termination"] = True
        save()
        api.TerminateJobObject(job, 77)
    # Closing the job explicitly would terminate this runner too. It stays open
    # until process exit, when only owned descendants could be terminated.
if failure:
    raise SystemExit(1)
