# SPDX-License-Identifier: Apache-2.0
"""Join an owned Large CPU Flash vector to its unchanged native T32 score."""

import argparse
import hashlib
import json
import math
from pathlib import Path


def pin(path):
    with path.open("rb") as stream:
        return hashlib.file_digest(stream, "sha256").hexdigest()


def load_owned(path, digest, key):
    if pin(path) != digest:
        raise ValueError("execution receipt digest differs")
    rec = json.loads(path.read_bytes())
    if (rec["complete"] is not True or rec["whole_phase_expired"] or rec["owned"]["exit_code"] != 0
            or rec["owned"]["owned_job_final_active_processes"] != 0 or rec["owned"]["timed_out"]):
        raise ValueError("successful natural Job settlement required")
    out = Path(rec[key]["path"])
    if out.parent != path.parent or pin(out) != rec[key]["sha256"] or out.stat().st_size != rec[key]["bytes"]:
        raise ValueError("execution result digest differs")
    return rec[key], json.loads(out.read_bytes())


def accuracy(actual, expected):
    if len(actual) != 8 or len(expected) != 8 or not all(map(math.isfinite, actual + expected)):
        raise ValueError("eight finite raw scores required")
    def probabilities(vec):
        exps = [math.exp(x - max(vec)) for x in vec]
        return [x / sum(exps) for x in exps]
    return {"raw_error": max(abs(a-b) for a,b in zip(actual, expected)),
            "mass_error": max(abs(a-b) for a,b in zip(probabilities(actual), probabilities(expected))),
            "winner": actual.index(max(actual)), "expected_winner": expected.index(max(expected))}


def per_slot(actual, simple, flash, original):
    return [
        {"slot": slot, "native": native, "original_fp32": fp32, "simple_reference": baseline,
         "flash_reference": revised, "native_minus_flash": native - revised,
         "native_minus_original": native - fp32, "flash_minus_original": revised - fp32,
         "flash_minus_simple": revised - baseline, "matching_raw_limit_exceeded": abs(native - revised) > .007}
        for slot, (native, baseline, revised, fp32) in enumerate(zip(actual, simple, flash, original, strict=True))
    ]


def compare(native_receipt, native_sha, reference_receipt, reference_sha, *, include_slots=False):
    native, n = load_owned(native_receipt, native_sha, "native")
    reference, r = load_owned(reference_receipt, reference_sha, "proof")
    old_proof = n["independent_proof"]
    old_path = Path(old_proof["path"])
    if pin(old_path) != old_proof["sha256"]:
        raise ValueError("historical independent closure changed")
    old = json.loads(old_path.read_bytes())
    row, = n["rows"]
    if (row["tokens"] != 32 or r["recipe"] != "baseline-half-sm86-causal-flash-single-tile"
            or row["input_digest"] != r["input_digest"] or r["model"] != "large" or r["device"] != "cpu"
            or r["forward_count"] != 1 or any(old[key] != r[key] for key in ("graph", "weights", "conversion_plan", "conversion_receipt",
                    "source_inputs", "input_digest", "original_fp32", "publisher", "verification"))):
        raise ValueError("independent/native scientific identity differs")
    matching = accuracy(row["raw_scores"], r["matching_fp16"])
    original = accuracy(row["raw_scores"], r["original_fp32"])
    verdict = (matching["raw_error"] <= .007 and matching["mass_error"] <= .05 and original["mass_error"] <= .05
               and matching["winner"] == matching["expected_winner"] == original["expected_winner"])
    result = {"native": native, "reference": reference, "native_execution_sha256": native_sha,
            "reference_execution_sha256": reference_sha, "input_digest": r["input_digest"], "recipe": r["recipe"],
            "matching": matching, "original": original, "release_matching_raw_limit": .007,
            "t32_matching_comparison_pass": verdict, "qualification": False}
    if include_slots:
        result["historical_reference"] = old_proof
        result["slots"] = per_slot(row["raw_scores"], old["matching_fp16"], r["matching_fp16"], r["original_fp32"])
    return result


if __name__ == "__main__":
    parser = argparse.ArgumentParser(description=__doc__)
    for key in ("native", "reference"):
        parser.add_argument("--" + key, type=Path, required=True)
        parser.add_argument("--" + key + "-sha256", required=True)
    parser.add_argument("--per-slot", action="store_true")
    args = parser.parse_args()
    print(json.dumps(compare(args.native, args.native_sha256, args.reference, args.reference_sha256,
                             include_slots=args.per_slot), allow_nan=False))
