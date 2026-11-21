# SPDX-License-Identifier: Apache-2.0
"""Compare two score receipts of the same variants, for example one from each provider.

This is a development description. It qualifies nothing. See README.md in this folder.
"""
import argparse
import json
from pathlib import Path
import sys

sys.path.insert(0, str(Path(__file__).resolve().parent))
from layout import order, pin, require, spread

SCORES = "airk.private.context-slot-scores/1"


def compared(first, second):
    """Differences between the scores of the variants that two receipts' rows have in common, by family.

    A common variant must have the same sources and the same tokens. `winner_changes` counts the forwards
    whose top option differs between the two.
    """
    known = {(row["ordinal"], variant["name"]): variant for row in first for variant in row["variants"]}
    by_family = {}
    for row in second:
        for variant in row["variants"]:
            earlier = known.get((row["ordinal"], variant["name"]))
            if earlier is None:
                continue
            require(earlier["sources"] == variant["sources"] and earlier["tokens"] == variant["tokens"]
                    and len(earlier["scores"]) == len(variant["scores"]),
                    "a common variant is not the same input: " + variant["name"])
            worst = max(abs(a - b) for a, b in zip(earlier["scores"], variant["scores"]))
            changed = order(earlier["scores"])[0] != order(variant["scores"])[0]
            by_family.setdefault(variant["family"], []).append((worst, changed))
    require(bool(by_family), "the receipts have no variant in common")
    everything = [item for items in by_family.values() for item in items]

    def block(items):
        return {"max_abs_difference": spread([worst for worst, _ in items]),
                "winner_changes": sum(changed for _, changed in items)}

    return {"forwards": len(everything), **block(everything),
            "by_family": {family: block(items) for family, items in sorted(by_family.items())}}


def main():
    parser = argparse.ArgumentParser(description=__doc__.splitlines()[0])
    parser.add_argument("first", type=Path, help="a completed scores.json")
    parser.add_argument("second", type=Path, help="another completed scores.json of the same model")
    args = parser.parse_args()
    require(sys.flags.isolated and sys.dont_write_bytecode, "use Python -I -B")
    docs = [json.loads(path.read_bytes()) for path in (args.first, args.second)]
    require(all(doc.get("schema") == SCORES and doc.get("complete") is True for doc in docs)
            and docs[0]["model"]["sha256"] == docs[1]["model"]["sha256"],
            "two completed score receipts of one model are required")
    print(json.dumps({"first": pin(args.first) | {"provider": docs[0]["provider"]},
                      "second": pin(args.second) | {"provider": docs[1]["provider"]},
                      "model": docs[0]["model"]["sha256"]} | compared(docs[0]["rows"], docs[1]["rows"])))


if __name__ == "__main__":
    main()
