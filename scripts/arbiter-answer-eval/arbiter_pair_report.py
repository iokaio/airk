# SPDX-License-Identifier: Apache-2.0
"""Summarize ``arbiter_pair_probe.py`` results and compare two of them.

Read-only. For one result it reports, per use and repository, how many right
candidates rank within the first ``k`` places (``k`` = 5 and the number of
right candidates in the group) and, for ``coverage``, which wrong candidate
outranked the whole passage. With ``--other`` it also reports how closely a
second result's ranking follows the first: the share of groups with the same
top candidate and the mean overlap of the first five places.

    python -B arbiter_pair_report.py --result <file> [--other <file>]
"""

from __future__ import annotations

import argparse
import json
import pathlib
import sys

TOP = 5


def ordered(group: dict) -> list[dict]:
    return sorted(group["candidates"], key=lambda candidate: (-candidate["score"], candidate["id"]))


def recall(groups: list[dict]) -> dict:
    """Per use and repository: right candidates, and how many rank in the first five and in the first R
    places, where R is the group's count of right candidates."""
    table: dict[str, dict] = {}
    for group in groups:
        if group["use"] == "coverage":
            continue
        repository = group["id"].split("-", 1)[0]
        ranking = ordered(group)
        right = sum(candidate["label"] for candidate in ranking)
        row = table.setdefault(f"{group['use']}/{repository}",
                               {"groups": 0, "right": 0, "candidates": 0, "in_top5": 0, "in_top_r": 0})
        row["groups"] += 1
        row["right"] += right
        row["candidates"] += len(ranking)
        row["in_top5"] += sum(candidate["label"] for candidate in ranking[:TOP])
        row["in_top_r"] += sum(candidate["label"] for candidate in ranking[:right])
    return table


def separation(groups: list[dict]) -> dict:
    """Per use: how well one threshold could split right from wrong scores across all groups. ``auc`` is the
    chance that a right candidate scores above a wrong one, counting ties as half. The two shares say how
    many right and wrong candidates score above 0.5."""
    table: dict[str, dict] = {}
    for group in groups:
        row = table.setdefault(group["use"], {"right_scores": [], "wrong_scores": []})
        for candidate in group["candidates"]:
            row["right_scores" if candidate["label"] else "wrong_scores"].append(candidate["score"])
    for row in table.values():
        right, wrong = row.pop("right_scores"), sorted(row.pop("wrong_scores"))
        wins = 0.0
        for value in right:
            below = _count_below(wrong, value)
            ties = _count_below(wrong, value, inclusive=True) - below
            wins += below + ties / 2
        row["right"], row["wrong"] = len(right), len(wrong)
        row["auc"] = round(wins / (len(right) * len(wrong)), 4) if right and wrong else None
        row["right_above_half"] = round(sum(v > 0.5 for v in right) / len(right), 4) if right else None
        row["wrong_above_half"] = round(sum(v > 0.5 for v in wrong) / len(wrong), 4) if wrong else None
    return table


def _count_below(ordered_values: list[float], value: float, inclusive: bool = False) -> int:
    import bisect
    return bisect.bisect_right(ordered_values, value) if inclusive else bisect.bisect_left(ordered_values, value)


def coverage_losses(groups: list[dict]) -> dict:
    """Which wrong candidate outranked the whole passage in coverage groups."""
    losses = {"groups": 0, "whole_first": 0, "first-part": 0, "other-note": 0}
    for group in groups:
        if group["use"] != "coverage":
            continue
        losses["groups"] += 1
        top = ordered(group)[0]
        losses["whole_first" if top["label"] else top["id"]] += 1
    return losses


def agreement(first: list[dict], second: list[dict]) -> dict:
    other = {group["id"]: group for group in second}
    table: dict[str, dict] = {}
    for group in first:
        if group["id"] not in other:
            raise ValueError(f"group {group['id']} is missing from the other result")
        a, b = ordered(group), ordered(other[group["id"]])
        if sorted(c["id"] for c in a) != sorted(c["id"] for c in b):
            raise ValueError(f"group {group['id']} has different candidates in the other result")
        row = table.setdefault(group["use"], {"groups": 0, "same_top": 0, "top5_overlap": 0.0, "max_score_delta": 0.0})
        row["groups"] += 1
        row["same_top"] += a[0]["id"] == b[0]["id"]
        depth = min(TOP, len(a))
        row["top5_overlap"] += len({c["id"] for c in a[:depth]} & {c["id"] for c in b[:depth]}) / depth
        scores = {c["id"]: c["score"] for c in b}
        row["max_score_delta"] = max(row["max_score_delta"], max(abs(c["score"] - scores[c["id"]]) for c in a))
    for row in table.values():
        row["same_top_share"] = round(row["same_top"] / row["groups"], 4)
        row["mean_top5_overlap"] = round(row.pop("top5_overlap") / row["groups"], 4)
        row["max_score_delta"] = round(row["max_score_delta"], 6)
    return table


def main(argv: list[str] | None = None) -> int:
    parser = argparse.ArgumentParser(description=__doc__.splitlines()[0])
    parser.add_argument("--result", required=True, type=pathlib.Path)
    parser.add_argument("--other", type=pathlib.Path)
    args = parser.parse_args(argv)
    first = json.loads(args.result.read_text(encoding="utf-8"))["groups"]
    report = {"recall": recall(first), "coverage": coverage_losses(first), "separation": separation(first)}
    if args.other is not None:
        second = json.loads(args.other.read_text(encoding="utf-8"))["groups"]
        report["other_recall"] = recall(second)
        report["agreement"] = agreement(first, second)
    print(json.dumps(report, indent=2, sort_keys=True))
    return 0


if __name__ == "__main__":
    sys.exit(main())
