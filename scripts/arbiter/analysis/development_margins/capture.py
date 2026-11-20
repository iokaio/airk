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
    SHARED: "cb2116b3322f1eba8da8943de1253211c771bf2c90401a9994108115629ae1fe",
    HERE / "capture_inputs.py": "461bc8b81fc13d604ceccc086cd6e9a9339968d7a474fa4bf206c7ea5d176248",
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
    parser.add_argument("--output", type=Path)
    parser.add_argument("--check-only", action="store_true")
    parser.add_argument("--worker", action="store_true", help=argparse.SUPPRESS)
    args = parser.parse_args()
    pilot, inputs, zero = (checked(path) for path in OWNERS)
    require, pin = pilot["require"], pilot["pin"]
    case_id = args.case
    require(re.fullmatch("TRAIN_Q[0-9]{3}", case_id), "invalid development case")
    roster = inputs["load"](inputs["INPUTS"] / "roster.private.json", ROSTER)
    row, = [entry for entry in roster if entry["case"] == case_id]

    def validate():
        descriptor = inputs["validate_one"](case_id)
        require(inputs["digest"](inputs["encode"](descriptor)) == row["descriptor_sha256"], "case roster changed")
        return descriptor

    def observed_zero(answer, request, binding, admissions, summary, dispatches):
        zero["verify_empty_journal"](summary, dispatches)
        return zero["zero_disposition"](answer, request, binding, admissions)

    capture = pilot["SourceCapture"](inputs["INPUTS"] / case_id, row["descriptor_sha256"], validate,
                                     "TechQA TRAIN development " + case_id, observed_zero)
    pin(pilot["NATIVE"], pilot["NATIVE_SHA"])
    pin(pilot["ROOT"] / "scripts/provision-static-onnx-runtime.py", pilot["OWNER_SHA"])
    native = runpy.run_path(str(pilot["NATIVE"]))
    owner = native["owner_module"]()
    if args.check_only:
        require(args.output is None and not args.worker, "check-only takes no output")
        validate()
        print(json.dumps({"case": case_id, "descriptor_sha256": row["descriptor_sha256"], "native_started": False}))
        return

    output = owner.plain(args.output)
    require(output.parent == pilot["CAMPAIGN"] and re.fullmatch("compact-development-" + case_id + "-run-r[1-9][0-9]*",
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
                                           "--output", output, "--worker"], pilot["ROOT"], output / "outer.log",
                                          pilot["LIMIT"], os.environ | {"AIRK_DEVELOPMENT_CAPTURE_OWNER": str(output)})
    result["source_sha256"] = before
    result["support_sha256"] = {path.name: sha for path, sha in OWNERS.items()}
    result["source_seals_unchanged"] = (pin(handler) == before and pin(pilot["NATIVE"]) == pilot["NATIVE_SHA"]
                                        and all(pin(path) == sha for path, sha in OWNERS.items()))
    native["write_json"](output / "owned.json", result)
    require(owner.successful(result) and result["source_seals_unchanged"] and result["owned_job_final_active_processes"] == 0
            and result["owned_job_final_member_pids"] == [] and (output / "capture.private.json").is_file(),
            "development capture did not settle")


if __name__ == "__main__":
    main()
