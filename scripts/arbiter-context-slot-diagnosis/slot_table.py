# SPDX-License-Identifier: Apache-2.0
"""Tabulate a retained Context calibration panel by slot: card kinds, read forms, scores, ranks and certificates.

This is a development description. It fits nothing and qualifies nothing. See README.md in this folder.
"""
import argparse
from collections import Counter
import json
from pathlib import Path
import sys

sys.path.insert(0, str(Path(__file__).resolve().parent))
from layout import (body_kind, card_kind, card_lines, centred, order, parse_prompt, pin, profile_share, read_form,
                    require, spread, without_profile)

SCHEMA = "airk.private.context-slot-table/1"
PROXY = "generic_read_original_reference_complement"
RAW_PANEL = "airk.private.r8-compact-scientific-raw-panel/1"
SCORES = "airk.private.context-slot-scores/1"


def raw_rows(doc):
    """Scores and input digest by ordinal, from a retained raw panel or a score receipt of this folder.

    A score receipt gives the scores of its identity variant. It must be complete.
    """
    if doc.get("schema") == RAW_PANEL:
        return {row["ordinal"]: {"input_digest": row["input_digest"], "scores": row["scores"]} for row in doc["rows"]}
    require(doc.get("schema") == SCORES and doc.get("complete") is True,
            "scores must come from a raw panel or a completed score receipt")
    out = {}
    for row in doc["rows"]:
        identity = [variant["scores"] for variant in row["variants"] if variant["family"] == "identity"]
        require(len(identity) == 1 and row["ordinal"] not in out, "a row lacks one identity variant or is repeated")
        out[row["ordinal"]] = {"input_digest": row["input_digest"], "scores": identity[0]}
    return out


def static_checks(row, symbols, max_tokens):
    """Checks of one compiled row that need no tokenizer. Each value is True when the row is as the template says."""
    compiled, roster = row["compiled"], row["roster"]
    tokens = len(compiled["input_ids"])
    options = parse_prompt(compiled["prompt"])["options"]
    texts = [text for _, _, text in options]
    return {
        "options_equal_roster_cards": [[name, text] for _, name, text in options] == card_lines(roster),
        "option_texts_distinct": len(set(texts)) == len(texts),
        "one_option_per_card": len(options) == len(roster["cards"]) == len(compiled["candidate_positions"]),
        "candidates_are_the_symbols_in_order": compiled["candidate_positions"] == symbols[:len(options)],
        "no_option_masked": compiled["option_mask"] == [True] * len(options),
        "decision_is_the_last_token": compiled["decision_position"] == tokens - 1,
        "attention_is_all_ones": compiled["attention_mask"] == [1] * tokens,
        "positions_are_a_plain_range": compiled["position_ids"] == list(range(tokens)),
        "within_compiler_token_limit": tokens <= max_tokens,
        "within_remaining_allowance": tokens <= row["remaining_input_tokens"],
    }


def described(row, scores, original):
    """One panel row as slots, kinds, forms, scores and certificates. No id and no text is kept."""
    compiled, cards = row["compiled"], row["roster"]["cards"]
    parts = parse_prompt(compiled["prompt"])
    state = json.loads(parts["state"])
    choices = [card["action"].get("choice") for card in cards]
    ids = [card["id"] for card in cards]
    proxies = original["reference_proxies"]
    issued = [item for item in proxies["valid_reference_proxy_whole_cards"] if item["proxy"] == PROXY]
    require(all(choices[ids.index(item["card"])] == item["choice"] for item in issued),
            "a certificate names a card that is not the card of its read")
    certified = sorted(ids.index(item["card"]) for item in issued)
    progress = [None] * len(cards)
    for item in proxies["counterfactual_reference_progress"]:
        progress[choices.index(item["choice"])] = item["classification"]
    for item in proxies["counterfactual_unresolved"]:
        progress[choices.index(item["choice"]["id"])] = "UNRESOLVED"
    ranked = order(scores)
    return {"ordinal": row["ordinal"], "tokens": len(compiled["input_ids"]),
            "state_characters": len(parts["state"]), "evidence_units": len(state["indexed_evidence"]),
            "evidence_excerpt_bytes": [unit[4] - unit[3] for unit in state["indexed_evidence"]],
            "evidence_original_bytes": [unit[5] for unit in state["indexed_evidence"]],
            "read_choices_available": state["read_choices_available"],
            "read_choices_omitted": state["read_choices_omitted"],
            "card_kinds": [card_kind(card) for card in cards],
            "option_kinds": [body_kind(text) for _, _, text in parts["options"]],
            "read_forms": [read_form(read) for read in state["verified_reads"]],
            "scores": scores, "order": ranked, "margin": scores[ranked[0]] - scores[ranked[1]],
            "certified_slots": certified, "reference_progress": progress}


def tables(rows):
    count = len(rows[0]["scores"])
    slots = range(count)
    require(all(len(row["scores"]) == count for row in rows), "rows differ in option count")
    kinds = [Counter(row["card_kinds"][slot] for row in rows) for slot in slots]
    reads = max(len(row["read_forms"]) for row in rows)
    forms = [Counter(row["read_forms"][slot] for row in rows if slot < len(row["read_forms"])) for slot in range(reads)]
    ranks = [[sum(row["order"][rank] == slot for row in rows) for rank in slots] for slot in slots]
    middle = [centred(row["scores"]) for row in rows]
    by_kind, by_form = {}, {}
    for row, values in zip(rows, middle):
        for slot in slots:
            by_kind.setdefault(row["option_kinds"][slot], []).append((row["scores"][slot], values[slot]))
            if slot < len(row["read_forms"]):
                by_form.setdefault((slot, row["read_forms"][slot]), []).append(values[slot])
    read_slots = [slot for slot in slots if all(row["option_kinds"][slot] == "read" for row in rows)]
    read_orders = Counter(">".join(str(slot) for slot in row["order"] if slot in read_slots) for row in rows)
    certified = [(row, slot) for row in rows for slot in row["certified_slots"]]
    # What each row prefers once the tendency of every slot over the other rows is taken away.
    apart = {row["ordinal"]: order(values)
             for row, values in zip(rows, without_profile([row["scores"] for row in rows]))}
    apart_reads = {ordinal: [slot for slot in ranked if slot in read_slots] for ordinal, ranked in apart.items()}
    progress = [Counter(row["reference_progress"][slot] for row in rows if row["reference_progress"][slot]) for slot in slots]
    return {
        "rows": len(rows),
        "kind_by_slot": [dict(counter) for counter in kinds],
        "kind_constant_in_every_slot": all(len(counter) == 1 for counter in kinds),
        "read_form_by_read_slot": [dict(counter) for counter in forms],
        "score_by_slot": [spread([row["scores"][slot] for row in rows]) for slot in slots],
        "centred_score_by_slot": [spread([values[slot] for values in middle]) for slot in slots],
        "rank_counts_by_slot": [{"first": ranks[slot][0], "second": ranks[slot][1], "last": ranks[slot][-1],
                                 "by_rank": ranks[slot]} for slot in slots],
        "score_by_option_kind": {kind: {"raw": spread([raw for raw, _ in values]),
                                        "centred": spread([value for _, value in values])}
                                 for kind, values in sorted(by_kind.items())},
        "centred_score_by_read_slot_and_form": [
            {"slot": slot, "form": form, **spread(values)} for (slot, form), values in sorted(by_form.items())],
        "read_slot_orders": dict(read_orders.most_common()),
        "best_read_slot_counts": dict(sorted(Counter(
            next(slot for slot in row["order"] if slot in read_slots) for row in rows).items())),
        "fixed_profile": profile_share([row["scores"] for row in rows]),
        # The same over the read cards alone, so that a strong terminal card does not carry the share.
        "fixed_profile_of_reads": profile_share([[row["scores"][slot] for slot in read_slots] for row in rows])
        if len(read_slots) >= 2 else None,
        "winner_slot_counts": dict(sorted(Counter(row["order"][0] for row in rows).items())),
        "winner_margin": spread([row["margin"] for row in rows]),
        "certified_rows": sorted({row["ordinal"] for row, _ in certified}),
        "certified_cards": len(certified),
        "certified_by_slot": dict(sorted(Counter(slot for _, slot in certified).items())),
        "certified_by_read_form": dict(Counter(row["read_forms"][slot] for row, slot in certified)),
        "certified_rank_counts": dict(sorted(Counter(row["order"].index(slot) + 1 for row, slot in certified).items())),
        "certified_rank_among_reads": dict(sorted(Counter(
            [candidate for candidate in row["order"] if candidate in read_slots].index(slot) + 1
            for row, slot in certified).items())),
        "profile_removed": {
            "winner_slot_counts": dict(sorted(Counter(ranked[0] for ranked in apart.values()).items())),
            "best_read_slot_counts": dict(sorted(Counter(ranked[0] for ranked in apart_reads.values()).items())),
            "certified_rank_among_reads": dict(sorted(Counter(
                apart_reads[row["ordinal"]].index(slot) + 1 for row, slot in certified).items()))},
        "reference_progress_by_slot": [dict(counter) for counter in progress],
        "tokens": spread([row["tokens"] for row in rows]),
        "evidence_units": dict(Counter(row["evidence_units"] for row in rows)),
        "evidence_excerpt_bytes": spread([value for row in rows for value in row["evidence_excerpt_bytes"]]),
        "evidence_units_cut_short": sum(shown < whole for row in rows
                                        for shown, whole in zip(row["evidence_excerpt_bytes"],
                                                                row["evidence_original_bytes"])),
        "read_choices_available": dict(Counter(row["read_choices_available"] for row in rows)),
        "read_choices_omitted": dict(Counter(row["read_choices_omitted"] for row in rows)),
    }


def main():
    parser = argparse.ArgumentParser(description=__doc__.splitlines()[0])
    parser.add_argument("--panel", required=True, type=Path, help="compiled rows with their rosters and prompts")
    parser.add_argument("--raw", required=True, type=Path,
                        help="scores for the same rows: a retained raw panel, or a scores.json of score_variants.py")
    parser.add_argument("--preflight", required=True, type=Path, help="the reference preflight with certificates")
    parser.add_argument("--compiler", required=True, type=Path, help="the compiler profile: symbols and token limit")
    parser.add_argument("--output", required=True, type=Path, help="a new file for the table")
    args = parser.parse_args()
    require(sys.flags.isolated and sys.dont_write_bytecode, "use Python -I -B")
    panel = json.loads(args.panel.read_bytes())["rows"]
    scores = json.loads(args.raw.read_bytes())
    raw = raw_rows(scores)
    preflight = {row["id"]: row for row in json.loads(args.preflight.read_bytes())["rows"]}
    profile = json.loads(args.compiler.read_bytes())
    symbols = [item["id"] for item in profile["symbols"]]
    require(len(raw) == len(panel) == len(preflight), "the three inputs differ in row count")
    rows, checks = [], Counter()
    for row in panel:
        scored, original = raw[row["ordinal"]], preflight[row["id"]]
        # The input digest binds a score vector to the compiled row, and the roster binds the certificates.
        require(scored["input_digest"] == row["compiled"]["input_digest"]
                and original["request"]["roster"] == row["roster"],
                "scores or preflight do not belong to panel row " + str(row["ordinal"]))
        rows.append(described(row, scored["scores"], original))
        outcome = static_checks(row, symbols, profile["max_tokens"])
        checks.update(name for name, ok in outcome.items() if ok)
        rows[-1]["failed_checks"] = sorted(name for name, ok in outcome.items() if not ok)
    option_blocks = {tuple(map(tuple, parse_prompt(row["compiled"]["prompt"])["options"])) for row in panel}
    receipt = {"schema": SCHEMA, "qualification": False, "panel": pin(args.panel), "raw": pin(args.raw),
               # Two models can share one compiled input, so a score receipt also says which graph scored it.
               "raw_scored_by": {key: scores.get(key) for key in ("provider", "model", "weights", "padding")}
               if scores.get("schema") == SCORES else None,
               "preflight": pin(args.preflight), "compiler": pin(args.compiler),
               "compiler_limits": {"max_tokens": profile["max_tokens"], "max_options": profile["max_options"],
                                   "symbols": profile["symbols"]},
               "static_checks_passed": dict(sorted(checks.items())),
               "distinct_option_blocks": len(option_blocks),
               "tables": tables(rows), "rows": rows}
    with args.output.open("x", encoding="utf-8") as file:
        json.dump(receipt, file, indent=1, allow_nan=False)
    print(json.dumps({key: receipt[key] for key in ("static_checks_passed", "distinct_option_blocks", "tables")}))


if __name__ == "__main__":
    main()
