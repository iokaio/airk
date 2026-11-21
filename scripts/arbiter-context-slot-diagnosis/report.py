# SPDX-License-Identifier: Apache-2.0
"""Describe how the scores of permuted and probe Context inputs follow the slot or the content. Fits nothing.

This is a development description. It qualifies nothing. See README.md in this folder.
"""
import argparse
from collections import Counter
import json
from pathlib import Path
import statistics
import sys

sys.path.insert(0, str(Path(__file__).resolve().parent))
from layout import (PAIRS, best, centred, effects, follow, full_sources, order, pearson, pin, require, split_by_row,
                    spread)

SCHEMA = "airk.private.context-slot-permutation-report/2"
SCORES = "airk.private.context-slot-scores/1"
TABLE = "airk.private.context-slot-table/1"
# A rotation design is a base variant and all rotations of one family, which together show every body at every
# slot once. The last entry says whether the design covers the read slots only.
DESIGNS = {"cards": ("identity", "cards_rot", False), "reads": ("identity", "reads_rot", True),
           "stated": ("stated", "stated_reads_rot", True)}
# The variant that the forwards of a family are compared with. Every other family is compared with the identity.
BASES = {"stated_reads": "stated"}
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


def outcome(base, variant, kinds):
    """Labels of one forward against the base forward of its row.

    `kinds` gives the option kind of every original slot. A slot is a symbol, and `sources` names the body each
    slot shows. In the read families the options do not move, and a body is the read that a read card names.
    In the `letters` family a body is a whole option line, so `content` there means that the choice stayed on
    the same line and text, and `slot` that it stayed on the same symbol.
    """
    count = len(base)
    sources = full_sources(variant["sources"], count)
    reads = [slot for slot in range(count) if kinds[slot] == "read"]
    shown = [slot for slot in range(count) if kinds[sources[slot]] == "read"]
    before, after = order(base)[0], order(variant["scores"])[0]
    read_before, read_after = best(base, reads), best(variant["scores"], shown)
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
    """The slot and body split of one rotation design, and what each row prefers once slots average out.

    For `cards` the slots are all options. For `reads` and `stated` they are the read slots only, and a body is
    a read. A row without the whole design is left out. None when no row has it. Otherwise the split, each
    row's bodies ranked by their slot-averaged centred scores, and those scores themselves.
    """
    base, prefix, reads_only = DESIGNS[family]
    width = sum(kind == "read" for kind in kinds) if reads_only else len(kinds)
    forwards, cells = [], {}
    for row in rows:
        chosen = [variant for variant in row["variants"]
                  if variant["name"] == base or variant["name"].startswith(prefix)]
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
    averaged = split.pop("row_body")
    preferred = {row: order(values) for row, values in averaged.items()}
    split["cell_mean_by_slot_then_body"] = [[sum(cells[(slot, body)]) / len(cells[(slot, body)])
                                             for body in range(width)] for slot in range(width)]
    reads = [slot for slot in range(width) if kinds[slot] == "read"]
    split["slot_averaged_first_body"] = dict(sorted(Counter(ranked[0] for ranked in preferred.values()).items()))
    split["slot_averaged_first_read"] = dict(sorted(Counter(
        next(body for body in ranked if body in reads) for ranked in preferred.values()).items()))
    return split, preferred, averaged


def read_groups(averaged, table_rows):
    """Slot-averaged centred scores of the reads of a read design, grouped by read form and by certificate.

    `averaged` gives one value per read and row. A read that is preferred for what it is, whatever slot names
    it, has a high value. `certified` and `other` split the reads of the rows that have a certificate.
    """
    forms, certified, other = {}, [], []
    for ordinal, values in averaged.items():
        row = table_rows[ordinal]
        for read, value in enumerate(values):
            forms.setdefault(row["read_forms"][read], []).append(value)
            if row["certified_slots"]:
                (certified if read in row["certified_slots"] else other).append(value)
    return {"by_read_form": {form: spread(values) for form, values in sorted(forms.items())},
            "certified": spread(certified) if certified else None, "other": spread(other) if other else None}


def certified_ranks(table_rows, direct, preferred, kinds):
    """For every scored row with a certificate, the rank of each certified read among the read cards.

    `direct` names score vectors by ordinal, such as the unpermuted scores, and ranks the read cards by them.
    `preferred` names, per design, each row's bodies ranked by their scores averaged over the slots, so that a
    preference for a slot no longer counts. Its entries are reported as `<design>_slot_averaged`.
    """
    reads = [slot for slot in range(len(kinds)) if kinds[slot] == "read"]

    def ranks(ranked, slots):
        among = [slot for slot in ranked if slot in reads]
        return [among.index(slot) + 1 for slot in slots]

    out = []
    for ordinal in sorted(next(iter(direct.values()))):
        slots = table_rows[ordinal]["certified_slots"]
        if not slots:
            continue
        entry = {"ordinal": ordinal, "certified_slots": slots}
        for name, vectors in direct.items():
            if ordinal in vectors:
                entry[name] = ranks(order(vectors[ordinal]), slots)
        for family, ranking in preferred.items():
            if ordinal in ranking:
                entry[family + "_slot_averaged"] = ranks(ranking[ordinal], slots)
        out.append(entry)
    return out


def rank_summary(entries, keys):
    """Per rank list name, the count of certified reads, how many rank first, the mean rank and the rank counts."""
    out = {}
    for key in keys:
        ranks = [rank for entry in entries for rank in entry.get(key, [])]
        if ranks:
            out[key] = {"certified_reads": len(ranks), "ranked_first": ranks.count(1),
                        "mean_rank": statistics.fmean(ranks), "rank_counts": dict(sorted(Counter(ranks).items()))}
    return out


def pair_table(row):
    """The read-minus-sufficient score differences of one row's two-option forwards.

    One entry per read, in read order: [difference with the read card first, difference with it second]. None
    when the row has no two-option forward. A row with only part of them is refused.
    """
    found = {}
    for variant in row["variants"]:
        if variant["family"] in PAIRS:
            require(len(variant["scores"]) == 2 and len(variant["sources"]) == 1, "a pair has two scores and one read")
            first, second = variant["scores"]
            swapped = variant["family"] == "pair_swapped"
            pair = found.setdefault(variant["sources"][0], [None, None])
            pair[swapped] = second - first if swapped else first - second
    require(sorted(found) == list(range(len(found))) and all(None not in pair for pair in found.values()),
            "a row lacks a read or one of its two orders")
    return [found[read] for read in range(len(found))] or None


def pair_summary(tables, table_rows, native, kinds):
    """What the two-option forwards say: does the read-minus-sufficient difference depend on the read or the order?

    `tables` gives `pair_table` by ordinal and `native` the unpermuted full-roster scores by ordinal.
    `order_averaged` is the mean of a read's two differences, in which a preference for the first or the second
    option cancels. `first_option_advantage` is half the gap between them: how much being listed first adds.
    """
    ordinals = sorted(tables)
    first = [[pair[0] for pair in tables[ordinal]] for ordinal in ordinals]
    second = [[pair[1] for pair in tables[ordinal]] for ordinal in ordinals]
    mean = [[(pair[0] + pair[1]) / 2 for pair in tables[ordinal]] for ordinal in ordinals]
    pairs = [pair for ordinal in ordinals for pair in tables[ordinal]]
    reads = len(first[0])
    read_slots = [slot for slot in range(len(kinds)) if kinds[slot] == "read"]
    # A certified slot is used as a read number below, which holds when the read cards come first.
    require(read_slots == list(range(reads)), "the read cards are not the first slots, one per read")
    enough = kinds.index("sufficient")
    by_form = {}
    for ordinal, values in zip(ordinals, mean):
        for read, value in enumerate(values):
            by_form.setdefault(table_rows[ordinal]["read_forms"][read], []).append(value)
    certified = []
    for ordinal, a, b, c in zip(ordinals, first, second, mean):
        slots = table_rows[ordinal]["certified_slots"]
        if slots:
            certified.append({"ordinal": ordinal, "certified_slots": slots,
                              "order_averaged": [order(c).index(slot) + 1 for slot in slots],
                              "read_first": [order(a).index(slot) + 1 for slot in slots],
                              "read_second": [order(b).index(slot) + 1 for slot in slots]})
    return {
        "rows": len(ordinals), "reads": reads,
        "read_first": split_by_row(first) | {"values": spread([value for row in first for value in row])},
        "read_second": split_by_row(second) | {"values": spread([value for row in second for value in row])},
        "order_averaged": split_by_row(mean) | {"values": spread([value for row in mean for value in row])},
        "first_option_advantage": spread([(pair[0] - pair[1]) / 2 for pair in pairs]),
        "read_chosen": {"pairs": len(pairs), "read_first": sum(pair[0] > 0 for pair in pairs),
                        "read_second": sum(pair[1] > 0 for pair in pairs),
                        "in_both_orders": sum(pair[0] > 0 and pair[1] > 0 for pair in pairs),
                        "in_neither_order": sum(pair[0] <= 0 and pair[1] <= 0 for pair in pairs),
                        "order_decides": sum((pair[0] > 0) != (pair[1] > 0) for pair in pairs)},
        "order_averaged_by_read": [statistics.fmean(row[read] for row in mean) for read in range(reads)],
        "order_averaged_by_read_form": {form: spread(values) for form, values in sorted(by_form.items())},
        "largest_read_by_row": dict(sorted(Counter(order(row)[0] for row in mean).items())),
        "row_mean_against_native_read_minus_sufficient": pearson(
            [statistics.fmean(row) for row in mean],
            [max(native[ordinal][slot] for slot in read_slots) - native[ordinal][enough] for ordinal in ordinals]),
        "certified": certified,
        "certified_summary": rank_summary(certified, ("order_averaged", "read_first", "read_second"))}


def gathered(rows, kinds):
    """Outcome labels by variant and by family, the named base score vectors and the pair tables, by ordinal.

    A forward is labelled against the base of its family: the `stated` variant for `stated_reads`, and the
    identity for every other family. The identity and the two-option forwards get no label.
    """
    by_variant, by_family, named, pairs = {}, {}, {}, {}
    for row in rows:
        scores = {variant["name"]: variant["scores"] for variant in row["variants"]}
        for name in ("identity", "stated"):
            if name in scores:
                named.setdefault(name, {})[row["ordinal"]] = scores[name]
        for variant in row["variants"]:
            if variant["family"] == "identity" or variant["family"] in PAIRS:
                continue
            labels = outcome(scores[BASES.get(variant["family"], "identity")], variant, kinds)
            by_variant.setdefault(variant["name"], []).append(labels)
            by_family.setdefault(variant["family"], []).append(labels)
        table = pair_table(row)
        if table:
            pairs[row["ordinal"]] = table
    return {"by_variant": by_variant, "by_family": by_family, "named": named, "pairs": pairs}


def main():
    parser = argparse.ArgumentParser(description=__doc__.splitlines()[0])
    parser.add_argument("--scores", action="append", required=True, type=Path,
                        help="scores.json from score_variants.py; repeat for further runs of the same model")
    parser.add_argument("--table", required=True, type=Path, help="the slot table of the same panel and model")
    parser.add_argument("--output", required=True, type=Path, help="a new file for the report")
    args = parser.parse_args()
    require(sys.flags.isolated and sys.dont_write_bytecode, "use Python -I -B")
    table = json.loads(args.table.read_bytes())
    require(table.get("schema") == TABLE, "unexpected table schema")
    table_rows = {row["ordinal"]: row for row in table["rows"]}
    receipts = [json.loads(path.read_bytes()) for path in args.scores]
    require(all(doc.get("schema") == SCORES and doc.get("complete") is True for doc in receipts)
            and len({doc["model"]["sha256"] for doc in receipts}) == 1,
            "completed score receipts of one model are required")
    rows = merged_rows(receipts)
    kinds = table_rows[rows[0]["ordinal"]]["option_kinds"]
    require(all(table_rows[row["ordinal"]]["option_kinds"] == kinds for row in rows),
            "the scored rows do not share one option layout")
    found = gathered(rows, kinds)
    by_variant, by_family, named, pairs = (found[key] for key in ("by_variant", "by_family", "named", "pairs"))
    designs, preferred = {}, {}
    for family in DESIGNS:
        result = design(rows, family, kinds)
        if result:
            designs[family], preferred[family], averaged = result
            if DESIGNS[family][2]:
                designs[family]["slot_averaged_reads"] = read_groups(averaged, table_rows)
    direct = {"original": named["identity"]} | ({"stated": named["stated"]} if "stated" in named else {})
    certified = certified_ranks(table_rows, direct, preferred, kinds)
    receipt = {"schema": SCHEMA, "qualification": False, "scores": [pin(path) for path in args.scores],
               "scored_by": [{key: doc.get(key) for key in ("provider", "model", "padding")} for doc in receipts],
               "table": pin(args.table), "rows": len(rows), "ordinals": [row["ordinal"] for row in rows],
               "option_kinds": kinds,
               "original_winner_slot": dict(sorted(Counter(
                   order(scores)[0] for scores in named["identity"].values()).items())),
               "variants": {name: counted(items) for name, items in by_variant.items()},
               "families": {name: counted(items) for name, items in by_family.items()},
               "designs": designs, "certified": certified,
               "certified_summary": rank_summary(certified, ["original", "stated"] + [
                   family + "_slot_averaged" for family in DESIGNS]),
               "pairs": pair_summary(pairs, table_rows, named["identity"], kinds) if pairs else None}
    with args.output.open("x", encoding="utf-8") as file:
        json.dump(receipt, file, indent=1, allow_nan=False)
    print(json.dumps({key: receipt[key] for key in ("rows", "original_winner_slot", "families", "designs",
                                                    "certified_summary", "pairs")}))


if __name__ == "__main__":
    main()
