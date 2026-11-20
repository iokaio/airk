# SPDX-License-Identifier: Apache-2.0
"""Observe one frozen TRAIN development request with the owned Windows fixture."""
import argparse
import hashlib
import json
import os
from pathlib import Path
import re
import runpy
import shutil
import sys


HERE = Path(__file__).resolve().parent
SHARED = HERE.parent / "public_source_admission/run_techqa_predispatch_pilot.py"
OWNERS = {
    SHARED: "240084bdb6affb292ad8926e4196776f6c9e0823ec9ad068dc64bb3a5fb8ca6d",
    HERE / "capture_inputs.py": "461bc8b81fc13d604ceccc086cd6e9a9339968d7a474fa4bf206c7ea5d176248",
    HERE / "engineering_inputs.py": "6c7ac6ff48cd0a82e037aef1336bba882116ea9d2d0b694e4e033e989ffe74d5",
    HERE / "focus1024.py": "428db7bb0e22f96bb6e67b3c9f24276cd9cdca4be92563a05881702a3aecf211",
    HERE / "zero_capture.py": "5e1bffd57937225a89bcf762705c2fa70f08d975fbd2377d99d075b3a1c102e3",
}
REUSE_SHA = "0dde43a83353c4302ddc83a21496ad966c753b028552461b6e87b5bef5864135"
ROSTER = "586f4d2d22c2cc4daef0c6b71bef5035f8165f9f8bde546a093d08d474049d67"


def checked(module):
    if hashlib.sha256(module.read_bytes()).hexdigest() != OWNERS[module]:
        raise ValueError("capture code identity changed: " + module.name)
    return runpy.run_path(str(module))


def storage_for_run(descriptor, inputs, parent):
    source = descriptor["runtime_inputs"]["inputs"]
    roster = inputs["load"](Path(source["path"]), source["sha256"])
    copies = 0
    for row in roster:
        profile = inputs["load"](Path(row["profile"]), row["profile_sha256"])
        copies += sum(asset["bytes"] for asset in profile["assets"])
    # The native graph-overlay guard requires these physical bytes after the
    # independent model copies. Q564's failed setup reports both values.
    needed = copies + (4 << 30) + (16 << 20)
    free = shutil.disk_usage(parent).free
    return {"independent_asset_bytes": copies, "minimum_bytes": needed, "available_bytes": free}


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("--case", required=True)
    parser.add_argument("--focus-width", type=int, choices=(512, 1024), default=512)
    parser.add_argument("--output", type=Path)
    parser.add_argument("--check-only", action="store_true")
    parser.add_argument("--reuse-after", type=Path)
    parser.add_argument("--reuse-sha256")
    parser.add_argument("--worker", action="store_true", help=argparse.SUPPRESS)
    args = parser.parse_args()
    pilot, inputs, engineering, focus1024, zero = (checked(path) for path in OWNERS)
    require, pin = pilot["require"], pilot["pin"]
    case_id = args.case
    source_pins = dict(OWNERS)
    retry_helper = pilot["FROZEN_RETRY"] if pilot["FROZEN_RETRY"].is_file() else SHARED.parent / "native_cohort_source_reads.py"
    source_pins[retry_helper] = pilot["RETRY_SHA"]
    pin(retry_helper, pilot["RETRY_SHA"])
    active_fd = case_id in ("FD_1394", "FD_1162")
    if active_fd:
        require(args.focus_width == 1024, "the two fd issue inputs use the active Focus profile")
        source = SHARED.parent / "contextbench_fd_issue_inputs.py"
        source_pins[source] = "c26a319ddbb1bc80a7c6f6ae4d41e4b4651291411d7ecd3a0b923051d2aeaf48"
        pin(source, source_pins[source])
        fd = runpy.run_path(str(source))
        key, expected = fd["STAGED_SHA"][case_id]
        directory = fd["STAGED"] / key
        read_input = lambda: fd["validate_staged"](case_id)
        source_name = "ContextBench exposed repository " + key
    elif case_id == engineering["CASE"]:
        directory = engineering["OUTPUT"]
        expected = "3dd3af9254b59c84792afce8ea6040c26a2789b7d78fb037b09ae7ed26c4860f"
        read_input = engineering["validate_one"]
        source_name = "Fixed synthetic engineering development"
    elif case_id == "DEV_Q002_EXPOSED":
        require(args.focus_width == 1024, "reuse the original 512 pilot receipt")
        directory = pilot["INPUT"]
        expected = pilot["DESCRIPTOR"]
        read_input = pilot["validate_inputs"]
        source_name = pilot["PILOT"].source_name
    else:
        require(re.fullmatch("TRAIN_Q[0-9]{3}", case_id), "invalid development case")
        roster = inputs["load"](inputs["INPUTS"] / "roster.private.json", ROSTER)
        row, = [entry for entry in roster if entry["case"] == case_id]
        directory = inputs["INPUTS"] / case_id
        expected = row["descriptor_sha256"]
        read_input = lambda: inputs["validate_one"](case_id)
        source_name = "TechQA TRAIN development " + case_id

    original_sha = expected

    def validate():
        descriptor = read_input()
        require(inputs["digest"](inputs["encode"](descriptor)) == original_sha, "case roster changed")
        if args.focus_width == 1024 and not active_fd:
            return focus1024["bind"](descriptor, original_sha)
        return descriptor

    variant = ""
    if args.focus_width == 1024:
        expected = inputs["digest"](inputs["encode"](validate()))
        variant = "-focus1024"

    def observed_zero(answer, request, binding, admissions, summary, dispatches):
        zero["verify_empty_journal"](summary, dispatches)
        return zero["zero_disposition"](answer, request, binding, admissions)

    capture = pilot["SourceCapture"](directory, expected, validate, source_name, observed_zero)
    pin(pilot["NATIVE"], pilot["NATIVE_SHA"])
    pin(pilot["ROOT"] / "scripts/provision-static-onnx-runtime.py", pilot["OWNER_SHA"])
    native = runpy.run_path(str(pilot["NATIVE"]))
    owner = native["owner_module"]()
    reuse_path = HERE / "reuse_installation.py"
    pin(reuse_path, REUSE_SHA)
    source_pins[reuse_path] = REUSE_SHA
    reuse = runpy.run_path(str(reuse_path))
    if args.focus_width == 1024:
        base, ready, immutable = reuse["inventory"](validate())
        if args.reuse_after is not None:
            previous_case = {"TRAIN_Q564": reuse["R4"] / "outer-owned.json",
                             "TRAIN_Q598": reuse["E"] / "compact-development-TRAIN_Q564-focus1024-run-r3/owned.json"}
            require(args.reuse_after.parent.parent == reuse["E"]
                    and args.reuse_after == previous_case.get(case_id, args.reuse_after),
                    "shared-control predecessor differs from original case order")
            require(isinstance(args.reuse_sha256, str) and re.fullmatch(r"[0-9a-f]{64}", args.reuse_sha256),
                    "verified predecessor receipt SHA required")
            restart, immutable = reuse["restart_inputs"](validate(), args.reuse_after, args.reuse_sha256)
        else:
            require(args.check_only, "static-1024 captures must reopen the existing physical installation")
    else:
        require(args.reuse_after is None and args.reuse_sha256 is None, "width mismatch with shared installation")
    if args.check_only:
        require(args.output is None and not args.worker, "check-only takes no output")
        validate()
        print(json.dumps({"case": case_id, "descriptor_sha256": expected, "native_started": False}))
        return

    output = owner.plain(args.output)
    require(output.parent == pilot["CAMPAIGN"] and output.parent.resolve(strict=True) == output.parent
            and re.fullmatch("compact-development-" + case_id + variant + "-run-r[1-9][0-9]*",
                                                                output.name), "fresh private case target required")
    handler = Path(__file__).resolve()
    if args.worker:
        require(os.environ.get("AIRK_DEVELOPMENT_CAPTURE_OWNER") == str(output), "original case Job required")
        with native["capped_worker_job"](output):
            shared = (restart, reuse["CONTROL"]) if args.focus_width == 1024 else None
            pilot["execute"](native, owner, output, capture, shared)
        return
    require(not output.exists(), "fresh output required")
    if args.focus_width == 1024:
        mutable_bytes = sum(p.stat().st_size for p in reuse["CONTROL"].rglob("*") if p.is_file()
                            and p.relative_to(reuse["CONTROL"]).as_posix() not in immutable)
        storage = {"independent_asset_bytes": 0, "control_backup_bytes": mutable_bytes,
                   "minimum_bytes": (4 << 30) + (16 << 20) + mutable_bytes,
                   "available_bytes": shutil.disk_usage(output.parent).free}
    else:
        storage = storage_for_run(validate(), inputs, output.parent)
    print(json.dumps({"output_storage": storage}), flush=True)
    require(storage["available_bytes"] >= storage["minimum_bytes"], "insufficient space for independent copies and native overlay")
    output.mkdir()
    before = pin(handler)
    extra = []
    if args.focus_width == 1024:
        require(os.environ.get("AIRK_TRAIN_REUSE_AUDITED_CONTROL") == str(reuse["CONTROL"]),
                "shared control requires the checked owner wrapper")
        prior_backup = reuse["custody"](output, immutable, True)
        extra = ["--reuse-after", args.reuse_after, "--reuse-sha256", args.reuse_sha256]
    result = owner.run_evaluation_fixture([sys.executable, "-I", "-B", "-S", handler, "--case", case_id,
                                           "--focus-width", str(args.focus_width), "--output", output, "--worker", *extra], pilot["ROOT"], output / "outer.log",
                                          pilot["LIMIT"], os.environ | {"AIRK_DEVELOPMENT_CAPTURE_OWNER": str(output)})
    if args.focus_width == 1024:
        after_backup = reuse["custody"](output, immutable, False) if reuse["settled"](result) else None
        shared = {"anchor_plan_sha256": reuse["PLAN"], "control_root": str(reuse["CONTROL"]),
                  "previous": {"path": str(args.reuse_after), "sha256": args.reuse_sha256},
                  "control_before": prior_backup, "control_after": after_backup}
        for key, name in (("ready", "fixture/ready.json"), ("closed", "fixture/closed.json"),
                          ("status", "commands/reuse-final-status.stdout"),
                          ("restart_request", "restart-request.json"), ("saved", "fixture/restart-owner.json"),
                          ("answer_command", "commands/answer.command.json")):
            if (output / name).is_file():
                shared[key] = reuse["link"](output / name)
        result["shared_installation"] = shared
    result["source_sha256"] = before
    result["focus_width"] = args.focus_width
    result["initial_output_storage"] = storage
    result["capture_descriptor_sha256"] = expected
    result["support_sha256"] = {path.name: sha for path, sha in source_pins.items()}
    result["source_seals_unchanged"] = (pin(handler) == before and pin(pilot["NATIVE"]) == pilot["NATIVE_SHA"]
                                        and all(pin(path) == sha for path, sha in source_pins.items()))
    native["write_json"](output / "owned.json", result)
    require(owner.successful(result) and result["source_seals_unchanged"] and result["owned_job_final_active_processes"] == 0
            and result["owned_job_final_member_pids"] == [] and (output / "capture.private.json").is_file(),
            "development capture did not settle")


if __name__ == "__main__":
    main()
