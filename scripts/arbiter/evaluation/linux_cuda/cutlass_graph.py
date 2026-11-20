# SPDX-License-Identifier: Apache-2.0
"""Observe a GNU protected Context graph using the original bounded scientific image."""
import argparse
import json
import os
from pathlib import Path
import subprocess
import sys
import time

from cutlass_observation import ADAPTER, AGGREGATE, LIMITS, SCIENTIFIC, confirm
from fp16_supervisor import Adapter, guarded_observation, verified_file
from product_projection import GUARD, PREFIX, checked_path, document, identity, image_proof, pin, require, write


def run(args):
    identity()
    model = args.model
    host, device = LIMITS[model]
    selector = PREFIX + f"gnu_cutlass::{model}_gnu_cutlass_graph"
    os.umask(0o077)
    require(not any(k.startswith("LD_") or k in {"GLIBC_TUNABLES", "CUDNN_FRONTEND_CUDART_LIB_NAME"}
                    for k in os.environ), "ambient loader override")
    image = checked_path(args.image)
    proof = image_proof(args.image_proof, args.image_proof_sha256, pin(image))
    require(proof["manifest_sha256"] == SCIENTIFIC and proof["aggregate_static_sha256"] == AGGREGATE,
            "original GNU static scientific image required")
    manifest = document(image.parent / "airk-cuda-runtime.json", SCIENTIFIC)
    closure = manifest["closure"]
    require(manifest["schema"] == "airk.native.linux-cuda-runtime-manifest/1"
            and closure["static_core_build"] == proof["static_core_build"], "embedded native producer differs")
    driver, = [entry["sha256"] for entry in closure["system_imports"] if entry["name"] == "libcuda.so.1"]
    page = os.sysconf("SC_PAGE_SIZE")
    backing = sum((entry["bytes"] + page - 1) // page * page for entry in closure["files"])
    require(0 < backing < host, "invalid private ELF reservation")
    # This path charges sealed ELF backing only; the staged sibling payload must use native ext4.
    mount = subprocess.run(["/usr/bin/findmnt", "--noheadings", "--output", "FSTYPE", "--target", str(image)],
                           text=True, capture_output=True, check=True)
    require(mount.stdout.strip() == "ext4", "original ext4 runtime required for this host reservation")
    out = checked_path(args.directory, True)
    require(not any(out.iterdir()), "fresh private observation directory required")
    scratch = out / "tmp"
    scratch.mkdir(0o700)
    checked_path(scratch, True)
    native_path = out / "native.json"
    env = {"HOME": "/home/tyler", "PATH": "/usr/bin:/bin", "LC_ALL": "C",
           "TMPDIR": str(scratch), "TMP": str(scratch), "TEMP": str(scratch), "CUDA_CACHE_PATH": str(scratch),
           "AIRK_PRIVATE_CENSUS_BASE": str(scratch), "AIRK_CUDA_QUALIFICATION_OUTPUT": str(native_path),
           "ORT_CUDA_GEMM_OPTIONS": "2", "ORT_ENABLE_ATTENTION_KERNEL_DEBUG_INFO": "1"}
    receipt = {"model": model, "host_limit": host, "device_limit": device, "selector": selector, "image": proof["image"], "image_proof_sha256": args.image_proof_sha256,
               "descriptor_sha256": SCIENTIFIC, "private_backing": backing, "driver_sha256": driver,
               "samples": [], "forced_termination": False, "qualified": False, "parent_complete": False}
    gpu = child = None
    try:
        with verified_file(image, proof["image"]["sha256"]) as executable, (out / "stdout.log").open("xb") as log:
            expires = time.monotonic_ns() + 120_000_000_000
            env[f"AIRK_{model.upper()}_CUTLASS_EXPIRES_NS"] = str(expires)
            gpu = Adapter(ADAPTER)
            baseline = receipt["baseline"] = gpu.sample()
            require(baseline["free"] >= device, "insufficient physical GPU headroom")
            cmd = [sys.executable, "-IB", str(GUARD), str(os.getpid()), str(host - backing),
                   str(executable.fileno()), str(image), "--exact", selector, "--ignored", "--nocapture", "--test-threads=1"]
            child = subprocess.Popen(cmd, env=env, cwd=out, stdin=subprocess.DEVNULL, stdout=log,
                                     stderr=subprocess.STDOUT, pass_fds=(executable.fileno(),), start_new_session=True)
            receipt["pid"] = child.pid
            receipt["exit_code"] = guarded_observation(child, gpu, expires / 1e9, baseline, receipt["samples"], receipt, device_limit=device)
            receipt["after_exit"] = gpu.sample()
            require(receipt["exit_code"] == 0 and time.monotonic_ns() < expires and receipt["samples"],
                    "original bounded graph process failed")
        path = checked_path(out / "stdout.log")
        require(path.stat().st_size <= 64 << 20, "native log oversized")
        logs = path.read_bytes()
        require(b"test result: ok. 1 passed;" in logs, "exactly one GNU graph selector must pass")
        receipt["graph_output"] = pin(native_path)
        require(receipt["graph_output"]["bytes"] <= 64 << 20, "native graph oversized")
        value = document(native_path, receipt["graph_output"]["sha256"], 64 << 20)
        result = confirm(value, child.pid, backing, driver, logs, model)
        receipt["typed_graph_sha256"] = result["typed_graph_sha256"]
        receipt["cpu_refusal"] = result["cpu_refusal"]
        receipt["parent_complete"] = True
    except BaseException as error:
        receipt["failure"] = str(error)
        raise
    finally:
        try:
            if child is not None and child.poll() is None:
                child.kill()
                child.wait(timeout=10)
                receipt["forced_termination"] = True
            if gpu is not None:
                gpu.close()
        finally:
            write(out / "parent.json", receipt)
            for item in out.rglob("*"):
                checked_path(item, item.is_dir())


if __name__ == "__main__":
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("--model", choices=LIMITS, default="compact")
    for name in ("image", "image-proof", "directory"):
        parser.add_argument("--" + name, type=Path, required=True)
    parser.add_argument("--image-proof-sha256", required=True)
    run(parser.parse_args())
