# SPDX-License-Identifier: Apache-2.0
"""Bind original protected-attention CPU scoring to exposed TRAIN or literal-width controls."""
import argparse
import hashlib
import json
import math
from pathlib import Path
import runpy
import sys
import time
from types import SimpleNamespace

HERE = Path(__file__).resolve().parent
CUTLASS_SHA = "9df92e6393079f61b6c2ccd13792afe243d4b3078fc2a6effc04c6651039e46f"
SUFFIX_SHA = "03f2170fa7112396bfb861ea486763371b2ca8707d4bb2e060d72af902d21e59"
TRAIN = "1ca4ef30499c75dac97c8a047988e03aaaec7d8e774f7ba82c965ca721aaecd1"
CAPTURE = "50ed40cec62a39b0067515b9aab2cd97c2849ef547dad149c49d86a32a863912"
FOCUS = "94c65d0c598b0b110cd25ddeb1c167ae730430197c0d5e6a59d79ee226c17c64"
NARROW = "304d9b16922ce88f9db181c1e1956d9c0aa99ab29c5ef4dff26f2c37f5b0fa28"
WIDE = "f0d0e01fa28e7a876f1c57bc9894dff7e2a915ac8bf8150885b2aac2fdff2f14"
CASES = {"TRAIN_Q543": TRAIN, "CONTROL_T32": NARROW, **{f"CONTROL_T{n}": WIDE for n in (128, 1024, 8192)}}


def require(ok, message):
    if not ok:
        raise ValueError(message)


def bind(base, legacy, case, seal, ordinal, check):
    require(CASES.get(case) == seal and ordinal == 1, "original source and ordinal required")
    if case == "TRAIN_Q543":
        receipt, pin = legacy.read_pinned_json(base.CAMPAIGN / "compact-train-focus1024-reference-q543-r1/execution.json", seal, check)
        owner = receipt["owned"]
        case_row, = receipt["result"]["rows"]
        admission, = case_row["admissions"]
        require(receipt["complete"] and owner["exit_code"] == 0 and owner["owned_descendants_settled"]
                and owner["owned_job_final_active_processes"] == 0 and owner["owned_job_final_member_pids"] == []
                and not owner["owned_job_forced_termination"] and not owner["timed_out"] and case_row["case"] == case
                and admission["ordinal"] == 1 and admission["state"] == "scored", "original TRAIN source was not settled")
        path = base.CAMPAIGN / "compact-development-TRAIN_Q543-focus1024-run-r1/fixture/context-cal-admissions/record-0001.json"
        record, capture = legacy.read_pinned_json(path, CAPTURE, check)
        typed, = [t for t in record["compiler_census"] if t["model"] == base.COMPACT]
        req = record["request"]
        row = typed["compiled"]
        require(capture == admission["capture"] and row == admission["compiled"] and typed["profile"] == base.CAPTURE_PROFILE
                and typed["NN_invocations"] == 0 and typed["fits_remaining_allowance"] and record["actual_Focus_forward_observed"]
                and record["actual_Focus_profile"] == admission["capture_focus"] == FOCUS and not record["Context_NN_dispatch"]
                and record["actual_Context_profile"]["manifest"] == admission["actual_context"] and record["ordinal_in_private_run"] == 1
                and all(req[k] == admission[k] for k in ("roster", "scope", "remaining_input_tokens", "parent_budget"))
                and req["choices"] == admission["offered_choices"] and record["remaining_ns_at_observation"] == admission["remaining_ns"]
                and len(row["input_ids"]) == typed["input_tokens"] == 1588 <= req["remaining_input_tokens"]
                and sum(row["option_mask"]) == len(req["roster"]["cards"]) == 7, "Focus1024 original input or action binding differs")
        evidence = {k: admission[k] for k in ("roster", "scope", "remaining_input_tokens", "parent_budget",
                                               "offered_choices", "remaining_ns", "actual_context", "capture_focus", "original_fp32")}
    else:
        width = int(case.removeprefix("CONTROL_T"))
        if width == 32:
            path = base.CAMPAIGN / "compact-fp16-qkv-fp32-reference-r1/compact-qkv-fp32-t32-reference.json"
            receipt, pin = legacy.read_pinned_json(path, seal, check)
            source = receipt["reference"]
        else:
            path = base.CAMPAIGN / "compact-fp16-qkv-fp32-wider-reference-r1/compact-qkv-fp32-wider-reference.json"
            receipt, pin = legacy.read_pinned_json(path, seal, check)
            source = receipt["references"][(128, 1024, 8192).index(width)]
        row = source["input"]
        require(len(row["input_ids"]) == width and len(row["option_mask"]) == 8, "original width or native mask differs")
        capture = pin
        evidence = {k: source[k] for k in ("producer", "fixture", "name", "compiler", "resource_policy", "original_fp32")}
    require(len(evidence["original_fp32"]) == len(row["option_mask"]) and all(map(math.isfinite, evidence["original_fp32"])),
            "original publisher scores differ")
    return (case, pin, capture, evidence, row)


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    modes = parser.add_mutually_exclusive_group(required=True)
    modes.add_argument("--output", type=Path)
    modes.add_argument("--worker-deadline", type=float)
    parser.add_argument("--case", required=True, choices=tuple(CASES))
    parser.add_argument("--census-sha256", required=True)
    parser.add_argument("--ordinal", type=int, required=True)
    parser.add_argument("--cal-pid", type=int)
    parser.add_argument("--source-sha256")
    args = parser.parse_args()
    require(sys.platform == "win32" and sys.flags.isolated and sys.flags.no_site and sys.dont_write_bytecode,
            "isolated checked Windows Python required")
    require(args.ordinal == 1 and CASES[args.case] == args.census_sha256, "original case and ordinal required")
    for name, seal in (("compact_cutlass_reference.py", CUTLASS_SHA), ("replay_compact_t1024_suffix.py", SUFFIX_SHA)):
        require(hashlib.sha256((HERE / name).read_bytes()).hexdigest() == seal, "numerical source or owner changed")
    cutlass = runpy.run_path(str(HERE / "compact_cutlass_reference.py"))
    full = HERE / "compact_full_projection_reference.py"
    require(hashlib.sha256(full.read_bytes()).hexdigest() == cutlass["FULL_SOURCE"], "original projection source differs")
    base = runpy.run_path(str(full))["load_base"]()
    replay = base.owner.module("replay_layer0.py", base.owner.REPLAY_SHA)
    wider = base.owner.module("verify_projection_candidate_t32.py", replay.WIDER_REFERENCE_SHA)
    if args.output:
        base.owner.run_owned(replay, wider, args.output, representative=(args.case, args.census_sha256, 1),
                             cutlass_projection=True, full_cuts_cal_pid=args.cal_pid)
        return
    require(replay.file_pin(Path(__file__).resolve())["sha256"] == args.source_sha256 and
            0 < args.worker_deadline - time.monotonic() <= 600, "original worker source and deadline required")
    evidence = {}
    def inputs(legacy, case, seal, ordinal, check):
        result = bind(base, legacy, case, seal, ordinal, check)
        evidence.update(result[3])
        return result
    bound = SimpleNamespace(owner=base.owner, CAMPAIGN=base.CAMPAIGN, inputs=inputs)
    result = cutlass["worker"](args, bound, replay, wider, bound_entry=True)
    result.update(original_binding=evidence, numerical_source_sha256=CUTLASS_SHA, suffix_sha256=SUFFIX_SHA)
    print(base.owner.MARKER + json.dumps(result, allow_nan=False))


if __name__ == "__main__":
    main()
