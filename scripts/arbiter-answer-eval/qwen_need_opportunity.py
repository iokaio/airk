# SPDX-License-Identifier: Apache-2.0
"""Post-hoc limits imposed by fixed pair allocation and single excerpt nomination. No model calls."""

import argparse
from collections import Counter
import json
from pathlib import Path

import common
from agent_cost_batch import working_set
from prompt_need_audit import evidence_level, parse_location


def spans(locations):
    return [(*parse_location(at), []) for at in locations if at]


def has_whole(notes, locations):
    return any(evidence_level(note, locations) == "whole" for note in notes)


def opportunities(row, notes, groups):
    result = []
    pool = row["pool"]
    direct = row["variants"]["direct"]
    delivered = direct["passages"]
    for n, ids in enumerate(groups, 1):
        located = [notes[i] for i in ids if not notes[i].get("absence") and notes[i].get("path") and notes[i].get("lines")]
        if not located:
            continue
        paths = {loc["path"] for note in located for loc in (note, *note.get("alternatives", []))
                 if loc.get("path") and loc.get("lines")}
        assigned = [p for i, p in enumerate(pool) if i % len(groups) == n - 1]
        if [p["file"] for p in assigned] != [s["file"] for s in direct["scores"] if s["need"] == n]:
            raise ValueError("Assigned roster differs from actual scoring receipt")
        all_ranges = lambda candidates: spans(at for p in candidates for at in p["locations"])
        tests = {
            "pool_contains_reference_file": any(p["file"] in paths for p in pool),
            "assigned_files_contain_reference_file": any(p["file"] in paths for p in assigned),
            "pool_ranges_contain_whole_ref": has_whole(located, all_ranges(pool)),
            "assigned_file_ranges_contain_whole_ref": has_whole(located, all_ranges(assigned)),
            "assigned_first_range_contains_whole_ref": has_whole(located, spans(p["returned_at"] for p in assigned)),
            "assigned_scored_excerpt_contains_whole_ref": has_whole(located, spans(p["scored_at"] for p in assigned)),
            "returned_attributed_range_contains_whole_ref": has_whole(located, spans(p["at"] for p in delivered if n in p["needs"])),
            "returned_range_any_owner_contains_whole_ref": has_whole(located, spans(p["at"] for p in delivered)),
        }
        result.append({"need": n, **tests})
    return result


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("campaign", type=Path)
    parser.add_argument("references", type=Path)
    parser.add_argument("receipts", type=Path)
    parser.add_argument("--output", required=True, type=Path)
    args = parser.parse_args()
    mapping = common.read_json(args.references)
    header, *rows = common.read_jsonl(args.receipts)
    meta = {t["id"]: t for t in working_set(args.campaign)}
    if sorted(r["task"] for r in rows) != header["tasks"] or len(rows) != 20:
        raise ValueError("Full matched panel required")
    by_task, count = {}, Counter()
    for row in rows:
        key = row["task"]
        annotation = mapping["tasks"][key]
        path = meta[key]["task"]
        if common.sha256_file(path).lower() != row["task_sha256"].lower() or row["task_sha256"].lower() != annotation["scenario_sha256"]:
            raise ValueError("Task inputs differ from independent mapping")
        notes = {n["id"]: n for n in common.read_json(path)["notes"]}
        by_task[key] = opportunities(row, notes, annotation["need_notes"])
        for rec in by_task[key]:
            count.update({k: int(v) for k,v in rec.items() if k != "need"})
    output = {"reference_sha256": common.sha256_file(args.references), "receipt_sha256": common.sha256_file(args.receipts),
              "scorable_needs": sum(map(len, by_task.values())), "counts": dict(count), "by_task": by_task}
    with args.output.open("x", encoding="utf-8") as fp:
        json.dump(output, fp, indent=2)
        fp.write("\n")


if __name__ == "__main__":
    main()
