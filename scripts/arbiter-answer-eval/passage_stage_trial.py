# SPDX-License-Identifier: Apache-2.0
"""Trace frozen reference notes through the ordinary gather and the all-spread focus candidate."""

import argparse
import json
import math
from collections import Counter
from pathlib import Path

import agent_cost_batch
import live_gather
import rank_check
from gather_context_call_replay import classify


def nominate_files(ranked, held, index, needs):
    """Alternate original leaders with each need's best matching file. Original rank breaks ties."""
    words_by_file = {}
    frequencies = Counter()
    for path in ranked:
        words = set(index["files"][path]["path_words"])
        for term in held[path]:
            if term != "named in the request":
                words.update(live_gather.topical_index.words_of(term))
        words_by_file[path] = words
        frequencies.update(words)
    nominations = []
    for need in needs:
        words = {word for word in live_gather.topical_index.words_of(str(need))
                 if len(word) >= 4 and word not in live_gather.STOP and word in frequencies}
        if words:
            weights = {word: math.log1p(len(ranked) / frequencies[word]) for word in sorted(words)}
            winner = max(ranked, key=lambda p: sum(w for word, w in weights.items() if word in words_by_file[p]))
            if winner not in nominations:
                nominations.append(winner)
    ordered = {}
    base = iter(ranked)
    for winner in nominations:
        leader = next((p for p in base if p not in ordered), None)
        if leader is not None:
            ordered[leader] = None
        ordered[winner] = None
    ordered.update(dict.fromkeys(ranked))
    return list(ordered)


def locations(table):
    return [{"at": f"{path}:{start}-{end}"} for path, spans in table.items() for start, end in spans]


def score_locations(task, rows):
    return classify(task, {"needs": [{"where": rows}]}, {})


def evaluate(repo, arguments, task, **options):
    trace, stages = {}, {"raw": {}, "expanded": {}, "merged": {}}
    brief = live_gather.gather(repo, arguments, trace, stage_trace=stages, **options)
    ranks = {p: i for i, (p, _) in enumerate(trace["ranked"], 1)}
    top_six = set(list(ranks)[:live_gather.FOCUS_FILES])
    selected = trace["all_locations"]
    pool = {k: score_locations(task, locations(stages[k])) for k in ("expanded", "merged")}
    pool["merged_six"] = score_locations(task, locations({p: v for p, v in stages["merged"].items() if p in top_six}))
    pool["cut"] = score_locations(task, selected)
    pool["cut_six"] = score_locations(task, [r for r in selected if r["at"].rpartition(":")[0] in top_six])
    pool["trim"] = score_locations(task, brief["needs"][0]["where"])
    for label, spread in (("baseline", False), ("all_spread", True)):
        if spread:
            brief["needs"][0]["where"] = selected
        focus = live_gather.focus(repo, brief, spread=spread)
        pool[label] = classify(task, {**focus, "omitted": live_gather.census_spans(focus)}, {})
    result = {}
    for note in task["notes"]:
        key = note["id"]
        if key not in pool["baseline"]:
            continue
        facts = {stage: notes[key] for stage, notes in pool.items()}
        for label, hits in (("raw", stages["raw"]), ("accepted", stages["accepted"])):
            facts[label] = any(start <= n <= end for loc in (note, *note.get("alternatives", []))
                               if loc.get("lines") for start, end in [loc["lines"]]
                               for n in hits.get(loc["path"], ()))
        facts["rank"] = ranks.get(note["path"])
        result[key] = facts
    return result, list(ranks)[:live_gather.FOCUS_FILES], {p: ranks.get(p) for p in rank_check.truth_files(task)}


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("campaign", type=Path)
    parser.add_argument("--nominate-files", action="store_true")
    args = parser.parse_args()
    campaign = args.campaign
    calls = json.loads((campaign / "replay/first-calls-r1.json").read_text(encoding="utf-8"))["calls"]
    rows = {row["id"]: row for row in agent_cost_batch.working_set(campaign)}
    if calls.keys() != rows.keys():
        raise ValueError("Frozen calls differ from working set")
    repos = {}
    for key, arguments in sorted(calls.items()):
        row = rows[key]
        source = row["snapshot"]
        if source not in repos:
            index = json.loads((campaign / "topical" / f"{source.name}.map.json").read_text(encoding="utf-8"))
            repos[source] = live_gather.Repository(source, index)
        task = json.loads(row["task"].read_text(encoding="utf-8"))
        notes, first, truth = evaluate(repos[source], arguments, task,
                                       rank_rule=nominate_files if args.nominate_files else None)
        print(json.dumps({"task": key, "notes": notes, "first_six": first, "truth": truth}), flush=True)


if __name__ == "__main__":
    main()
