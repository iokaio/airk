# SPDX-License-Identifier: Apache-2.0
"""Grade actual budgeted passages and compare each arm with the same-task control."""
import argparse
from collections import Counter, defaultdict
import json
from pathlib import Path

from agent_cost_batch import working_set
import prompt_need_audit as audit


def report(campaign, allocations, wire, reference):
    offline = audit.read_json(allocations)
    framed = audit.read_json(wire)
    labels = audit.read_json(reference)
    if (framed["allocation_sha256"] != audit.digest(allocations.read_bytes())
            or offline["reference_sha256"] != audit.digest(reference.read_bytes())):
        raise ValueError("Different frozen comparison")
    output, details = defaultdict(lambda: defaultdict(Counter)), {}
    for task in working_set(campaign):
        key = task["id"]
        meta = offline["tasks"][key]
        truth = audit.read_json(task["task"])
        links = labels["tasks"][key]["need_notes"]
        notes = {n["id"]: n for n in truth["notes"]}
        snapshot = task["snapshot"] if task["repository"] != "bookbuilder" else campaign / "agent-cost/E2E-r1/snapshots/bookbuilder"
        details[key] = {}
        for arm, raw in meta["arms"].items():
            formats = {"passage": raw, **framed["tasks"][key][arm]}
            for label, value in formats.items():
                scope = f"{label}/{arm}"
                rows = value["passages"]
                data = {"passages": rows, "budget": raw["budget"], "passage_bytes": value["passage_bytes"]}
                judged = audit.score_variant(data, snapshot, notes, links, expected_budget=raw["budget"])
                whole = sorted(k for k, v in judged["notes"].items() if v == "whole")
                partial = sorted(k for k,v in judged["notes"].items() if v == "rule3")
                sizes = {"passage_bytes": value["passage_bytes"], "passages": len(rows),
                         "whole_positive": len(whole), "five_line_partial_only": len(partial),
                         "any_whole_need": value["totals"]["needs_with_any_whole_reference"],
                         "any_five_line_or_whole_need": value["totals"]["needs_with_rule3_reference"],
                         "whole_essential": value["totals"]["whole_essential_notes"],
                         "attributed_whole_need": value["totals"]["needs_with_attributed_whole_reference"],
                         "omitted": meta["candidate_ranges"] - len(rows), "tasks": 1}
                if label != "passage":
                    sizes.update(wire_bytes=value["wire_bytes"], wire_dropped=value["dropped_by_wire"])
                for stratum in ("all", meta["stratum"]):
                    output[scope][stratum].update(sizes)
                details[key][scope] = {"whole": whole, "five_line_partial_only": partial,
                                       "any_whole_need": [n["index"] for n in judged["needs"] if n["whole_refs"]],
                                       "bytes": value["passage_bytes"]}
    return {"allocation_sha256": audit.digest(allocations.read_bytes()), "wire_sha256": audit.digest(wire.read_bytes()),
            "reference_sha256": audit.digest(reference.read_bytes()), "aggregates": output, "tasks": details}


if __name__ == "__main__":
    parser = argparse.ArgumentParser(description=__doc__)
    for name in ("campaign", "allocations", "wire", "reference", "output"):
        parser.add_argument(name, type=Path)
    args = parser.parse_args()
    result = report(args.campaign, args.allocations, args.wire, args.reference)
    with args.output.open("x", encoding="utf-8") as stream:
        json.dump(result, stream, indent=1)
        stream.write("\n")