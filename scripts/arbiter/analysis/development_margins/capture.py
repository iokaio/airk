# SPDX-License-Identifier: Apache-2.0
"""Observe one frozen TRAIN development request with the owned Windows fixture."""
import argparse
import hashlib
import json
import os
from pathlib import Path
import re
import runpy
import sys


HERE = Path(__file__).resolve().parent
SHARED = HERE.parent / "public_source_admission/run_techqa_predispatch_pilot.py"
OWNERS = {
    SHARED: "873d95e610c46d7241932d9a049ca035d71eb0cfce1ce9ed8db77a3d613d19b1",
    HERE / "capture_inputs.py": "461bc8b81fc13d604ceccc086cd6e9a9339968d7a474fa4bf206c7ea5d176248",
    HERE / "engineering_inputs.py": "6c7ac6ff48cd0a82e037aef1336bba882116ea9d2d0b694e4e033e989ffe74d5",
    HERE / "focus1024.py": "428db7bb0e22f96bb6e67b3c9f24276cd9cdca4be92563a05881702a3aecf211",
    HERE / "zero_capture.py": "5e1bffd57937225a89bcf762705c2fa70f08d975fbd2377d99d075b3a1c102e3",
}
ROSTER = "586f4d2d22c2cc4daef0c6b71bef5035f8165f9f8bde546a093d08d474049d67"


def checked(module):
    if hashlib.sha256(module.read_bytes()).hexdigest() != OWNERS[module]:
        raise ValueError("capture code identity changed: " + module.name)
    return runpy.run_path(str(module))


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("--case", required=True)
    parser.add_argument("--focus-width", type=int, choices=(512, 1024), default=512)
    parser.add_argument("--output", type=Path)
    parser.add_argument("--check-only", action="store_true")
    parser.add_argument("--worker", action="store_true", help=argparse.SUPPRESS)
    args = parser.parse_args()
    pilot, inputs, engineering, focus1024, zero = (checked(path) for path in OWNERS)
    require, pin = pilot["require"], pilot["pin"]
    case_id = args.case
    source_pins = dict(OWNERS)
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
    if args.check_only:
        require(args.output is None and not args.worker, "check-only takes no output")
        validate()
        print(json.dumps({"case": case_id, "descriptor_sha256": expected, "native_started": False}))
        return

    output = owner.plain(args.output)
    require(output.parent == pilot["CAMPAIGN"] and re.fullmatch("compact-development-" + case_id + variant + "-run-r[1-9][0-9]*",
                                                                output.name), "fresh private case target required")
    handler = Path(__file__).resolve()
    if args.worker:
        require(os.environ.get("AIRK_DEVELOPMENT_CAPTURE_OWNER") == str(output), "original case Job required")
        with native["capped_worker_job"](output):
            pilot["execute"](native, owner, output, capture)
        return
    require(not output.exists(), "fresh output required")
    validate()
    output.mkdir()
    before = pin(handler)
    result = owner.run_evaluation_fixture([sys.executable, "-I", "-B", "-S", handler, "--case", case_id,
                                           "--focus-width", str(args.focus_width), "--output", output, "--worker"], pilot["ROOT"], output / "outer.log",
                                          pilot["LIMIT"], os.environ | {"AIRK_DEVELOPMENT_CAPTURE_OWNER": str(output)})
    result["source_sha256"] = before
    result["focus_width"] = args.focus_width
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
