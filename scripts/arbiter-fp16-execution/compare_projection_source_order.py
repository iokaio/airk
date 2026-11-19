# SPDX-License-Identifier: Apache-2.0
"""Compare the retained Compact T32 diagnostic vectors after independent execution."""

import argparse
import json
import math
from pathlib import Path
import runpy
import struct
import time


VERIFIER = Path(__file__).with_name("verify_compact.py")
CAMPAIGN = Path(__file__).resolve().parents[2] / "artifacts/v190/final-plan/exec-20261116T052942Z"
STAGE_SHA = "87ec206416eda607ff1585809d085e37fc4f8b99a5fc029308b5c78218cefcc2"
BASELINE = CAMPAIGN / "compact-fp16-qkv-fp32-reference-r1/compact-qkv-fp32-t32-reference.json"
BASELINE_SHA = "304d9b16922ce88f9db181c1e1956d9c0aa99ab29c5ef4dff26f2c37f5b0fa28"
CUDA = CAMPAIGN / "compact-fp16-qkv-fp32-native-t32-r2/native-result.json"
CUDA_SHA = "217431438d1bf79bbffa071c9d69d62f4dcb73c44cdee458c19cc50a544d774f"
REFERENCE_SCHEMA = "airk.private.compact-fp32-qkv-source-order-t32-reference/1"


def require(condition, message):
    if not condition:
        raise ValueError(message)


def binary32(values):
    require(type(values) is list and len(values) == 8
            and all(type(value) in (int, float) and math.isfinite(value) for value in values),
            "eight finite Compact scores required")
    return struct.pack("<8f", *values)


def score_delta(actual, expected):
    def mass(values):
        peak = max(values)
        weights = [math.exp(value - peak) for value in values]
        total = sum(weights)
        return [value / total for value in weights]

    raw = max(abs(a - b) for a, b in zip(actual, expected, strict=True))
    normalized = max(abs(a - b) for a, b in zip(mass(actual), mass(expected), strict=True))
    return {"max_raw_error": raw, "max_mass_error": normalized,
            "matching_winner": actual.index(max(actual)) == expected.index(max(expected))}


def compare(reference, expected):
    require(reference.parent.parent == CAMPAIGN, "reference must be a direct Compact campaign result")
    deadline = time.monotonic() + 10

    def check():
        if time.monotonic() >= deadline:
            raise TimeoutError("retained score comparison expired")

    read = runpy.run_path(str(VERIFIER))["read_pinned_json"]
    diagnostic, diagnostic_pin = read(reference, expected, check)
    baseline, baseline_pin = read(BASELINE, BASELINE_SHA, check)
    cuda, cuda_pin = read(CUDA, CUDA_SHA, check)
    recipe = diagnostic["recipe"]
    observation = diagnostic["source_order_observation"]
    expected_norms = [
        {"shape": [1, 32, 2048], "calls": 57},
        {"shape": [1, 32, 16, 128], "calls": 28},
        {"shape": [1, 32, 8, 128], "calls": 28},
    ]
    require(diagnostic["schema"] == REFERENCE_SCHEMA and diagnostic["complete"] is True
            and diagnostic["reference_device"] == "cpu" and diagnostic["torch_forward_invocations"] == 1
            and diagnostic["native_execution"] is diagnostic["qualification"] is diagnostic["product_admission"] is False
            and baseline["stage_receipt"] == diagnostic["stage_receipt"]
            and diagnostic["stage_receipt"]["sha256"] == STAGE_SHA
            and diagnostic["candidate"] == baseline["candidate"]
            and diagnostic["reference"]["input"] == baseline["reference"]["input"]
            and recipe["qkv_fp32_routes"] == baseline["recipe"]["qkv_fp32_routes"]
            and recipe["context_flash_single_tile"] is True
            and observation["rms"] == expected_norms and observation["quickgelu_calls"] == 28,
            "independently completed source-order Compact proof required")
    require(cuda["schema"] == "airk.private.compact-fp32-qkv-t32-component/1"
            and cuda["native_forward_returned"] is cuda["forward_synchronized"] is True
            and cuda["numeric_forward_attempts"] == 1
            and cuda["projection_reference"]["sha256"] == BASELINE_SHA
            and cuda["source_mask"]["verified_candidate_graph"] == baseline["candidate"]["graph"]["sha256"]
            and len(cuda["rows"]) == 1
            and cuda["rows"][0]["input_digest"] == diagnostic["reference"]["input"]["input_digest"],
            "retained synchronized Compact CUDA vector differs")
    original = binary32(baseline["reference"]["original_fp32"])
    require(binary32(diagnostic["reference"]["original_fp32"]) == original,
            "original independent FP32 score words differ")
    def to_scores(values):
        return struct.unpack("<8f", binary32(values))

    actual = to_scores(cuda["rows"][0]["raw_scores"])
    ordinary = to_scores(baseline["reference"]["matching_qkv_fp32"])
    ordered = to_scores(diagnostic["reference"]["matching_qkv_fp32"])
    result = {
        "schema": "airk.private.compact-fp32-qkv-source-order-comparison/1",
        "qualification": False, "product_admission": False, "diagnostic_reference": diagnostic_pin,
        "baseline_reference": baseline_pin, "native_observation": cuda_pin,
        "cuda_vs_baseline": score_delta(actual, ordinary),
        "cuda_vs_source_order": score_delta(actual, ordered),
        "source_order_vs_baseline": score_delta(ordered, ordinary),
        "cuda_vs_original_fp32": score_delta(actual, struct.unpack("<8f", original)),
    }
    check()
    return result


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("--reference", type=Path, required=True)
    parser.add_argument("--reference-sha256", required=True)
    args = parser.parse_args()
    print(json.dumps(compare(args.reference, args.reference_sha256), allow_nan=False))


if __name__ == "__main__":
    main()
