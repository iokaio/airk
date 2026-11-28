# SPDX-License-Identifier: Apache-2.0
"""Score file ranks and focused passages from the same recorded calls, without model use."""

import argparse
import json
from functools import cache, partial
from pathlib import Path, PurePosixPath
from unittest.mock import patch

import agent_cost_batch
import live_gather
import rank_check
import topical_aspects


def apply_file_rule(rule, repo, terms, score, held):
    files = repo.index["files"]
    if rule == "direct-doc":
        direct = {term["term"] for term in terms if term["source"] in ("anchor", "quoted", "named")}
        for path, matches in held.items():
            if files[path]["role"] == "document" and direct.intersection(matches):
                score[path] /= live_gather.ROLE_WEIGHT["document"]
        return
    if rule == "sibling-test":
        links = topical_aspects.test_links(files)
    else:
        projects = {}
        for path in files:
            name = PurePosixPath(path)
            if name.suffix == ".csproj" or name.name in ("Cargo.toml", "pyproject.toml", "package.json", "go.mod"):
                projects.setdefault(name.parent, []).append(path)
        links = {}
        for path in held:
            for parent in PurePosixPath(path).parents:
                if parent in projects:
                    if len(projects[parent]) == 1:
                        links[path] = projects[parent]
                    break
    inferred = {}
    for path in held:
        if files[path]["role"] == "source":
            for target in links.get(path, ()):
                inferred[target] = max(inferred.get(target, 0), score[path] * 0.15)
    for target, credit in inferred.items():
        score[target] = max(score.get(target, 0), credit)
        held.setdefault(target, {})


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("campaign", type=Path)
    parser.add_argument("rules", nargs="+", choices=("baseline", "score-only", "sibling-test", "project", "direct-doc", "direct-passage", "spread", "all-locations", "all-spread", "short-spread"))
    args = parser.parse_args()
    campaign = args.campaign
    calls = json.loads((campaign / "replay/first-calls-r1.json").read_text(encoding="utf-8"))["calls"]
    rows = {row["id"]: row for row in agent_cost_batch.working_set(campaign)}
    if calls.keys() != rows.keys():
        raise ValueError("Recorded calls differ from the working set")
    repos = {}
    for task_id, arguments in sorted(calls.items()):
        row = rows[task_id]
        source = row["snapshot"]
        if source not in repos:
            index = json.loads((campaign / "topical" / f"{source.name}.map.json").read_text(encoding="utf-8"))
            repos[source] = live_gather.Repository(source, index)
        repo = repos[source]
        task = json.loads(row["task"].read_text(encoding="utf-8"))
        # All rules read the same frozen snapshot. Share term scans only within this task.
        with patch.object(repo, "search", cache(repo.search)):
            for rule in args.rules:
                trace = {}
                file_rule = partial(apply_file_rule, rule) if rule in ("sibling-test", "project", "direct-doc") else None
                brief = live_gather.gather(repo, arguments, trace, extra_hop_weight=0.15 if rule == "score-only" else 0,
                                           file_rule=file_rule, direct_passages=rule == "direct-passage",
                                           short_passages=rule == "short-spread")
                if rule in ("all-locations", "all-spread", "short-spread"):
                    brief["needs"][0]["where"] = trace["all_locations"]
                focused = live_gather.focus(repo, brief, spread=rule in ("spread", "all-spread", "short-spread"))
                ranks = {path: position for position, (path, _) in enumerate(trace["ranked"], 1)}
                truth = {path: ranks.get(path) for path in rank_check.truth_files(task)}
                scored = {**focused, "omitted": live_gather.census_spans(focused)}
                notes = live_gather.replay_check.classify(task, scored, {})
                print(json.dumps({"task": task_id, "rule": rule, "truth": truth, "notes": notes,
                                  "first_six": [path for path, _ in trace["ranked"][:6]],
                                  "elapsed_ms": brief["elapsed_ms"]}), flush=True)


if __name__ == "__main__":
    main()
