# SPDX-License-Identifier: Apache-2.0
"""Run one reviewed Compact selector under WSL2 after the Windows GPU gate."""
import argparse
import ctypes
import hashlib
import json
import os
from pathlib import Path
import pwd
import runpy
import select
import stat
import subprocess
import sys
import threading
import time
import uuid

from fp16_process import process_memory
from fp16_operations import verify as verify_operations
from unpack import guarded

BUILD = Path("/home/tyler/airk-builds/arbiter-final-linux-r1")
MODULE = "inference_engine::pool::arbiter::qualification::software_v2::cuda_qualification::fp16_compact::"
MODES = {
    "graph": "cuda_compact_fp32_qkv_projection_load_only_graph_diagnostic",
    "t32": "cuda_compact_fp32_qkv_projection_t32_component",
    "wider": "cuda_compact_fp32_qkv_projection_development_batch",
    "fixed": "cuda_compact_fp16_resource20",
    "mixed": "cuda_compact_fp16_mixed_timing",
}
NVML = Path("/usr/lib/wsl/lib/libnvidia-ml.so.1")
NVML_SHA = "873a769d099be3c9ee3864db80a4716af22c30e6b204b788973652d7b99f2d9f"
HOST = 32 << 30
DEVICE = 24 << 30


def sha(file):
    file.seek(0)
    return hashlib.file_digest(file, "sha256").hexdigest()


def verified_file(path, expected):
    file = path.open("rb")
    try:
        if sha(file) != expected:
            raise ValueError(f"SHA differs: {path}")
        file.seek(0)
        return file
    except BaseException:
        file.close()
        raise


def private(path, directory=False):
    if not path.is_relative_to(BUILD) or path != path.resolve(strict=True):
        raise ValueError(f"private path outside native build root: {path}")
    for parent in path.parents:
        if parent == Path("/home"):
            break
        guarded(parent, os.getuid(), stat.S_ISDIR)
    guarded(path, os.getuid(), stat.S_ISDIR if directory else stat.S_ISREG)


class DeviceMemory(ctypes.Structure):
    _fields_ = [(name, ctypes.c_ulonglong) for name in ("total", "free", "used")]


class Adapter:
    def __init__(self, uuid):
        if not uuid.startswith("GPU-") or len(uuid) != 40:
            raise ValueError("exact NVIDIA GPU UUID required")
        guarded(NVML, 0, stat.S_ISREG)
        with verified_file(NVML, NVML_SHA) as library:
            self.nvml = ctypes.CDLL(f"/proc/self/fd/{library.fileno()}", mode=os.RTLD_NOW)
        self.handle = ctypes.c_void_p()
        self.check(self.nvml.nvmlInit_v2())
        try:
            self.check(self.nvml.nvmlDeviceGetHandleByUUID(uuid.encode(), ctypes.byref(self.handle)))
        except BaseException:
            self.close()
            raise

    @staticmethod
    def check(code):
        if code:
            raise RuntimeError(f"WSL NVML operation failed: {code}")

    def sample(self):
        info = DeviceMemory()
        self.check(self.nvml.nvmlDeviceGetMemoryInfo(self.handle, ctypes.byref(info)))
        if not 0 < info.total or not 0 <= info.free <= info.total or not 0 <= info.used <= info.total:
            raise ValueError("invalid WSL adapter memory observation")
        return {"total": info.total, "free": info.free, "used": info.used}

    def close(self):
        self.check(self.nvml.nvmlShutdown())


def observe(proc, gpu, deadline, baseline, samples, device_limit=DEVICE):
    fd = os.pidfd_open(proc.pid)
    try:
        poll = select.poll()
        poll.register(fd, select.POLLIN)
        while True:
            now = time.monotonic()
            if now >= deadline:
                raise TimeoutError("original FP16 parent deadline expired")
            if poll.poll(0):
                return proc.wait()
            sample_start = time.monotonic_ns()
            try:
                host = process_memory(proc.pid)
            except (FileNotFoundError, ProcessLookupError):
                if poll.poll(0):
                    return proc.wait()
                raise
            device = gpu.sample()
            samples.append({"sample_started_ns":sample_start, "monotonic_ns": time.monotonic_ns(), "host": host, "adapter": device})
            if device["total"] != baseline["total"] or device["used"] - baseline["used"] > device_limit:
                raise ValueError("observed device-wide growth exceeds original allocation cap")
            poll.poll(min(100, max(0, int((deadline - time.monotonic()) * 1000))))
    finally:
        os.close(fd)


def guarded_observation(proc, gpu, deadline, baseline, samples, receipt, *, device_limit=DEVICE):
    finished = threading.Event()
    fd = os.pidfd_open(proc.pid)
    expired = []

    def watchdog():
        if not finished.wait(max(0, deadline - time.monotonic())):
            try:
                import signal
                signal.pidfd_send_signal(fd, signal.SIGKILL)
                expired.append(True)
            except ProcessLookupError:
                pass

    worker = threading.Thread(target=watchdog, name="scorer-deadline")
    worker.start()
    try:
        return observe(proc, gpu, deadline, baseline, samples, device_limit)
    finally:
        finished.set()
        worker.join()
        os.close(fd)
        if expired:
            receipt["forced_termination"] = True
            raise TimeoutError("watchdog reached original scorer deadline")


def native_settled(value, pid, image_sha, gpu_uuid):
    expected = list(uuid.UUID(gpu_uuid.removeprefix("GPU-")).bytes)
    if any(value.get(phase, {}).get("uuid") != expected for phase in ("adapter_before", "adapter_after")):
        raise ValueError("native CUDA UUID differs from independently sampled NVIDIA device")
    flags = ("complete", "session_teardown_observed", "device_teardown_observed",
             "native_control_cleanup_observed", "native_host_charge_release_observed")
    if (value.get("pid") != pid or value.get("native_binary_sha256") != image_sha
            or any(value.get(key) is not True for key in flags)
            or value.get("native_control_retained_for_unknown_cuda") is not False
            or value.get("process_cuda_poisoned") is not False):
        raise ValueError("exact native scorer identity or natural settlement unverified")


def run(args):
    uid = os.getuid()
    if os.getresuid() != (1000,) * 3 or pwd.getpwuid(uid).pw_name != "tyler":
        raise ValueError("registered non-root WSL human required")
    if any(k.startswith("LD_") or k in ("GLIBC_TUNABLES", "CUDNN_FRONTEND_CUDART_LIB_NAME")
           for k in os.environ):
        raise ValueError("loader environment is not frozen")
    os.umask(0o077)
    private(args.directory, True)
    private(args.image)
    validator = Path(__file__).resolve().parents[3] / "arbiter-fp16-execution" / "probe_compact.py"
    with verified_file(validator, args.validator_sha):
        common = runpy.run_path(str(validator))
    private(args.metadata)
    if any(args.directory.iterdir()):
        raise ValueError("fresh private run directory required")
    with verified_file(args.metadata, args.metadata_sha) as file:
        metadata = json.load(file)
    reserve = metadata["private_elf_backing_bytes"]
    if type(reserve) is not int or not 0 < reserve < HOST or reserve % os.sysconf("SC_PAGE_SIZE"):
        raise ValueError("native private ELF backing reserve unavailable")
    if metadata["execution_user"] != str(uid) or metadata["qualified"] is not False:
        raise ValueError("native preparation identity differs")
    request = metadata["probe_request"]
    private(Path(request["path"]))
    with verified_file(Path(request["path"]), request["sha256"]) as file:
        native = json.load(file)
    bounds = native["bounds"]
    if (bounds["host_limit_bytes"], bounds["device_limit_bytes"], bounds["deadline_ms"],
            native["job_limit_bytes"]) != (HOST, DEVICE, 120000, HOST):
        raise ValueError("native Compact request limits differ")
    timing = None
    if args.mode in ("fixed", "mixed"):
        timing_path = validator.with_name("timing_control.py")
        with verified_file(timing_path, args.timing_validator_sha):
            timing = runpy.run_path(str(timing_path))
    receipt = {"schema": "airk.private.linux-fp16-parent/1", "mode": args.mode,
               "uid": uid, "command_selector": MODULE + MODES[args.mode],
               "image_sha256": args.image_sha, "metadata_sha256": args.metadata_sha,
               "attention_validator_sha256": args.validator_sha, "timing_validator_sha256": args.timing_validator_sha, "request_sha256": request["sha256"], "private_elf_backing_bytes": reserve,
               "address_space_limit_bytes": HOST - reserve, "device_limit_bytes": DEVICE,
               "gpu_uuid": args.gpu_uuid, "nvml_sha256": NVML_SHA, "samples": [],
               "parent_complete": False, "forced_termination": False, "qualification": False}
    output = args.directory / "native.json"
    env = {k: v for k, v in os.environ.items()
           if not k.upper().startswith(("AIRK_", "ORT_", "PYTHON", "CUDA_", "CUDNN_", "NVIDIA_"))}
    sources = ["AIRK_CUDA_FP16_PROJECTION_STAGE"]
    if args.mode != "graph":
        sources.extend(("AIRK_CUDA_FP16_PROJECTION_REFERENCE", "AIRK_CUDA_FP16_REVIEWED_GRAPH"))
    if timing is not None:
        sources.extend(("AIRK_CUDA_FP16_TIMING_T32", "AIRK_CUDA_FP16_TIMING_WIDER"))
    for base in sources:
        for name in (base, base + "_SHA256"):
            env[name] = os.environ[name]
    scratch = args.directory / "private-temp"
    scratch.mkdir(mode=0o700)
    private(scratch, True)
    receipt["math_environment"] = {"ORT_CUDA_GEMM_OPTIONS": "2", "ORT_ENABLE_ATTENTION_KERNEL_DEBUG_INFO": "1"}
    env.update(receipt["math_environment"])
    env.update(PATH="/usr/bin:/bin", TEMP=str(scratch), TMP=str(scratch), TMPDIR=str(scratch),
               AIRK_PRIVATE_CENSUS_BASE=str(scratch), CUDA_CACHE_PATH=str(scratch),
               AIRK_CUDA_QUALIFICATION_INPUT=request["path"],
               AIRK_CUDA_QUALIFICATION_INPUT_SHA256=request["sha256"],
               AIRK_CUDA_FP16_METADATA_RECEIPT=str(args.metadata),
               AIRK_CUDA_FP16_METADATA_RECEIPT_SHA256=args.metadata_sha,
               AIRK_CUDA_QUALIFICATION_OUTPUT=str(output),
               AIRK_CUDA_FP16_PROFILE_PREFIX=str(args.directory / "profile"))
    guard = Path(__file__).resolve().with_name("fp16_process.py")
    log = args.directory / "native.log"
    proc = gpu = None
    try:
        gpu = Adapter(args.gpu_uuid)
        baseline = receipt["baseline_adapter"] = gpu.sample()
        with verified_file(args.image, args.image_sha) as image, log.open("xb") as sink:
            command = [sys.executable, "-IB", str(guard), str(os.getpid()), str(HOST - reserve),
                       str(image.fileno()), str(args.image), "--exact", receipt["command_selector"],
                       "--ignored", "--nocapture", "--test-threads=1"]
            started = time.monotonic()
            deadline = started + 120
            proc = subprocess.Popen(command, env=env, stdin=subprocess.DEVNULL, stdout=sink,
                                    stderr=subprocess.STDOUT, pass_fds=(image.fileno(),),
                                    start_new_session=True)
            receipt["pid"] = proc.pid
            status = guarded_observation(proc, gpu, deadline, baseline, receipt["samples"], receipt)
            receipt["natural_exit_status"] = status
            receipt["elapsed_seconds"] = time.monotonic() - started
            receipt["after_exit_adapter"] = gpu.sample()
            if status != 0 or receipt["elapsed_seconds"] > 120 or not receipt["samples"]:
                raise ValueError("bounded native process failed or returned no samples")
        if log.stat().st_size > common["LOG_CAP"]:
            raise ValueError("native session log exceeds original evidence bound")
        native_log = log.read_bytes()
        if b"test result: ok. 1 passed;" not in native_log:
            raise ValueError("exactly one native selector must pass")
        private(output)
        with output.open("rb") as file:
            receipt["native_sha256"] = sha(file)
            file.seek(0)
            value = json.load(file)
            native_settled(value, proc.pid, args.image_sha, args.gpu_uuid)
            journal = output.with_suffix(".operations.jsonl")
            private(journal)
            with journal.open("rb") as jf:
                receipt["journal_sha256"] = sha(jf)
            events = common["read_operations"](journal)
            receipt["memory_by_cycle"] = verify_operations(args.mode, value, events, receipt["samples"])
            if timing is not None:
                sealed = {}
                for pin in value["timing_score_sources"]:
                    private(Path(pin["path"]))
                    with verified_file(Path(pin["path"]), pin["sha256"]) as sf:
                        for row in json.load(sf)["rows"]:
                            if row["tokens"] in sealed:
                                raise ValueError("duplicate sealed score width")
                            sealed[row["tokens"]] = row
                mode = "qkv-resource20" if args.mode == "fixed" else "qkv-mixed"
                receipt["timing_statistics"] = timing["timing_rows"](mode, value["rows"], sealed)
            if args.mode == "t32":
                receipt["flash_attention_records"] = common["attention_logs"](native_log, "projection-t32")
            elif args.mode in ("wider", "fixed", "mixed"):
                mode = {"wider": "qkv-development", "fixed": "qkv-resource20", "mixed": "qkv-mixed"}[args.mode]
                receipt["flash_attention_records"] = common["development_attention_logs"](native_log, value["rows"], mode)
            else:
                if b"SdpaKernel=" in native_log or value.get("numeric_forward_attempts") != 0:
                    raise ValueError("graph-only selector performed a scored forward")
        with verified_file(validator, args.validator_sha):
            pass
        receipt["sampled_device_growth_peak_bytes"] = max(0, max(
            row["adapter"]["used"] - baseline["used"] for row in receipt["samples"]))
        receipt["sampled_host_rss_peak_bytes"] = max(row["host"]["VmRSS"] for row in receipt["samples"])
        receipt["parent_complete"] = True
    except BaseException as error:
        receipt["failure"] = str(error)
        raise
    finally:
        try:
            if proc is not None and proc.poll() is None:
                proc.kill()
                proc.wait(timeout=10)
                receipt["forced_termination"] = True
                receipt["parent_complete"] = False
            if gpu is not None:
                gpu.close()
        finally:
            path = args.directory / "parent.json"
            with path.open("x") as out:
                json.dump(receipt, out, separators=(",", ":"))
            for path in args.directory.rglob("*"):
                private(path, path.is_dir())


if __name__ == "__main__":
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("mode", choices=MODES)
    for name in ("image", "metadata", "directory"):
        parser.add_argument("--" + name, type=Path, required=True)
    for name in ("image-sha", "metadata-sha", "validator-sha", "gpu-uuid"):
        parser.add_argument("--" + name, required=True)
    parser.add_argument("--timing-validator-sha")
    args = parser.parse_args()
    if bool(args.timing_validator_sha) != (args.mode in ("fixed", "mixed")):
        parser.error("fixed and mixed timing require the reviewed timing validator digest")
    run(args)
