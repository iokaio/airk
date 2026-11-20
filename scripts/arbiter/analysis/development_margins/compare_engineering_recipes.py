# SPDX-License-Identifier: Apache-2.0
"""Rejoin the original seven-option T1014 development observations across precision recipes."""
import itertools
import json
import math
from pathlib import Path
import runpy
from statistics import median


HERE = Path(__file__).resolve().parent
inputs = runpy.run_path(str(HERE / "capture_inputs.py"))
require = inputs["require"]
RUNS = {
    "compact-fp16-qkv-observed-engineering-r1": "1580381c761b10c56b80db0bb62bb0bd33d50c28c1eeaa62027520b0ec45474f",
    "compact-fp16-qkv-down-engineering-score-r1": "70c4ee8e78ca0f83492a54134f50d887ca9ec1a911dde3d3d7598b012a58868c",
    "compact-fp16-full-engineering-score-r1": "da06e20d2d7b05c780caadd7b10fa5da0319120471e0409395a1c3b877239753",
}


def mass(scores):
    weights = [math.exp(score - max(scores)) for score in scores]
    return [weight / sum(weights) for weight in weights]


def summary(values):
    absolute = sorted(abs(v) for v in values)
    return {"maximum": absolute[-1], "median": median(absolute),
            "nearest_rank_p95": absolute[math.ceil(.95 * len(absolute)) - 1]}


def order(a, b):
    return (a > b) - (a < b)


def compare(actual, reference, *, options=7):
    require(len(actual) == len(reference) == options and all(math.isfinite(s) for s in actual + reference),
            "complete finite option vectors required")
    delta = [a - r for a, r in zip(actual, reference, strict=True)]
    changed = [[i, j] for i, j in itertools.combinations(range(options), 2)
               if order(actual[i], actual[j]) != order(reference[i], reference[j])]
    return {"signed_raw_by_slot": delta, "raw": summary(delta),
            "normalized_mass": summary([a - r for a, r in zip(mass(actual), mass(reference), strict=True)]),
            "above_release_ceiling": [i for i, d in enumerate(delta) if abs(d) > .007],
            "changed_pair_order": changed, "reference_winner": reference.index(max(reference)),
            "native_winner": actual.index(max(actual))}


def main():
    previous = None
    output = {}
    for directory, epoch in RUNS.items():
        run = inputs["CAMPAIGN"] / directory
        receipt = inputs["load"](run / "execution.json", epoch)
        pin = receipt["outputs"]["native"]
        require(receipt["complete"] and not receipt["whole_phase_expired"]
                and Path(pin["path"]) == run / "native-result.json", "native observation did not settle")
        native = inputs["load"](Path(pin["path"]), pin["sha256"])
        row, = native["rows"]
        identity = (row["compiled"], row["roster"], row["original_fp32"])
        require(row["tokens"] == 1014 and row["compiled"]["option_mask"] == [True] * 7
                and (previous is None or identity == previous), "compiled input, offered choices or original FP32 changed")
        previous = identity
        results = {label: compare(row["raw_scores"], row[key]) for label, key in
                   (("matching", "matching_fp16"), ("original", "original_fp32"))}
        for label, comparison in results.items():
            reported = receipt["observed_comparison"][label]
            require(comparison["raw"]["maximum"] == reported["raw_error"] and
                    abs(comparison["normalized_mass"]["maximum"] - reported["mass_error"]) < 1e-12,
                    "recomputed score drift differs from the independent receipt")
        output[directory] = dict(results, execution_sha256=epoch, native_sha256=pin["sha256"])
    print(json.dumps({"requests": 1, "options_per_recipe": 7, "pairwise_orders_per_recipe": 21,
                      "paired_judged_requests": 0, "recipes": output}, sort_keys=True, allow_nan=False))


if __name__ == "__main__":
    main()
