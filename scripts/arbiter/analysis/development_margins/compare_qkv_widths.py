# SPDX-License-Identifier: Apache-2.0
"""Recover complete fixed-width score drift for the original Compact Q/K/V CUDA control."""
import json
from pathlib import Path
import runpy

HERE = Path(__file__).resolve().parent
scoring = runpy.run_path(str(HERE / "compare_engineering_recipes.py"))
inputs = scoring["inputs"]
require = inputs["require"]
RUNS = {
    "compact-fp16-qkv-fp32-native-t32-r2": "a479f953b5c13076d7c54388a89e257f6a18dc3cc5031a3bae9fa89e2b99e3ee",
    "compact-fp16-qkv-fp32-native-development-r1": "ebf0f2e1eadd7ff543479a254ed81240af4830a1cc9c1c78b1db69e5f1841ade",
}


def observe(directory, seal):
    run = inputs["CAMPAIGN"] / directory
    owner = inputs["load"](run / "execution.json", seal)
    short = owner["mode"] == "projection-t32"
    widths = [32] if short else [128, 1024, 8192]
    job = owner["owned"]
    require(owner["mode"] in ("projection-t32", "qkv-development") and owner["complete"] is (not short)
            and job["exit_code"] == (101 if short else 0) and not job["timed_out"]
            and not job["owned_job_forced_termination"] and job["owned_descendants_settled"]
            and job["owned_job_final_active_processes"] == 0 and job["owned_job_final_member_pids"] == []
            and not owner["whole_phase_expired"], "native run disposition differs")
    if short:
        require(owner["failure"]["message"] == "Compact FP16 T32 matching parity, original mass or time bound refused",
                "unexpected short-control failure")
        pin, = [item for item in owner["failure_outputs"] if Path(item["path"]) == run / "native-result.json"]
    else:
        pin = owner["outputs"]["native"]
    require(Path(pin["path"]) == run / "native-result.json", "foreign result")
    native = inputs["load"](Path(pin["path"]), pin["sha256"])
    ref = owner["projection_reference"]
    comparison = inputs["load"](Path(ref["path"]), ref["sha256"])
    require(comparison["candidate"]["graph"]["sha256"] == "e621cf6bffad1f0a27e3cc1ecba5b4379949295498f6585963620d71cfe4ff14"
            and comparison["candidate"]["weights"]["sha256"] == "0c4bd89d032e7dd821071c3559776cd3da50d54d84b6e575373fd8cf7003a7c3"
            and comparison["complete"] and native["forward_synchronized"] and not native["qualified"]
            and native["numeric_forward_attempts"] == len(widths), "candidate or forward population differs")
    reference_rows = [comparison["reference"]] if short else comparison["references"]
    rows = []
    for width, actual, expected in zip(widths, native["rows"], reference_rows, strict=True):
        compiled = expected["input"]
        require(width == actual["tokens"] == len(compiled["input_ids"])
                and actual["input_digest"] == compiled["input_digest"] and actual["name"] == expected["name"]
                and compiled["option_mask"] == [True] * 8, "original width or card identity changed")
        result = {}
        for label, column, reported in (("matching", "matching_qkv_fp32", "matching_fp16" if short else "matching_qkv"),
                                       ("original", "original_fp32", "original_fp32")):
            stats = scoring["compare"](actual["raw_scores"], expected[column], options=8)
            require(stats["raw"]["maximum"] == actual[reported + "_raw_error"]
                    and abs(stats["normalized_mass"]["maximum"] - actual[reported + "_mass_error"]) < 1e-12
                    and (stats["native_winner"] == stats["reference_winner"]) == actual[reported + "_argmax_matches"],
                    "recomputed drift or winner differs from the recorded observation")
            result[label] = stats
        rows.append(dict(result, tokens=width, options=8, input_digest=actual["input_digest"],
                         execution_sha256=seal, native_sha256=pin["sha256"], reference_sha256=ref["sha256"],
                         original_process_exit=job["exit_code"]))
    return rows


if __name__ == "__main__":
    rows = [row for directory, seal in RUNS.items() for row in observe(directory, seal)]
    print(json.dumps({"population": "fixed-width development controls", "rows": rows,
                      "judged_request_pairs": 0}, sort_keys=True, allow_nan=False))
