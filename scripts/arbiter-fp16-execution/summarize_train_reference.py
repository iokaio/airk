# SPDX-License-Identifier: Apache-2.0
"""Reconcile scored CPU shards to their frozen source cases; preserve unmeasured slots."""
import argparse
import hashlib
import json
from pathlib import Path
import runpy


HERE = Path(__file__).resolve().parent
batch = runpy.run_path(str(HERE / "compact_train_batch_reference.py"))
require = batch["require"]
CAMPAIGN = batch["CAMPAIGN"]


def read(path, expected):
    raw = path.read_bytes()
    require(hashlib.sha256(raw).hexdigest() == expected, "pinned receipt differs: " + path.name)
    return json.loads(raw)


def summarize(prefix_sha, receipts, final_sha=None, q543_sha=None):
    if q543_sha:
        require(len(receipts) == 4 and prefix_sha == batch["ORIGINAL_PREFIX_SHA"] and final_sha is None,
                "Q543 cutoff requires four original shards and the exact frozen prefix")
    require(0 < len(receipts) <= 5 and (len(receipts) < 5 or final_sha is not None), "ordered prefix of measured shards required")
    original = read(batch["PREFIX"], prefix_sha)
    cohort = read(batch["CENSUS"], final_sha) if final_sha else original
    require(original["roster_sha256"] == cohort["roster_sha256"] and len(cohort["rows"]) == 15
            and [r["case"] for r in original["rows"]] == [r["case"] for r in cohort["rows"]], "frozen cohort identity differs")
    rows = [{"case": row["case"], "capture_state": row["state"], "reference_state": "unmeasured"} for row in cohort["rows"]]
    counts = {reference: {key: 0 for key in ("scored", "within_0014", "within_0020", "within_0140")}
              for reference in ("matching_fp16", "original_fp32")}
    for number, seal in enumerate([*receipts, *([q543_sha] if q543_sha else [])], 1):
        cutoff = number == 5 and q543_sha is not None
        label = "q543" if cutoff else f"batch{number}"
        path = CAMPAIGN / f"compact-train-focus1024-reference-{label}-r1/execution.json"
        receipt = read(path, seal)
        run, result = receipt["owned"], receipt["result"]
        require(receipt["complete"] and run["exit_code"] == 0 and run["owned_descendants_settled"]
                and run["owned_job_final_active_processes"] == 0 and run["owned_job_final_member_pids"] == []
                and not run["timed_out"] and not run["owned_job_forced_termination"]
                and result["batch"] == number and len(result["rows"]) == (1 if cutoff else 3)
                and result["cohort"]["sha256"] == (prefix_sha if number <= 4 or cutoff else final_sha)
                and result["ragged_t65_constant"] and result["matching_t128_words_equal"] and result["original_t128_max_abs"] <= .001,
                "incomplete or mismatched numerical shard")
        for offset, observed in enumerate(result["rows"]):
            idx = (number - 1) * 3 + offset
            source = cohort["rows"][idx]
            require(observed["case"] == source["case"] and observed["case_evidence"] == source,
                    "case evidence changed between source prefix and final population")
            admissions = []
            for item in observed["admissions"]:
                entry = {"ordinal": item["ordinal"], "capture_sha256": item["capture"]["sha256"], "state": item["state"]}
                if item["state"] in ("scored", "reused_exact_reference"):
                    entry["tokens"] = len(item["compiled"]["input_ids"])
                    for reference, screen in (("matching_fp16", "matching_screen"), ("original_fp32", "original_screen")):
                        actual = batch["bounds"](item[reference], item["compiled"]["option_mask"])
                        require(actual == item[screen], "recorded margin differs from scores and legal options")
                        entry[screen] = actual
                        counts[reference]["scored"] += 1
                        for key in ("within_0014", "within_0020", "within_0140"):
                            counts[reference][key] += int(actual[key])
                admissions.append(entry)
            rows[idx].update(reference_state="shard_observed", admissions=admissions)
    return {"schema": "airk.private.compact-train-independent-margin-census/1",
            "roster_sha256": cohort["roster_sha256"], "prefix_sha256": prefix_sha, "final_sha256": final_sha,
            "receipts": receipts, **({"q543_cutoff_sha256": q543_sha} if q543_sha else {}), "rows": rows, "screen_counts": counts, "paired_task_outcomes": 0}


if __name__ == "__main__":
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("--prefix-sha256", required=True)
    parser.add_argument("--receipt-sha256", action="append", required=True)
    parser.add_argument("--final-sha256")
    parser.add_argument("--q543-sha256")
    args = parser.parse_args()
    print(json.dumps(summarize(args.prefix_sha256, args.receipt_sha256, args.final_sha256, args.q543_sha256), sort_keys=True, allow_nan=False))
