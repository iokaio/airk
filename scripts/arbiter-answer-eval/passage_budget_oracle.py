# SPDX-License-Identifier: Apache-2.0
"""Offline exact coverage ceilings for two ranked prefixes, under the focused reply's passage limits."""

import argparse
import json
from collections import Counter
from functools import cache
from pathlib import Path
from unittest.mock import patch

import agent_cost_batch
import live_gather as gather
from gather_context_call_replay import classify


def passage(repo, at):
    path, _, interval = at.rpartition(":")
    first, last = map(int, interval.split("-"))
    return {"at": at, "text": "\n".join(repo.texts[path][first - 1:last]) + "\n"}


def retain(table, key, cost, witness):
    if cost <= gather.FOCUS_PASSAGE_BYTES and (key not in table or cost < table[key][0]):
        table[key] = (cost, witness)


def optimum(files, mask_key):
    """Exact finite DP. Equal masks with equal remaining quotas need only their cheapest witness."""
    bundles = {(0, 0): (0, ())}
    for rows in files:
        within = {(0, 0): (0, ())}
        for row in rows:
            mask = row[mask_key]
            if mask == 0:
                continue
            for (count, prior), (size, witness) in list(within.items()):
                if count < gather.RANGES_PER_FILE and mask & ~prior:
                    retain(within, (count + 1, prior | mask), size + row["bytes"], witness + (row["at"],))
        choices = {}
        for (_, mask), (size, witness) in within.items():
            if mask:
                retain(choices, mask, size, witness)
        prior_files = list(bundles.items())
        for (count, prior), (size, witness) in prior_files:
            if count < gather.FOCUS_FILES:
                for mask, (extra, ranges) in choices.items():
                    if mask & ~prior:
                        retain(bundles, (count + 1, prior | mask), size + extra, witness + ranges)
    (_, mask), (size, witness) = max(bundles.items(), key=lambda item: (item[0][1].bit_count(), -item[1][0]))
    return mask, size, witness


def solve(repo, task, files, keys, objective):
    mask, size, witness = optimum(files, objective)
    passages = [passage(repo, at) for at in witness]
    notes = classify(task, {"passages": passages}, {})
    allowed = {"passage"} if objective == "full" else {"passage", "near"}
    ids = [key for key, stage in notes.items() if stage in allowed]
    expected = {key for i, key in enumerate(keys) if mask & (1 << i)}
    counts = Counter(at.rpartition(":")[0] for at in witness)
    if (set(ids) != expected or len(counts) > gather.FOCUS_FILES or max(counts.values(), default=0) > gather.RANGES_PER_FILE
            or size != sum(len(p["text"].encode("utf-8")) for p in passages) or size > gather.FOCUS_PASSAGE_BYTES):
        raise ValueError("Oracle witness failed independent replay or capacity checks")
    return {"notes": ids, "bytes": size, "ranges": witness}


def evaluate(repo, arguments, task, previous):
    with patch.object(repo, "search", cache(repo.search)):
        trace = {}
        brief = gather.gather(repo, arguments, trace)
        brief["needs"][0]["where"] = trace["all_locations"]
        focused = gather.focus(repo, brief, spread=True)
        control = classify(task, {**focused, "omitted": gather.census_spans(focused)}, {})
        if control != previous["notes"]:
            raise ValueError("Control differs from retained all-spread reply")
        stages = {"raw": {}, "expanded": {}, "merged": {}}
        wide = {}
        with patch.object(gather, "TOP_FILES", 40):
            gather.gather(repo, arguments, wide, stage_trace=stages)
        if wide["ranked"] != trace["ranked"]:
            raise ValueError("Expanding the candidate pool changed the file order")
    keys = list(control)
    files = []
    for path, _ in wide["ranked"][:40]:
        candidates = []
        for first, last in stages["merged"][path]:
            at = f"{path}:{first}-{last}"
            item = passage(repo, at)
            scores = classify(task, {"passages": [item]}, {})
            full = sum(1 << i for i, key in enumerate(keys) if scores[key] == "passage")
            overlap = full | sum(1 << i for i, key in enumerate(keys) if scores[key] == "near")
            candidates.append({"at": at, "bytes": len(item["text"].encode("utf-8")), "full": full, "overlap": overlap})
        files.append(candidates)
    arms = {}
    for width in (6, 40):
        for cut in (True, False):
            pool = [rows[:gather.RANGES_PER_FILE] if cut else rows for rows in files[:width]]
            arms[f"{width}_{'cut' if cut else 'all'}"] = {objective: solve(repo, task, pool, keys, objective)
                                                          for objective in ("full", "overlap")}
    return {"control": control, "arms": arms}


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("campaign", type=Path)
    campaign = parser.parse_args().campaign
    calls = json.loads((campaign / "replay/first-calls-r1.json").read_text(encoding="utf-8"))["calls"]
    prior = {r["task"]: r for s in (campaign / "replay/focus-before-trim-r1.jsonl").read_text(encoding="utf-8").splitlines()
             if (r := json.loads(s))["rule"] == "all-spread"}
    rows = {r["id"]: r for r in agent_cost_batch.working_set(campaign)}
    if calls.keys() != rows.keys() or prior.keys() != rows.keys():
        raise ValueError("Mismatched frozen tasks")
    repos = {}
    for key, arguments in sorted(calls.items()):
        row = rows[key]
        source = row["snapshot"]
        if source not in repos:
            index = json.loads((campaign / "topical" / f"{source.name}.map.json").read_text(encoding="utf-8"))
            repos[source] = gather.Repository(source, index)
        task = json.loads(row["task"].read_text(encoding="utf-8"))
        print(json.dumps({"task": key, **evaluate(repos[source], arguments, task, prior[key])}), flush=True)


if __name__ == "__main__":
    main()
