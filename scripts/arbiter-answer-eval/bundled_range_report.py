# SPDX-License-Identifier: Apache-2.0
"""Report paired gains and losses for every budget, including case IDs and source note IDs."""
import argparse
from collections import Counter
import json
from pathlib import Path

import prompt_need_audit as audit


def report(receipt):
    data = audit.read_json(receipt)
    totals, detail = {}, {}
    for key, task in data["tasks"].items():
        results = task["outcomes"]
        detail[key] = {}
        for label, row in results.items():
            prefix = label.rsplit("/",1)[0]
            baseline = results[prefix+"/original"]
            changes = {}
            for metric in ("whole_needs","whole_notes","whole_essential","all_mapped"):
                before, after = set(baseline[metric]), set(row[metric])
                changes[metric] = {"gained": sorted(after-before), "lost": sorted(before-after)}
            detail[key][label] = changes
            for stratum in ("all", task["set"]):
                c = totals.setdefault(label,{}).setdefault(stratum,Counter())
                c["tasks"] += 1
                for metric,delta in changes.items():
                    c[metric] += len(row[metric])
                    c[metric+"_gained"] += len(delta["gained"])
                    c[metric+"_lost"] += len(delta["lost"])
                    c[metric+"_tasks_with_gain"] += bool(delta["gained"])
                    c[metric+"_tasks_with_loss"] += bool(delta["lost"])
                c["passage_bytes"] += row["passage_bytes"]
                c["wire_bytes"] += row["wire_bytes"] or 0
                c["selector_seconds"] += row["selector_seconds"]
    return {"input_sha256":audit.digest(receipt.read_bytes()),"conditions":totals,"per_task":detail}


if __name__ == "__main__":
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("receipt",type=Path)
    parser.add_argument("output",type=Path)
    args = parser.parse_args()
    with args.output.open("x", encoding="utf-8") as fp:
        json.dump(report(args.receipt),fp,separators=(",",":"))
        fp.write("\n")
