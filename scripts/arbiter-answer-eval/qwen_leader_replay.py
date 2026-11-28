# SPDX-License-Identifier: Apache-2.0
"""Replay one protected file leader, and identify where plain Qwen lost passages."""

import argparse
from functools import cache
import json
from pathlib import Path
from unittest.mock import patch

import agent_cost_batch
import live_gather as gather
from gather_context_call_replay import classify
from qwen_file_trial import focused, reorder


def run_task(repo, arguments, task, saved):
    paths, scores = zip(*saved["scores"])
    qwen = reorder(paths, scores)
    results = {}
    for label, prefix in (("qwen", qwen), ("leader", [paths[0]] + [p for p in qwen if p != paths[0]])):
        def order(ranked, held, index, needs):
            if ranked[:len(paths)] != list(paths):
                raise ValueError("Changed candidate roster")
            return prefix + ranked[len(paths):]
        trace = {}
        brief = gather.gather(repo, arguments, trace, rank_rule=order)
        full = classify(task, {"needs": [{"where": trace["all_locations"]}]}, {})
        reply = focused(repo, brief, trace)
        with patch.object(gather, "FOCUS_PASSAGE_BYTES", 10**12):
            roomy = focused(repo, brief, trace)
        notes = classify(task, {**reply, "omitted": gather.census_spans(reply)}, {})
        allowed = classify(task, {"passages": roomy["passages"]}, {})
        lost = {k: {"after_cut": full[k], "without_byte_cap": allowed[k], "delivered": notes[k]}
                for k, v in saved["control"]["notes"].items() if v == "passage" and saved["qwen"]["notes"][k] != "passage"}
        results[label] = {"notes": notes, "lost": lost, "first_six": prefix[:6]}
    if results["qwen"]["notes"] != saved["qwen"]["notes"]:
        raise ValueError("Qwen control failed")
    return results


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("campaign", type=Path)
    c = parser.parse_args().campaign
    calls = json.loads((c / "replay/first-calls-r1.json").read_text(encoding="utf-8"))["calls"]
    saved = {r["task"]: r for line in (c / "replay/qwen-file-top40-r1.jsonl").read_text().splitlines()[1:]
             if (r := json.loads(line))}
    rows = {r["id"]: r for r in agent_cost_batch.working_set(c)}
    if calls.keys() != rows.keys() or saved.keys() != rows.keys():
        raise ValueError("Frozen tasks differ")
    repos = {}
    for key, arguments in sorted(calls.items()):
        row = rows[key]
        src = row["snapshot"]
        if src not in repos:
            repos[src] = gather.Repository(src, json.loads((c / "topical" / f"{src.name}.map.json").read_text(encoding="utf-8")))
        repo = repos[src]
        task = json.loads(row["task"].read_text(encoding="utf-8"))
        with patch.object(repo, "search", cache(repo.search)):
            print(json.dumps({"task": key, **run_task(repo, arguments, task, saved[key])}), flush=True)


if __name__ == "__main__":
    main()
