# SPDX-License-Identifier: Apache-2.0
"""Describe one completed native Compact Read study by ordinal, count and margin. Fits nothing."""
import argparse
from collections import Counter
import json
from pathlib import Path
import struct
import sys

HERE = Path(__file__).resolve().parent
sys.path.insert(0, str(HERE))
from r8_cutlass_study_inputs import pinned, read, require

PROXY = "generic_read_original_reference_complement"
TOLERATED = (0, 1, 2, 3, 5)


def f32(value):
    return struct.unpack("<f", struct.pack("<f", value))[0]


def generic_read(card):
    return (card["action"]["kind"] == "read" and card["gap"] == "unsupported"
            and card["reason"] == "additional_evidence_may_help"
            and all(unit["role"] == "unknown" and unit["relevance"] == "uncertain"
                    for unit in card["assessments"]))


def kind(card):
    name = card["action"]["kind"]
    if name == "read":
        return "Read" if generic_read(card) else "other"
    return {"sufficient": "Sufficient", "clarify": "Clarify"}.get(name, "other")


def ranked(scores):
    """Slots by descending single-precision score, and the top-two margin when the top is unique."""
    values = [f32(score) for score in scores]
    order = sorted(range(len(values)), key=lambda slot: (-values[slot], slot))
    unique = len(values) > 1 and values[order[0]] != values[order[1]]
    return order, f32(values[order[0]] - values[order[1]]) if unique else None


def tradeoff(winners, tolerated):
    """Most certified admissions with at most `tolerated` uncertified ones, then the greatest cutoff."""
    best = None
    for cutoff in sorted({margin for margin, certified in winners if certified}):
        admitted = [certified for margin, certified in winners if margin >= cutoff]
        counts = (sum(admitted), len(admitted) - sum(admitted))
        if counts[1] <= tolerated and (best is None or (counts[0], cutoff) > (best[0], best[2])):
            best = (*counts, cutoff)
    return ({"tolerated_uncertified": tolerated, "certified_admitted": 0, "uncertified_admitted": 0, "cutoff": None}
            if best is None else
            {"tolerated_uncertified": tolerated, "certified_admitted": best[0], "uncertified_admitted": best[1],
             "cutoff": best[2]})


def describe(packet, report):
    tops, top_slots, read_slots, certified_slots = Counter(), Counter(), Counter(), Counter()
    certified_rows, read_winners = [], []
    for ordinal, (raw, original) in enumerate(zip(packet["raw"]["rows"], packet["preflight"]["rows"], strict=True)):
        require(raw["id"] == original["id"] and raw["status"] == "raw_scores_only", "study input rows differ")
        cards = raw["roster"]["cards"]
        order, margin = ranked(raw["scores"])
        top = cards[order[0]]
        label = "tie" if margin is None else kind(top)
        tops[label] += 1
        top_slots[order[0]] += 1
        best_read = next((slot for slot in order if kind(cards[slot]) == "Read"), None)
        if best_read is not None:
            read_slots[best_read] += 1
        certified = {item["card"] for item in original["reference_proxies"]["valid_reference_proxy_whole_cards"]
                     if item["proxy"] == PROXY}
        winner_certified = top["id"] in certified
        if label == "Read":
            read_winners.append({"ordinal": ordinal, "margin": margin, "certified": winner_certified})
        if certified:
            slots = [slot for slot, card in enumerate(cards) if card["id"] in certified]
            certified_slots.update(slots)
            certified_rows.append({
                "ordinal": ordinal, "certified_cards": len(slots), "certified_slots": slots,
                "certified_ranks": sorted(order.index(slot) + 1 for slot in slots),
                "top_card": label, "top_slot": order[0], "top_two_margin": margin,
                "winner_is_certified": label == "Read" and winner_certified})
    winners = [(row["margin"], row["certified"]) for row in read_winners]
    tradeoffs = [tradeoff(winners, tolerated) for tolerated in TOLERATED]
    # The native owner decides. This description must agree with it before it is reported.
    native = report["minimum_raw_margin"]
    cutoff = tradeoffs[0]["cutoff"]
    require((native is None) == (cutoff is None) and (native is None or f32(native) == cutoff),
            "description disagrees with the native cutoff")
    admitted = {row["ordinal"] for row in read_winners if cutoff is not None and row["margin"] >= cutoff}
    require(admitted == {ordinal for ordinal, row in enumerate(report["outcomes"]) if row["card"] is not None},
            "description disagrees with the native admissions")
    read_winners.sort(key=lambda row: -row["margin"])
    return {
        "schema": "airk.private.cutlass-read-study-description/1", "descriptive_only": True,
        "frozen_rule": "most certified admissions with zero uncertified admissions, then the greatest such cutoff",
        "plan": report["plan"], "native_minimum_raw_margin": native,
        "native_outcome": "cutoff" if native is not None else "unsupported_no_positive_safe_margin",
        "rows": len(packet["raw"]["rows"]), "top_card_counts": dict(tops),
        "top_slot_counts": dict(sorted(top_slots.items())),
        "best_read_slot_counts": dict(sorted(read_slots.items())),
        "certified_slot_counts": dict(sorted(certified_slots.items())),
        "certified_rows": certified_rows,
        "certified_read_winners": [row for row in read_winners if row["certified"]],
        "uncertified_read_winners": [row for row in read_winners if not row["certified"]],
        "descriptive_tradeoffs_not_a_proposed_rule": tradeoffs,
    }


if __name__ == "__main__":
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("--execution", type=Path, required=True)
    parser.add_argument("--sha256", required=True)
    args = parser.parse_args()
    require(sys.flags.isolated and sys.flags.no_site and sys.dont_write_bytecode, "use checked Python -I -B -S")
    execution = read(pinned({"path": str(args.execution), "sha256": args.sha256}, "study execution"))
    require(execution["complete"] is True, "completed native study required")
    packet = read(pinned({"path": execution["source"], "sha256": execution["input_sha256"]}, "study input"))
    require(execution["report"]["input_sha256"] == execution["input_sha256"], "study report names another input")
    print(json.dumps(describe(packet, execution["report"]), allow_nan=False, separators=(",", ":")))
