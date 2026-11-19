# SPDX-License-Identifier: Apache-2.0
"""Run one owned Windows Large FP32 graph observation, without scoring."""

import argparse
import datetime
import json
import os
from pathlib import Path
import runpy
import sys
import time

ROOT = Path(__file__).resolve().parents[2]
HOST_BYTES = 48 << 30
PROFILE = "eb467966e528b3b34f0c95d26bdcb0e3f404267b5dcaa673cc6c56755b85905b"
REQUEST_SHA = "9a7daee2c118030c979852efc6780f011356af320e0c3fe0a6d1d9a2a0ab33b0"
SELECTOR = (
    "inference_engine::pool::arbiter::qualification::software_v2::"
    "cuda_qualification::cuda_large_fp32_load_only_graph_metadata_diagnostic"
)


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("--request", required=True, type=Path)
    parser.add_argument("--image", required=True, type=Path)
    parser.add_argument("--image-sha256", required=True)
    parser.add_argument("--build-receipt", required=True, type=Path)
    parser.add_argument("--build-receipt-sha256", required=True)
    parser.add_argument("--powershell", required=True, type=Path)
    parser.add_argument("--output", required=True, type=Path)
    args = parser.parse_args()

    component = runpy.run_path(str(ROOT / "scripts/cuda-component-execution.py"))
    base = component["base"]()
    require = component["require"]
    require(os.name == "nt" and sys.flags.isolated and sys.flags.dont_write_bytecode,
            "use the verified Windows account and isolated Python -I -B -S")
    base_guard = {"checked_guard": base.pin(ROOT / "scripts/dev/assert-human-execution.ps1"),
                  "powershell": base.pin(args.powershell)}
    parent, owner = base.helpers({"ownership_helper_root": str(base.MAINTAINED_HELPER_ROOT)})
    guard = component["guard_many"]
    mkdir = component["mkdir"]
    request = base.pin(args.request)
    image = base.pin(args.image)
    build = base.pin(args.build_receipt)
    require(request["sha256"] == REQUEST_SHA and image["sha256"] == args.image_sha256
            and build["sha256"] == args.build_receipt_sha256,
            "reviewed Large request, staged native image or build receipt differs")
    probe = base.read(args.request)
    manifest = args.image.with_name("airk-cuda-runtime.json")
    require({key: base.pin(manifest)[key] for key in ("path", "sha256")}
            == probe["runtime_manifest"], "native runtime locator or bytes differ")
    observed_build = base.read(args.build_receipt, 32 << 20)
    require(observed_build.get("passed") is True and observed_build.get("inputs_unchanged") is True,
            "the scoped native Cargo build must have passed with stable source")
    log_name = observed_build["log"]
    require(Path(log_name).name == log_name, "the retained Cargo log must be beside its receipt")
    build_log = args.build_receipt.with_name(log_name)
    guard(base_guard, [build_log], True)
    log_pin = base.pin(build_log)
    require(log_pin["bytes"] <= 32 << 20, "Cargo JSON log is oversized")
    with build_log.open(encoding="utf-8") as log:
        artifacts = []
        for line in log:
            if not line.startswith("{"):
                continue
            event = json.loads(line)
            if (event.get("reason") == "compiler-artifact" and event["target"]["name"] == "airk_inference"
                    and event["profile"]["test"] is True and event.get("executable")):
                artifacts.append(Path(event["executable"]))
    require(len(artifacts) == 1, "the Cargo log must contain exactly one Inference lib-test image")
    guard(base_guard, artifacts, True)
    require(base.pin(artifacts[0])["sha256"] == image["sha256"],
            "the staged image differs from the tested Cargo image")
    require(args.output.is_absolute() and args.output.parent.resolve(strict=True) == args.output.parent
            and args.output.is_relative_to(ROOT / "artifacts") and not args.output.exists(),
            "a fresh private campaign output is required")
    guard(base_guard, [args.request, args.image, args.build_receipt, manifest], True)
    require(parent.worker.windows_sid() == base.SID, "parent SID differs")

    start = time.monotonic()
    started_utc = datetime.datetime.now(datetime.timezone.utc)
    deadline = start + 120.0
    mkdir(base_guard, args.output, deadline=deadline)
    temp = args.output / "temp"
    mkdir(base_guard, temp, deadline=deadline)
    stdin_path = args.output / "stdin.txt"
    stdout_path = args.output / "stdout-stderr.log"
    native_path = args.output / "native-result.json"
    receipt_path = args.output / "execution.json"
    guard(base_guard, [stdin_path, stdout_path, native_path, receipt_path], deadline=deadline)

    command = [str(args.image), "--ignored", "--exact", SELECTOR, "--test-threads=1"]
    receipt = {
        "schema": "airk.private.owned-large-cuda-graph-observation/1",
        "complete": False, "qualification": False, "model_requests_dispatched": False,
        "started_utc": started_utc.isoformat(), "wall_seconds": 120, "host_bytes": HOST_BYTES,
        "command": command, "request": request, "image": image, "build": build, "build_log": log_pin,
        "execution_user_sid": base.SID, "failure": None,
    }

    def authenticate(api, job, info, child_deadline):
        proof = parent.suspended_authentication(api, info, deadline, child_deadline, base.SID,
                                                image, owner.image_proof)
        require(component["token_owner"](info.process) == base.SID,
                "suspended native default owner differs")
        require(owner.job_snapshot(api, job) == ([int(info.pid)], 1, 1),
                "the suspended native image must be the only Job member")
        proof["default_owner_sid"] = base.SID
        return proof

    try:
        env = {key: value for key, value in os.environ.items()
               if not key.upper().startswith(("AIRK_", "ORT_", "PYTHON", "CUDA_", "CUDNN_"))}
        env.update(
            PATH=r"C:\Windows\System32;C:\Windows",
            TEMP=str(temp), TMP=str(temp), TMPDIR=str(temp),
            AIRK_PRIVATE_CENSUS_BASE=str(temp),
            AIRK_CUDA_QUALIFICATION_INPUT=str(args.request),
            AIRK_CUDA_QUALIFICATION_INPUT_SHA256=REQUEST_SHA,
            AIRK_CUDA_QUALIFICATION_OUTPUT=str(native_path),
        )
        with stdin_path.open("xb"):
            pass
        with stdin_path.open("rb") as stdin, stdout_path.open("xb") as stdout:
            guard(base_guard, [stdin_path, stdout_path], True, deadline=deadline)
            owned = owner.run_windows_owned_training(
                command, ROOT, stdout, deadline - time.monotonic(), env, stdin, HOST_BYTES,
                before_resume=authenticate, absolute_deadline=deadline,
            )
        receipt["owned"] = owned
        outputs = [stdout_path, *temp.rglob("*")]
        if native_path.exists():
            outputs.append(native_path)
        guard(base_guard, outputs, True, deadline=deadline)
        receipt["stdout"] = base.pin(stdout_path)
        if native_path.exists():
            receipt["native_result"] = base.pin(native_path)
        require(parent.settled(owned) and 0 < owned["peak_host_committed_bytes"] <= HOST_BYTES,
                "native graph process did not pass natural one-child Job settlement")
        native = base.read(native_path)
        require(native["schema"] == "airk.private.cuda-graph-metadata-diagnostic/1"
                and native["profile"] == PROFILE and native["native_binary_sha256"] == image["sha256"]
                and native["pid"] == owned["owned_leader_pid"]
                and native["execution_user"] == native["default_owner"] == base.SID
                and native["outcome"] == "graph_metadata_observed" and native["diagnostic_complete"] is True
                and native["model_requests_dispatched"] is False and native["qualification"] is False
                and native["native_host_charge_release_observed"] is True
                and native["actual_job"] == {
                    "process_memory_limit_bytes": HOST_BYTES, "job_memory_limit_bytes": HOST_BYTES,
                    "flags": 0x2308,
                }, "native Large graph identity, scope or actual Job differs")
        component["validate_graph_metadata"](native["graph_metadata"], native["placement"])
        for pin in (request, image, build, log_pin, receipt["native_result"], receipt["stdout"]):
            base.pinned(pin)
        require(time.monotonic() < deadline, "original Large graph deadline expired")
        receipt["complete"] = True
    except BaseException as error:
        receipt["failure"] = f"{type(error).__name__}: {error}"
        if hasattr(error, "emergency_cleanup"):
            receipt["emergency_cleanup"] = error.emergency_cleanup
        raise
    finally:
        receipt["elapsed_seconds"] = time.monotonic() - start
        with receipt_path.open("x", encoding="utf-8") as stream:
            json.dump(receipt, stream, ensure_ascii=False, indent=2)
            stream.write("\n")
        guard(base_guard, [receipt_path], True)


if __name__ == "__main__":
    main()
