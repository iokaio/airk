# SPDX-License-Identifier: Apache-2.0
"""Summarize the four sealed Windows Large direct-score populations offline."""
import argparse
import hashlib
import json
import math
import re
import statistics
from pathlib import Path

CASES = {
    32: ("r1", "599c990fe6907434b5c0df747b5de90e7af50e392ba475e707b7637063c41516"),
    128: ("r2", "b5a0d90cfa65aa67f446ffd81d4a273515450b43216ab69afe433d0c59397256"),
    1024: ("r1", "abc94e17345cb6a0fad96c7bc7ebb85413f8d7f8d2b95bcf54c3dc0f5e0b17b6"),
    8192: ("r1", "379748d72aa02d66e323fa3385e80dd96e3c67474d213dbf16cf46a1b4f27675"),
}
TYPED = "13da8267e7490e334b5a860ced90af8318a01f5c2d03759173feac45ca598b46"

def read(path, digest):
    raw = path.read_bytes()
    if hashlib.sha256(raw).hexdigest() != digest:
        raise ValueError(f"changed evidence {path}")
    return raw

def stats(values):
    ordered = sorted(values)
    return {"count": len(values), "max": max(values), "median": statistics.median(values),
            "p95_nearest_rank": ordered[math.ceil(0.95 * len(values)) - 1]}

def mass(values):
    maximum = max(values)
    terms = [math.exp(x - maximum) for x in values]
    return [x / sum(terms) for x in terms]

def summarize(campaign):
    rows = {}
    pooled = {}
    for width, (revision, digest) in CASES.items():
        p = campaign / f"windows-large-direct-fp32-score-t{width}-{revision}" / "execution.json"
        r = json.loads(read(p, digest))
        n = r["numerical"]
        if not (r["complete"] and n["release_numerical_case_pass"] and r["width"] == width
                and r["result"]["result"]["typed_graph_sha256"] == TYPED and r["owned"]["exit_code"] == 0
                and r["owned"]["owned_job_final_active_processes"] == 0 and not r["owned"]["timed_out"]):
            raise ValueError(f"wrong original native qualification input at {width}")
        logits = {k: n[k + "_scores"] for k in ("candidate", "matching", "original")}
        if any(len(v) != 8 or not all(map(math.isfinite, v)) for v in logits.values()):
            raise ValueError("eight finite original scores required")
        probabilities = {k: mass(v) for k, v in logits.items()}
        winners = {k: v.index(max(v)) for k, v in logits.items()}
        metrics = {}
        for role in ("matching", "original"):
            for kind, quantities in (("raw", logits), ("mass", probabilities)):
                errors = [abs(x - y) for x,y in zip(quantities["candidate"], quantities[role], strict=True)]
                if max(errors) != n[role]["max_" + kind + "_abs"]:
                    raise ValueError("original error summary differs")
                key = role + "_" + kind
                metrics[key] = stats(errors)
                pooled.setdefault(key, []).extend(errors)
        log = r["original_stdout"]
        name = p.with_name("stdout-stderr.log")
        if Path(log["path"]) != name:
            raise ValueError("wrong original log path")
        elapsed, = re.findall(rb"test result: ok\..*?finished in (\d+\.\d+)s", read(name, log["sha256"]))
        samples = r["owned"]["before_GO"]["wddm"]["samples"]
        peaks = {key: max(s["process"][key] for s in samples if s["process"][key] is not None)
                 for key in ("Dedicated Usage","Shared Usage","Total Committed")}
        rows[width] = {"execution_sha256": digest, "raw": logits, "mass": probabilities, "winner": winners,
                       "errors": metrics, "cold_native_test_seconds": float(elapsed),
                       "job_commit_peak_bytes": r["owned"]["peak_host_committed_bytes"],
                       "sampled_pid_wddm_peaks_bytes": peaks}
    return {"widths": rows, "pooled_errors": {k: stats(v) for k,v in pooled.items()},
            "p95_method": "nearest rank on absolute per-card differences",
            "latency_scope": "native test including load, placement, synchronized score and teardown"}

if __name__ == "__main__":
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("campaign", type=Path)
    parser.add_argument("output", type=Path)
    args = parser.parse_args()
    report = summarize(args.campaign.resolve(strict=True))
    with args.output.open("x", encoding="utf-8") as out:
        json.dump(report, out, indent=2, allow_nan=False)
        out.write("\n")
