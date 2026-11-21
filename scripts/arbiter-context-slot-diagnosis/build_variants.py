# SPDX-License-Identifier: Apache-2.0
"""Check compiled Context rows against the pinned tokenizer and build permuted inputs for chosen rows.

This is a development diagnosis. It qualifies nothing. See README.md in this folder.
"""
import argparse
from collections import Counter
import json
from pathlib import Path
import sys

sys.path.insert(0, str(Path(__file__).resolve().parent))
from layout import (FAMILIES, PAIRS, SUFFIX, digest, moved_reads, parse_prompt, pin, require, variant_plan,
                    variant_prompt, within_input_limits)

SCHEMA = "airk.private.context-slot-variants/1"
ASSISTANT = SUFFIX[SUFFIX.index("<|im_start|>"):]
# Families whose prompts have a form that no native input has.
PROBES = ("stated", "stated_reads") + PAIRS


def admitted(tokenizer, text, ids, profile):
    """Check a probe prompt against the rules the compiler applies, and return its candidate token ids.

    The prompt must parse as the template, meet the compiler's input limits, offer at least two options, which
    the score owner needs for a margin, and be extended by each symbol as one token.
    """
    parts = parse_prompt(text)
    symbols = profile["symbols"][:len(parts["options"])]
    require(len(symbols) >= 2 and within_input_limits(parts, profile["max_options"]),
            "a probe prompt breaks a compiler input limit")
    require(all(tokenizer.encode(text + item["text"], add_special_tokens=False).ids == ids + [item["id"]]
                for item in symbols), "a symbol does not extend a probe prompt by one token")
    return [item["id"] for item in symbols]


def chosen_ordinals(available, listed, every):
    """The ordinals to build: the listed ones and every `every`-th one, or all of them when neither is given."""
    if not listed and not every:
        return sorted(available)
    picked = set(listed) | ({ordinal for ordinal in available if ordinal % every == 0} if every else set())
    require(picked <= set(available), "an ordinal is not in the panel")
    return sorted(picked)


def tokenizer_checks(tokenizer, compiled, prompt, parts, profile):
    """Checks of one compiled row against the tokenizer. Each value is True when the row is as the compiler says."""
    ids = tokenizer.encode(prompt, add_special_tokens=False).ids
    symbols = profile["symbols"][:len(parts["options"])]
    # The compiler requires that each symbol is one token and that appending it leaves the prefix unchanged.
    extended = all(tokenizer.token_to_id(item["text"]) == item["id"]
                   and tokenizer.encode(item["text"], add_special_tokens=False).ids == [item["id"]]
                   and tokenizer.encode(prompt + item["text"], add_special_tokens=False).ids == ids + [item["id"]]
                   for item in symbols)
    tail = tokenizer.encode(ASSISTANT, add_special_tokens=False).ids
    reads = len(json.loads(parts["state"])["verified_reads"])
    return {
        "tokens_equal_the_whole_prompt": ids == compiled["input_ids"],
        "tokens_decode_to_the_prompt": tokenizer.decode(compiled["input_ids"], skip_special_tokens=False) == prompt,
        "symbols_are_single_tokens_that_extend_the_prefix": extended,
        "candidates_are_the_symbol_tokens": compiled["candidate_positions"] == [item["id"] for item in symbols],
        "option_symbols_are_the_profile_symbols": [option[0] for option in parts["options"]]
        == [item["text"] for item in symbols],
        "input_ends_with_the_assistant_turn": compiled["input_ids"][-len(tail):] == tail,
        "identity_read_order_keeps_the_state": moved_reads(parts["state"], list(range(reads))) == parts["state"],
    }


def main():
    parser = argparse.ArgumentParser(description=__doc__.splitlines()[0])
    parser.add_argument("--panel", required=True, type=Path, help="compiled rows with their prompts")
    parser.add_argument("--tokenizer", required=True, type=Path, help="the pinned tokenizer.json of the model")
    parser.add_argument("--compiler", required=True, type=Path, help="the compiler profile beside the tokenizer")
    parser.add_argument("--ordinal", action="append", default=[], type=int, help="a row to build variants for")
    parser.add_argument("--every", type=int, help="also build variants for every N-th ordinal")
    parser.add_argument("--family", action="append", default=[], choices=FAMILIES,
                        help="a variant family to build; all of them by default")
    parser.add_argument("--identity-only", action="store_true", help="build the unpermuted input alone")
    parser.add_argument("--output", required=True, type=Path, help="a fresh, empty directory")
    args = parser.parse_args()
    require(sys.flags.isolated and sys.dont_write_bytecode, "use Python -I -B")
    require(args.output.is_dir() and not any(args.output.iterdir()), "a fresh empty output directory is required")
    profile = json.loads(args.compiler.read_bytes())
    require(profile["recipe"] == "qwen_single_choice_v1" and profile["tokenizer"] == digest(args.tokenizer),
            "the tokenizer is not the one the compiler profile pins")

    from tokenizers import Tokenizer
    import tokenizers

    tokenizer = Tokenizer.from_file(str(args.tokenizer))
    # The compiler disables truncation and padding before it encodes.
    tokenizer.no_truncation()
    tokenizer.no_padding()
    panel = json.loads(args.panel.read_bytes())["rows"]
    require(not (args.identity_only and args.family), "--identity-only builds no family")
    families = [] if args.identity_only else args.family or list(FAMILIES)
    build = set(chosen_ordinals([row["ordinal"] for row in panel], args.ordinal, args.every))
    passed, failed, rows, pure, lengths = Counter(), {}, [], Counter(), {}
    for row in panel:
        compiled = row["compiled"]
        prompt = compiled["prompt"]
        parts = parse_prompt(prompt)
        outcome = tokenizer_checks(tokenizer, compiled, prompt, parts, profile)
        passed.update(name for name, ok in outcome.items() if ok)
        if not all(outcome.values()):
            failed[row["ordinal"]] = sorted(name for name, ok in outcome.items() if not ok)
        if row["ordinal"] not in build:
            continue
        require(all(outcome.values()), "row " + str(row["ordinal"]) + " fails a tokenizer check: " + str(outcome))
        reads = len(json.loads(parts["state"])["verified_reads"])
        variants = []
        for name, family, sources in variant_plan(len(parts["options"]), reads, families):
            text = variant_prompt(parts, family, sources)
            ids = tokenizer.encode(text, add_special_tokens=False).ids
            require(len(ids) <= profile["max_tokens"], "a variant exceeds the compiler token limit")
            variant = {"name": name, "family": family, "sources": sources, "tokens": len(ids), "input_ids": ids}
            if family == "identity":
                # The method is valid only if the unpermuted rebuild gives back the compiled tokens exactly.
                require(ids == compiled["input_ids"], "the identity rebuild differs from the compiled tokens")
            if family in ("cards", "cards_ids"):
                # Moving whole option texts should move token spans and change nothing else.
                pure[(family, len(ids) == len(compiled["input_ids"])
                      and sorted(ids) == sorted(compiled["input_ids"]))] += 1
            if family in PROBES:
                # No native input has this form. It must still satisfy the rules the compiler applies to one.
                symbols = admitted(tokenizer, text, ids, profile)
                if family in PAIRS:
                    variant["candidate_positions"] = symbols
            low, high = lengths.get(family, (len(ids), len(ids)))
            lengths[family] = (min(low, len(ids)), max(high, len(ids)))
            variants.append(variant)
        rows.append({"ordinal": row["ordinal"], "input_digest": compiled["input_digest"],
                     "candidate_positions": compiled["candidate_positions"], "variants": variants})
    receipt = {"schema": SCHEMA, "qualification": False, "tokenizers": tokenizers.__version__,
               "panel": pin(args.panel) | {"rows": len(panel)}, "tokenizer": pin(args.tokenizer),
               "compiler": pin(args.compiler), "families": families,
               "tokenizer_checks_passed": dict(sorted(passed.items())), "tokenizer_checks_failed": failed,
               "assistant_turn_tokens": [tokenizer.decode([token], skip_special_tokens=False) for token in
                                         tokenizer.encode(ASSISTANT, add_special_tokens=False).ids],
               "card_moves_that_only_move_tokens": {family + ("" if same else "_not"): count
                                                    for (family, same), count in sorted(pure.items())},
               "tokens_by_family": {family: {"min": low, "max": high} for family, (low, high) in lengths.items()},
               "rows": rows}
    with (args.output / "variants.json").open("x", encoding="utf-8") as file:
        json.dump(receipt, file, allow_nan=False)
    print(json.dumps({key: receipt[key] for key in receipt if key != "rows"}
                     | {"built_rows": len(rows), "variants_per_row": len(rows[0]["variants"]) if rows else 0}))


if __name__ == "__main__":
    main()
