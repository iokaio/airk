# SPDX-License-Identifier: Apache-2.0
"""Measure the selected CPU4 Focus candidate against both completed 14-row references."""
import argparse
import json
import os
from pathlib import Path
import time

from maintained_helpers import owner, parent, private_root, worker, REPOSITORY
from run_reference import checked_output_guard

DIR = REPOSITORY / "artifacts/v190/final-plan/exec-20261116T052942Z/windows-focus1024-release-inputs-r1-retry"
PINS = {
    "airk_inference.exe": "dba73a2eccad3e0314fdb76a993938f6c1c2763f3aee9d7b0ca9c212a7cbae3f",
    "focus-resource-input-image.json": "7a3c9036ef39b2258c0c7dbc7207651c09c45caf35781e280be9af02c5e309bf",
    "resource-run-input.json": "794b4b827cdb03f8a7a2d18d300a3846b26bd992c4707b080061e3d97b64676a",
    "plan-same_artifact.json": "227f2a8c24176e95279fd20b3d5be9cc1680fab5e45a101a312b4b1fbab2cf39",
    "assembled/assembly.json": "ad78e826bee83d5b6cd473163055d6f9aea10700c2441e9b3d34b75d22f1fbad",
    "assembled/numeric-fixture.json": "d5d8474f3f4b9bd0a91c718e581d340e2d9b69989d86553546f865e4d9b1a382",
    "assembled/reference-producer.json": "f9147ec28f5fa88b94b3241d0b7f8b3c2d12e7612e2545fa121218e74ac232fe",
    "reference-original_fp32/execution.json": "56cef9045bb8127235a821d5eb92aa169939b80b50510fc18e9a2e0fe2a5840c",
    "reference-same_artifact/execution.json": "5500f956bff9c59ea17e16ab56e7a93895b319d573f71d61b29fd7b6612153ad",
}
WATCH = REPOSITORY / "scripts/arbiter/evaluation/research_history/composed_orchestration/native_operations_watchdog--7151a2f63a5c.py"
SELECTOR = "inference_engine::pool::arbiter::qualification::software_v2::software_v2_selected_native_numeric"


def verify(seals):
    for path, expected in seals.items():
        worker.pinned({"path": path, "sha256": expected})


def save(path, data):
    with path.open("x", encoding="utf8") as stream:
        json.dump(data, stream, indent=2)
        stream.write("\n")


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("--retained-build", type=Path)
    parser.add_argument("--inventory-sha256")
    parser.add_argument("--request-sha256")
    args = parser.parse_args()
    if any((args.retained_build, args.inventory_sha256, args.request_sha256)) and not all(
            (args.retained_build, args.inventory_sha256, args.request_sha256)):
        parser.error("retained build, inventory SHA and request SHA are required together")
    started = time.monotonic()
    deadline = started + 3600
    private_root(DIR)
    seals = {str(DIR / name): value for name, value in PINS.items()}
    seals[str(WATCH)] = "dd96844f8fc3a47c946b3286ea60fb3499df8e577720121dd1f74760d4a24c1f"
    load = lambda path: json.loads(path.read_bytes())
    work = DIR
    image = DIR / "airk_inference.exe"
    image_sha = PINS[image.name]
    request_path = DIR / "resource-run-input.json"
    build_sha = PINS["focus-resource-input-image.json"]
    if args.retained_build:
        work = args.retained_build.resolve(strict=True)
        private_root(work)
        if work.parent != DIR.parent:
            raise ValueError("expected the original campaign root")
        inventory_path = work / "native-image-inventory.json"
        verify({str(inventory_path): args.inventory_sha256})
        inventory = load(inventory_path)
        if inventory["schema"] != "airk.preserved-native-test-images/1":
            raise ValueError("unknown retained image inventory")
        [entry] = [item for item in inventory["images"] if item["binary_id"] == "airk-inference"]
        image, image_sha = Path(entry["path"]), entry["sha256"]
        build = inventory["actual_build_receipt"]
        build_sha = build["sha256"]
        request_path = work / "resource-run-input.json"
        for old_name in ("airk_inference.exe", "resource-run-input.json", "focus-resource-input-image.json"):
            del seals[str(DIR / old_name)]
        seals.update({str(image): image_sha, str(inventory_path): args.inventory_sha256,
                      str(request_path): args.request_sha256, build["path"]: build_sha})
    verify(seals)
    reference = load(DIR / "plan-same_artifact.json")
    sid = reference["human_sid"]
    if worker.windows_sid() != sid:
        raise ValueError("different Windows identity")
    powershell = Path("C:/Program Files/PowerShell/7/pwsh.exe")
    audit = checked_output_guard(reference, powershell)
    request = load(request_path)
    profile = load(worker.pinned(request["profile"]))
    manifest = load(worker.pinned(request["source_manifest"]))
    fixture = load(DIR / "assembled/numeric-fixture.json")
    if (manifest["native_build_receipt"]["sha256"] != build_sha
            or fixture["model"] != profile["model"] or len(fixture["rows"]) != 14
            or request["policy"]["focus_pair_input_tokens"] != 1024 or request["policy"]["deadline_ms"] != 120000
            or profile["memory_reservation_bytes"] != 4 << 30 or request["numeric_fixture"] is not None):
        raise ValueError("unexpected native request or independent fixture")
    for pin in [request["profile"], request["source_manifest"], manifest["native_build_receipt"], *manifest["source_files"]]:
        seals[pin["path"]] = pin["sha256"]
    for asset in profile["assets"]:
        seals[request["files"][asset["name"]]] = asset["sha256"]
    seals[str(Path(__file__).resolve())] = worker.digest(Path(__file__))
    verify(seals)
    out = work / "native-numeric"
    audit([out], False, deadline)
    out.mkdir()
    audit([out], True, deadline)
    input_path, raw, log_path = out / "numeric-input.json", out / "native.json", out / "native.log"
    journal, temp = raw.with_suffix(".operations.jsonl"), out / "temp"
    audit([input_path, raw, log_path, journal, temp, out / "stdin", out / "owned.json"], False, deadline)
    temp.mkdir()
    audit([temp], True, deadline)
    request["numeric_fixture"] = load(DIR / "assembled/assembly.json")["fixture"]
    worker.pinned(request["numeric_fixture"])
    save(input_path, request)
    audit([input_path], True, deadline)
    input_sha = worker.digest(input_path)
    seals[str(input_path)] = input_sha
    env = os.environ.copy()
    env.update(TMP=str(temp), TEMP=str(temp), CUDA_VISIBLE_DEVICES="", OMP_NUM_THREADS="4",
               MKL_NUM_THREADS="4", AIRK_SOFTWARE_V2_NATIVE_INPUT=str(input_path),
               AIRK_SOFTWARE_V2_NATIVE_INPUT_SHA256=input_sha, AIRK_SOFTWARE_V2_NATIVE_OUTPUT=str(raw))
    image_pin = {"path": str(image), "bytes": image.stat().st_size, "sha256": image_sha}
    roster = [{"kind": "load", "ordinal": 0, "input_digest": None}]
    roster.extend({"kind": "numeric", "ordinal": i, "input_digest": row["input"]["input_digest"]}
                  for i, row in enumerate(fixture["rows"], 1))
    watch_module = worker.load_module({"path": str(WATCH), "sha256": seals[str(WATCH)]}, "focus_native_operation_watch")
    watch = None
    def authenticate(api, job, info, child_deadline):
        nonlocal watch
        proof = parent.suspended_authentication(api, info, deadline, child_deadline, sid, image_pin, owner.image_proof)
        identity = {"input_sha256": input_sha, "profile": profile["manifest"], "process_id": int(info.pid),
                    "native_binary_sha256": image_pin["sha256"]}
        watch = watch_module.WindowsWatch(journal, log_path, identity, roster, 120000, True)
        return proof
    def observe(api, job, info, child_deadline):
        return watch.run(api, info, deadline)
    command = [str(image), "--ignored", "--exact", SELECTOR, "--nocapture", "--test-threads=1"]
    receipt = {"execution_user_sid": sid, "command": command, "input_seals": seals,
               "complete": False, "release_qualified": False, "failure": None}
    try:
        with (out / "stdin").open("xb") as stream:
            pass
        with (out / "stdin").open("rb") as stdin, log_path.open("xb") as log:
            receipt["owned"] = owner.run_windows_owned_training(command, REPOSITORY, log, deadline-time.monotonic(),
                env, stdin, 8 << 30, before_resume=authenticate, after_resume=observe, absolute_deadline=deadline)
        child = receipt["owned"]
        native = load(raw)
        receipt["native_sha256"] = worker.digest(raw)
        if not (child["exit_code"] == 0 and child["owned_descendants_settled"]
                and child["owned_job_final_active_processes"] == 0 and child["owned_job_final_member_pids"] == []
                and not child["owned_job_forced_termination"] and 0 < child["peak_host_committed_bytes"] <= 4 << 30
                and native["complete"] and native["process_id"] == child["owned_leader_pid"]
                and native["native_binary_sha256"] == image_pin["sha256"] and native["input_sha256"] == input_sha
                and len(native["rows"]) == 14):
            raise ValueError("native measurement or actual Job settlement refused")
        verify(seals)
        if time.monotonic() >= deadline:
            raise TimeoutError("original measurement phase expired")
        receipt["complete"] = True
    except BaseException as exc:
        receipt["failure"] = {"type": type(exc).__name__, "message": str(exc)}
        if hasattr(exc, "owned_cleanup"):
            receipt["owned_cleanup"] = exc.owned_cleanup
        raise
    finally:
        if watch is not None:
            watch.close()
        receipt["elapsed_seconds"] = time.monotonic()-started
        save(out / "owned.json", receipt)


if __name__ == "__main__":
    main()
