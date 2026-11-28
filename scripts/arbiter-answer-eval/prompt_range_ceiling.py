# SPDX-License-Identifier: Apache-2.0
"""Development-only oracle for widening the existing forty files' nominated source intervals.

The measured ceiling precedes ranking, reply packing and inference. The private oracle never
selects evidence for a solver. For each fixed radius also report single-reference feasibility:
at least one expanded source interval containing a full mapped note fits 14,000 UTF-8 bytes.
"""
import argparse
from collections import Counter
import json
from pathlib import Path

from prompt_need_audit import digest, evidence_level, parse_location, read_json, source_lines, working_set

RADII = (0, 4, 8, 16, 32, 64)


def widen(snapshot, pool, radius):
    expanded = []
    eligible = []
    for entry in pool:
        path = entry["file"]
        lines = source_lines(snapshot, path)
        for at in entry["locations"]:
            _, start, end = parse_location(at)
            low, high = max(1, start - radius), min(len(lines), end + radius)
            span = path, low, high, []
            expanded.append(span)
            size = len(("\n".join(lines[low-1:high]) + "\n").encode())
            if size <= 14000:
                eligible.append(span)
    return expanded, eligible


def levels_for(notes, spans, eligible):
    return {key: (evidence_level(note, spans) == "whole",
                  any(evidence_level(note, [span]) == "whole" for span in eligible))
            for key, note in notes.items()}


def run(campaign, paired, grades, annotations):
    independent = read_json(grades)
    if independent["receipt_sha256"] != digest(paired.read_bytes()) or independent["reference_sha256"] != digest(annotations.read_bytes()):
        raise ValueError("Input hashes differ from independent grade")
    inventory = {r["id"]: r for r in working_set(campaign)}
    mapping = read_json(annotations)
    rows = [json.loads(line) for line in paired.read_bytes().splitlines()[1:]]
    results = {}
    counts = {r: Counter() for r in RADII}
    for row in rows:
        key = row["task"]
        pool = row["pool"]
        snapshot = Path(row["source"])
        case = read_json(inventory[key]["task"])
        ref = mapping["tasks"][key]
        if digest(inventory[key]["task"].read_bytes()) != ref["scenario_sha256"]:
            raise ValueError("Reference source changed")
        positive = {n["id"]: n for n in case["notes"] if n.get("path") and n.get("lines") and not n.get("absence")}
        baseline = independent["tasks"][key]["pool_ceiling"]
        missing = {n["index"]: n["pooled_file_refs"] for n in baseline if n["pooled_file_refs"] and not n["pooled_whole_refs"]}
        missing_refs = {i for refs in missing.values() for i in refs}
        selected = {n: positive[n] for n in missing_refs}
        results[key] = {}
        for radius in RADII:
            spans, eligible = widen(snapshot, pool, radius)
            levels = levels_for(selected, spans, eligible)
            needs = []
            for index, refs in missing.items():
                whole = sorted(key for key in refs if levels[key][0])
                fit = sorted(key for key in refs if levels[key][1])
                needs.append({"index": index, "whole_refs": whole, "single_interval_within_budget": fit})
            results[key][radius] = needs
            result = counts[radius]
            result["file_present_range_missing_needs"] += len(missing)
            result["needs_with_any_whole_reference"] += sum(bool(n["whole_refs"]) for n in needs)
            result["needs_with_any_single_fitting_interval"] += sum(bool(n["single_interval_within_budget"]) for n in needs)
            result["distinct_mapped_notes_in_missing_needs"] += len(selected)
            result["whole_mapped_notes"] += sum(v[0] for v in levels.values())
            result["single_fitting_interval_notes"] += sum(v[1] for v in levels.values())
            result["essential_mapped_notes"] += sum(n["mark"] == "essential" for n in selected.values())
            result["whole_essential_mapped_notes"] += sum(v[0] and selected[k]["mark"] == "essential" for k, v in levels.items())
    if counts[0]["needs_with_any_whole_reference"] or len(results) != len(inventory):
        raise ValueError("Does not reproduce baseline range ceiling")
    return {"raw_sha256": digest(paired.read_bytes()), "grade_sha256": digest(grades.read_bytes()),
            "mapping_sha256": digest(annotations.read_bytes()), "by_radius": counts, "tasks": results}


if __name__ == "__main__":
    parser = argparse.ArgumentParser(description=__doc__)
    for name in ("campaign", "paired", "grades", "annotations"):
        parser.add_argument(name, type=Path)
    print(json.dumps(run(**vars(parser.parse_args())), sort_keys=True))
