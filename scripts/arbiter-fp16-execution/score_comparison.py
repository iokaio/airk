# SPDX-License-Identifier: Apache-2.0
"""Compare finite raw score vectors, normalized action mass and the selected action."""
import math


def compare(scores, reference):
    if not scores or len(reference) != len(scores) or not all(math.isfinite(v) for v in (*scores, *reference)):
        raise ValueError("finite complete numerical vectors required")

    def mass(values):
        maximum = max(values)
        exponentials = [math.exp(value - maximum) for value in values]
        total = sum(exponentials)
        return [value / total for value in exponentials]

    return {
        "max_raw_abs": max(abs(x - y) for x, y in zip(scores, reference, strict=True)),
        "max_mass_abs": max(abs(x - y) for x, y in zip(mass(scores), mass(reference), strict=True)),
        "winner_agrees": scores.index(max(scores)) == reference.index(max(reference)),
    }


def numerical(result, matching, original):
    m = compare(result, matching)
    o = compare(result, original)
    mass_and_winner = m["winner_agrees"] and m["max_mass_abs"] <= 0.05 and o["max_mass_abs"] <= 0.05
    return {
        "matching": m,
        "original": o,
        "release_numerical_case_pass": mass_and_winner and m["max_raw_abs"] <= 0.007,
        "development_numerical_case_pass": mass_and_winner and m["max_raw_abs"] <= 0.07,
        "candidate_scores": result,
        "matching_scores": matching,
        "original_scores": original,
    }
