# SPDX-License-Identifier: Apache-2.0
"""Describe how the scores of permuted Context inputs follow the slot or the content. Fits nothing.

This is a development description. It qualifies nothing. See README.md in this folder.
"""
import argparse
from collections import Counter
import json
from pathlib import Path
import sys

sys.path.insert(0, str(Path(__file__).resolve().parent))
from layout import best, centred, effects, follow, full_sources, order, pin, require

SCHEMA = "airk.private.context-slot-permutation-report/1"
SCORES = "airk.private.context-slot-scores/1"
TABLE = "airk.private.context-slot-table/1"
# The forwards of a family that together show every body at every slot once: the identity and all rotations.
DESIGNS = {"cards": "cards_rot", "reads": "reads_rot"}


REPEAT_TOLERANCE = 0.001


def merged_rows(receipts):
    """Rows of several score receipts joined by ordinal, each variant once.

    Runs over the same rows may repeat a variant, such as the identity. A repeat must have the same sources and
    scores within a small tolerance, and the first one is kept.
    """
    merged = {}
    for receipt in receipts:
        for row in receipt["rows"]:
            known = merged.setdefault(row["ordinal"], {})
            for variant in row["variants"]:
                earlier = known.setdefault(variant["name"], variant)
                require(earlier["sources"] == variant["sources"] and len(earlier["scores"]) == len(variant["scores"])
                        and all(abs(a - b) <= REPEAT_TOLERANCE for a, b in zip(earlier["scores"], variant["scores"])),
                        "a repeated variant differs between score receipts: " + variant["name"])
    return [{"ordinal": ordinal, "variants": list(variants.values())} for ordinal, variants in sorted(merged.items())]


def outcome(identity, variant, kinds):
    """Labels of one permuted forward against the identity forward of its row.

    `kinds` gives the option kind of every original slot. A slot is a symbol, and `sources` names the body each
    slot shows. In the `reads` family the options do not move, and a body is the read that a read card names.
    In the `letters` family a body is a whole option line, so `content` there means that the choice stayed on
    the same line and text, and `slot` that it stayed on the same symbol.
    """
    count = len(identity)
    sources = full_sources(variant["sources"], count)
    reads = [slot for slot in range(count) if kinds[slot] == "read"]
    shown = [slot for slot in range(count) if kinds[sources[slot]] == "read"]
    before, after = order(identity)[0], order(variant["scores"])[0]
    read_before, read_after = best(identity, reads), best(variant["scores"], shown)
    return {"winner": follow(before, after, sources), "winner_slot": after, "winner_body": sources[after],
            "winner_kind": kinds[sources[after]], "winner_kind_before": kinds[before],
            "best_read": follow(read_before, read_after, sources), "best_read_slot": read_after,
            "best_read_body": sources[read_after], "original_read_slot_still_shows_a_read": read_before in shown}


def counted(outcomes):
    """Counts of the outcome labels of one variant or one family over its rows."""
    def tally(key):
        return dict(sorted(Counter(item[key] for item in outcomes).items()))

    return {"forwards": len(outcomes), "winner": tally("winner"), "best_read": tally("best_read"),
            "winner_slot": tally("winner_slot"), "winner_body": tally("winner_body"),
            "winner_kind": tally("winner_kind"),
            "winner_kind_changed": sum(item["winner_kind"] != item["winner_kind_before"] for item in outcomes),
            "best_read_slot": tally("best_read_slot"), "best_read_body": tally("best_read_body"),
            "original_read_slot_still_shows_a_read": sum(item["original_read_slot_still_shows_a_read"]
                                                         for item in outcomes)}


def design(rows, family, kinds):
    """The slot and body split of one family's rotation design, and what each row prefers once slots average out.

    For `cards` the slots are all options. For `reads` they are the read slots only, and a body is a read.
    """
    prefix = DESIGNS[family]
    width = len(kinds) if family == "cards" else sum(kind == "read" for kind in kinds)
    forwards, cells = [], {}
    for row in rows:
        chosen = [variant for variant in row["variants"]
                  if variant["family"] == "identity" or variant["name"].startswith(prefix)]
        if len(chosen) != width:
            continue
        for variant in chosen:
            sources = variant["sources"][:width]
            scores = variant["scores"][:width]
            forwards.append((row["ordinal"], sources, scores))
            for slot, value in enumerate(centred(scores)):
                cells.setdefault((slot, sources[slot]), []).append(value)
    if not forwards:
        return None
    split = effects(forwards)
    preferred = {row: order(values) for row, values in split.pop("row_body").items()}
    split["cell_mean_by_slot_then_body"] = [[sum(cells[(slot, body)]) / len(cells[(slot, body)])
                                             for body in range(width)] for slot in range(width)]
    reads = [slot for slot in range(width) if kinds[slot] == "read"]
    split["slot_averaged_first_body"] = dict(sorted(Counter(ranked[0] for ranked in preferred.values()).items()))
    split["slot_averaged_first_read"] = dict(sorted(Counter(
        next(body for body in ranked if body in reads) for ranked in preferred.values()).items()))
    return split, preferred


def certified_ranks(table_rows, scored, preferred, kinds):
    """For every scored row with a certificate, the rank of each certified read among the read cards.

    `original` ranks the read cards by the unpermuted scores. Each design ranks the read bodies by their scores
    averaged over the slots, so that a preference for a slot no longer counts.
    """
    reads = [slot for slot in range(len(kinds)) if kinds[slot] == "read"]
    out = []
    for ordinal, identity in sorted(scored.items()):
        slots = table_rows[ordinal]["certified_slots"]
        if not slots:
            continue
        entry = {"ordinal": ordinal, "certified_slots": slots,
                 "original": [[slot for slot in order(identity) if slot in reads].index(slot) + 1 for slot in slots]}
        for family, ranking in preferred.items():
            if ordinal in ranking:
                entry[family + "_slot_averaged"] = [[body for body in ranking[ordinal] if body in reads].index(slot) + 1
                                                    for slot in slots]
        out.append(entry)
    return out


def main():
    parser = argparse.ArgumentParser(description=__doc__.splitlines()[0])
    parser.add_argument("--scores", action="append", required=True, type=Path,
                        help="scores.json from score_variants.py; repeat for runs over other rows")
    parser.add_argument("--table", required=True, type=Path, help="the slot table of the same panel")
    parser.add_argument("--output", required=True, type=Path, help="a new file for the report")
    args = parser.parse_args()
    require(sys.flags.isolated and sys.dont_write_bytecode, "use Python -I -B")
    table = json.loads(args.table.read_bytes())
    require(table.get("schema") == TABLE, "unexpected table schema")
    table_rows = {row["ordinal"]: row for row in table["rows"]}
    receipts = [json.loads(path.read_bytes()) for path in args.scores]
    require(all(doc.get("schema") == SCORES and doc.get("complete") is True for doc in receipts),
            "a completed score receipt is required")
    rows = merged_rows(receipts)
    kinds = table_rows[rows[0]["ordinal"]]["option_kinds"]
    require(all(table_rows[row["ordinal"]]["option_kinds"] == kinds for row in rows),
            "the scored rows do not share one option layout")
    by_variant, by_family, scored = {}, {}, {}
    for row in rows:
        identity = next(variant["scores"] for variant in row["variants"] if variant["family"] == "identity")
        scored[row["ordinal"]] = identity
        for variant in row["variants"]:
            if variant["family"] == "identity":
                continue
            labels = outcome(identity, variant, kinds)
            by_variant.setdefault(variant["name"], []).append(labels)
            by_family.setdefault(variant["family"], []).append(labels)
    designs, preferred = {}, {}
    for family in DESIGNS:
        result = design(rows, family, kinds)
        if result:
            designs[family], preferred[family] = result
    certified = certified_ranks(table_rows, scored, preferred, kinds)
    receipt = {"schema": SCHEMA, "qualification": False, "scores": [pin(path) for path in args.scores],
               "table": pin(args.table), "rows": len(rows), "ordinals": sorted(scored), "option_kinds": kinds,
               "original_winner_slot": dict(sorted(Counter(order(scores)[0] for scores in scored.values()).items())),
               "variants": {name: counted(items) for name, items in by_variant.items()},
               "families": {name: counted(items) for name, items in by_family.items()},
               "designs": designs, "certified": certified}
    with args.output.open("x", encoding="utf-8") as file:
        json.dump(receipt, file, indent=1, allow_nan=False)
    print(json.dumps({key: receipt[key] for key in ("rows", "original_winner_slot", "families", "designs",
                                                    "certified")}))


if __name__ == "__main__":
    main()
