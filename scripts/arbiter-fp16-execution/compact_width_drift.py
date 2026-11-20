# SPDX-License-Identifier: Apache-2.0
"""Report per-card drift on the four frozen Compact scientific width controls."""
import hashlib
import json
import math
from pathlib import Path
import runpy
from statistics import median

HERE = Path(__file__).resolve().parent
CAMPAIGN = HERE.parents[1] / "artifacts/v190/final-plan/exec-20261116T052942Z"
TIMING = runpy.run_path(str(HERE / "compact_cutlass_timing.py"))
NUMERICAL = runpy.run_path(str(HERE / "probe_compact_cutlass.py"))["numerical"]
GRAPH = "6142500349b0c9e72ef97e6c1f06ab5a81eb519942fe35d936a9d29020542581"
UPPER = "d9e4abed37173d97ce98aef07615be70dc1fa3d735ee666e5bff467883ac8986"


def mass(scores):
    maximum = max(scores)
    weights = [math.exp(x - maximum) for x in scores]
    total = sum(weights)
    return [x / total for x in weights]


def distribution(values):
    ordered = sorted(values)
    return {"count": len(ordered), "median": median(ordered),
            "p95": ordered[math.ceil(.95 * len(ordered)) - 1], "maximum": ordered[-1]}


def deltas(a, b):
    return [abs(x - y) for x, y in zip(a, b, strict=True)]


def summarize():
    checks = dict(TIMING["RECEIPTS"])
    checks[8192] = UPPER
    report = {"widths": {}, "all_cards": {}, "percentile": "nearest_rank",
              "complete_answer_pairs": 0, "active_rule_examined": False}
    pooled = {role: {"raw": [], "mass": []} for role in ("matching", "original")}
    for width in TIMING["WIDTHS"]:
        path = CAMPAIGN / f"compact-cutlass-width-score-{width}-r1/execution.json"
        raw = path.read_bytes()
        if hashlib.sha256(raw).hexdigest() != checks[width]:
            raise ValueError(f"Different original width receipt at {width}")
        receipt = json.loads(raw)
        native = receipt["result"]
        owned = receipt["owned"]
        n = receipt["numerical"]
        candidate = native["score"]["raw_scores"]
        if not (receipt["complete"] and receipt["mode"] == "score" and receipt["case"] == str(width)
                and native["model_sha256"] == TIMING["CANDIDATE"] and native["typed_graph_sha256"] == GRAPH
                and native["score"]["forward_synchronized"] and len(candidate) == 8
                and owned["exit_code"] == 0 and owned["owned_descendants_settled"]
                and owned["owned_job_final_active_processes"] == 0 and not owned["owned_job_forced_termination"]
                and n == NUMERICAL(candidate, n["matching_scores"], n["original_scores"])):
            raise ValueError(f"Invalid closed native score or reference at {width}")
        row = {"receipt_sha256": checks[width], "candidate_winner": candidate.index(max(candidate))}
        for role in pooled:
            reference = n[role + "_scores"]
            raw_error = deltas(candidate, reference)
            mass_error = deltas(mass(candidate), mass(reference))
            top, next_best = sorted(reference, reverse=True)[:2]
            row[role] = {"raw": distribution(raw_error), "mass": distribution(mass_error),
                         "reference_winner": reference.index(top), "reference_top_two_gap": top - next_best}
            pooled[role]["raw"].extend(raw_error)
            pooled[role]["mass"].extend(mass_error)
        report["widths"][str(width)] = row
    report["all_cards"] = {role: {key: distribution(values) for key, values in data.items()}
                           for role, data in pooled.items()}
    return report


if __name__ == "__main__":
    print(json.dumps(summarize(), sort_keys=True, allow_nan=False))
