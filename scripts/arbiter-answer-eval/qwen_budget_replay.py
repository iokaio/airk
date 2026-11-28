# SPDX-License-Identifier: Apache-2.0
"""Replay saved Qwen file scores with a different focused passage byte limit."""

import argparse
from collections import Counter
import json
from pathlib import Path
from unittest.mock import patch

import agent_cost_batch
import live_gather as gather
from gather_context_call_replay import classify
from qwen_file_trial import focused, reorder


def measure(repo, brief, trace, task, budget):
    with patch.object(gather, "FOCUS_PASSAGE_BYTES", budget):
        reply = focused(repo, brief, trace)
    return {"notes": classify(task, {**reply, "omitted": gather.census_spans(reply)}, {}),
            "bytes": sum(len(p["text"].encode("utf-8")) for p in reply["passages"])}


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("campaign", type=Path)
    parser.add_argument("budget", type=int)
    args = parser.parse_args()
    if args.budget <= 0:
        parser.error("budget must be positive")
    c = args.campaign
    calls = json.loads((c / "replay/first-calls-r1.json").read_text(encoding="utf-8"))["calls"]
    receipt = (c / "replay/qwen-file-top40-r1.jsonl").read_text(encoding="utf-8").splitlines()
    saved = {r["task"]: r for line in receipt[1:] if (r := json.loads(line))}
    rows = {r["id"]: r for r in agent_cost_batch.working_set(c)}
    if calls.keys() != saved.keys() or calls.keys() != rows.keys():
        raise ValueError("Frozen task set mismatch")
    repos = {}
    totals = [Counter(), Counter()]
    for key, arguments in sorted(calls.items()):
        row = rows[key]
        source = row["snapshot"]
        if source not in repos:
            index = json.loads((c / "topical" / f"{source.name}.map.json").read_text(encoding="utf-8"))
            repos[source] = gather.Repository(source, index)
        repo = repos[source]
        paths, scores = zip(*saved[key]["scores"])

        def rank_rule(ranked, held, index, needs):
            if list(paths) != ranked[:len(paths)]:
                raise ValueError("Frozen candidate order mismatch")
            return reorder(paths, scores) + ranked[len(paths):]

        trace = {}
        brief = gather.gather(repo, arguments, trace, rank_rule=rank_rule)
        task = json.loads(row["task"].read_text(encoding="utf-8"))
        control = measure(repo, brief, trace, task, 14000)
        if any(control[k] != saved[key]["qwen"][k] for k in control):
            raise ValueError("Recorded Qwen control mismatch")
        trial = measure(repo, brief, trace, task, args.budget)
        for total, arm in zip(totals, (control, trial)):
            total.update(arm["notes"].values())
            total["bytes"] += arm["bytes"]
        changes = {n: [v, trial["notes"][n]] for n, v in control["notes"].items() if v != trial["notes"][n]}
        print(json.dumps({"task": key, "changes": changes, "bytes": [control["bytes"], trial["bytes"]]}), flush=True)
    print(json.dumps({"totals": totals}))


if __name__ == "__main__":
    main()
