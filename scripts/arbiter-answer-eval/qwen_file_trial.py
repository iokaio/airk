# SPDX-License-Identifier: Apache-2.0
"""Paired local CUDA file-reranking experiment on frozen topical first calls."""

import argparse
from collections import defaultdict
from functools import cache
import json
import math
from pathlib import Path
from time import perf_counter
from unittest.mock import patch

import agent_cost_batch
import common
from arbiter_pair_probe import PREFIX, SUFFIX, pair_text
from arbiter_loop_prototype import Judge
import live_gather as gather
import rank_check
from gather_context_call_replay import classify

INSTRUCTION = "Given a coding agent's task, determine whether this repository file contains evidence needed to complete it."
WIDTH = 40
EXCERPT_CHARS = 480


def document(repo, path, locations):
    parts = [path]
    for at in locations:
        start, end = map(int, at.rpartition(":")[2].split("-"))
        text = "\n".join(repo.texts[path][start - 1:end])
        parts.append(f"Lines {start}-{end} (excerpt):\n{text[:EXCERPT_CHARS]}")
    return "\n\n".join(parts)


def reorder(paths, scores):
    if len(scores) != len(paths) or not all(math.isfinite(s) and 0 <= s <= 1 for s in scores):
        raise ValueError("Invalid model scores")
    return [p for p, _ in sorted(zip(paths, scores), key=lambda pair: -pair[1])]


def focused(repo, brief, trace):
    brief["needs"][0]["where"] = trace["all_locations"]
    return gather.focus(repo, brief, spread=True)


def live_reply(repo, args, score, example=None):
    """Return the live all-spread reply using Qwen file scores and the frozen focus budget."""
    with patch.object(repo, "search", cache(repo.search)):
        wide = {}
        with patch.object(gather, "TOP_FILES", WIDTH):
            gather.gather(repo, args, wide)
        paths = [p for p, _ in wide["ranked"][:WIDTH]]
        locations = defaultdict(list)
        for row in wide["all_locations"]:
            locations[row["at"].rpartition(":")[0]].append(row["at"])
        query = "\n".join([args.get("task", ""), *args.get("needs", []), *args.get("anchors", [])])
        pairs = [(query, document(repo, p, locations[p])) for p in paths]
        scores = score(INSTRUCTION, pairs) if pairs else []
        prefix = reorder(paths, scores) if pairs else []
        if pairs and example is not None:
            query, card = pairs[0]
            rendered = PREFIX + pair_text(INSTRUCTION, query, card) + SUFFIX
            example.update(file=paths[0], source_sha256=repo.digests[paths[0]].hex(),
                           input_position=1, pool_size=len(paths), score=scores[0], instruction=INSTRUCTION,
                           query=query, card=card, card_sha256=common.sha256_bytes(card.encode("utf-8")),
                           before_token_cap=rendered, before_token_cap_sha256=common.sha256_bytes(rendered.encode("utf-8")))

        def rank_rule(ranked, held, index, needs):
            if ranked[:WIDTH] != paths:
                raise ValueError("Live file pool changed")
            return prefix + ranked[WIDTH:]

        trace = {}
        return focused(repo, gather.gather(repo, args, trace, rank_rule=rank_rule), trace)


def evaluate(repo, args, judge):
    with patch.object(repo, "search", cache(repo.search)):
        start = perf_counter()
        base = {}
        control = focused(repo, gather.gather(repo, args, base), base)
        control_seconds = perf_counter() - start
        wide = {}
        with patch.object(gather, "TOP_FILES", WIDTH):
            gather.gather(repo, args, wide)
        original = [p for p, _ in base["ranked"]]
        if wide["ranked"] != base["ranked"]:
            raise ValueError("Candidate extraction changed the original order")
        pool = original[:WIDTH]
        by_file = defaultdict(list)
        for row in wide["all_locations"]:
            by_file[row["at"].rpartition(":")[0]].append(row["at"])
        query = "\n".join([args["task"], *args.get("needs", []), *args.get("anchors", [])])
        pairs = [(query, document(repo, p, by_file[p])) for p in pool]
        clipped = judge.clipped
        start = perf_counter()
        scores = judge.score(INSTRUCTION, pairs)
        model_seconds = perf_counter() - start
        new_order = reorder(pool, scores) + original[WIDTH:]
        def rank_rule(ranked, held, index, needs):
            if ranked != original:
                raise ValueError("Paired order changed")
            return new_order
        trial = {}
        candidate = focused(repo, gather.gather(repo, args, trial, rank_rule=rank_rule), trial)
    return control, candidate, original, new_order, {"scores": list(zip(pool, scores)),
            "clipped": judge.clipped - clipped, "model_s": model_seconds, "control_s": control_seconds}


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("campaign", type=Path)
    parser.add_argument("checkpoint", type=Path)
    args = parser.parse_args()
    campaign = args.campaign
    calls = json.loads((campaign / "replay/first-calls-r1.json").read_text(encoding="utf-8"))["calls"]
    prior = {r["task"]: r for s in (campaign / "replay/focus-before-trim-r1.jsonl").read_text(encoding="utf-8").splitlines()
             if (r := json.loads(s))["rule"] == "all-spread"}
    rows = {r["id"]: r for r in agent_cost_batch.working_set(campaign)}
    if calls.keys() != rows.keys() or prior.keys() != rows.keys():
        raise ValueError("Mismatched task set")
    start = perf_counter()
    judge = Judge(args.checkpoint, max_tokens=1024, batch=8, bits=8)
    print(json.dumps({"device": judge.torch.cuda.get_device_name(), "torch": str(judge.torch.__version__),
                      "load_s": perf_counter() - start, "weights": judge.weights}), flush=True)
    repos = {}
    for key, arguments in sorted(calls.items()):
        row = rows[key]
        src = row["snapshot"]
        if src not in repos:
            index = json.loads((campaign / "topical" / f"{src.name}.map.json").read_text(encoding="utf-8"))
            repos[src] = gather.Repository(src, index)
        control, candidate, before, after, stats = evaluate(repos[src], arguments, judge)
        task = json.loads(row["task"].read_text(encoding="utf-8"))
        result = {}
        for label, reply, order in (("control", control, before), ("qwen", candidate, after)):
            notes = classify(task, {**reply, "omitted": gather.census_spans(reply)}, {})
            ranks = {p: i for i, p in enumerate(order, 1)}
            result[label] = {"notes": notes, "ranks": {p: ranks.get(p) for p in rank_check.truth_files(task)},
                             "bytes": sum(len(r["text"].encode("utf-8")) for r in reply["passages"])}
        if result["control"]["notes"] != prior[key]["notes"] or result["control"]["ranks"] != prior[key]["truth"]:
            raise ValueError("Control differs from frozen replay")
        print(json.dumps({"task": key, **result, **stats}), flush=True)


if __name__ == "__main__":
    main()
