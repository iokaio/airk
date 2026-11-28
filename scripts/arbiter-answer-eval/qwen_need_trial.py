# SPDX-License-Identifier: Apache-2.0
"""Replay the frozen development calls with matched Qwen prompts and independently graded citations."""

import argparse
from collections import defaultdict
from functools import cache
import hashlib
import json
from pathlib import Path
from time import perf_counter
from unittest.mock import patch

import agent_cost_batch
import common
import fixture
import live_gather as g
import qwen_file_trial as legacy
import rank_check
from arbiter_pair_probe import PREFIX, SUFFIX, pair_text
from gather_context_call_replay import classify

ARMS = ("control", "per-need", "fixed40", "labeled", "direct-wording", "direct", "no-anchors")
DIRECT = ("Given a coding agent's task, determine whether the supplied source provides direct evidence for this need. "
          "A caller, rule, example, or test may provide useful partial evidence.")
BUDGET = g.FOCUS_PASSAGE_BYTES


def sha(text):
    return hashlib.sha256(text.encode("utf-8")).hexdigest()


def complete_card(repo, path, locations):
    """One contiguous, line-complete excerpt of the first nominated source range that fits."""
    for at in locations:
        first, last = map(int, at.rsplit(":", 1)[1].split("-"))
        lines = []
        for n in range(first, last + 1):
            line = repo.texts[path][n - 1]
            if len("\n".join(lines + [line])) > legacy.EXCERPT_CHARS:
                if lines:
                    break
                continue
            if not lines:
                begin = n
            lines.append(line)
        if lines and any(x.strip() for x in lines):
            end = begin + len(lines) - 1
            loc = f"{path}:{begin}-{end}"
            body = "\n".join(lines)
            names = [name for name, ns in sorted(repo.index["files"][path].get("defines", {}).items())
                     if any(begin <= n <= end and name in repo.texts[path][n - 1] for n in ns)]
            symbol = "\nDeclaration: " + ", ".join(names) if names else ""
            return f"Path: {path}\nLines: {begin}-{end}{symbol}\nSource:\n{body}", loc, at
    return f"Path: {path}\nSource: no complete source lines available", None, None


def interleave(rosters):
    """Give every need one new nomination in each round. Stable source order breaks score ties."""
    ordered, seen = [], set()
    iters = [iter(row) for row in rosters]
    while True:
        old = len(ordered)
        for it in iters:
            new = next((path for path in it if path not in seen), None)
            if new is not None:
                seen.add(new)
                ordered.append(new)
        if len(ordered) == old:
            return ordered


class ModelCache:
    def __init__(self, judge):
        self.judge = judge
        self.values = {}
        self.truncated = {}

    def score(self, instruction, pairs):
        missing = list(dict.fromkeys(p for p in pairs if (instruction, p) not in self.values))
        start, clipped = perf_counter(), self.judge.clipped
        if missing:
            for query, document in missing:
                text = pair_text(instruction, query, document)
                self.truncated[instruction, (query, document)] = len(self.judge.tokenizer.encode(text, add_special_tokens=False)) > self.judge.room
            scores = self.judge.score(instruction, missing)
            legacy.reorder(missing, scores)
            self.values.update({(instruction, p): s for p, s in zip(missing, scores)})
        return ([self.values[instruction, p] for p in pairs], len(missing),
                self.judge.clipped - clipped, perf_counter() - start)


def readout(repo, brief, order, task, assigned=None):
    keep = []
    for row in brief["passages"]:
        at = row["at"]
        keep.append({"at": at, "needs": sorted((assigned or {}).get(at, ())), "sha256": sha(row["text"])})
    return {"passages": keep, "notes": classify(task, {**brief, "omitted": g.census_spans(brief)}, {}),
            "ranks": {p: order.index(p)+1 if p in order else None for p in rank_check.truth_files(task)},
            "passage_bytes": sum(len(r["text"].encode()) for r in brief["passages"]), "budget": BUDGET}


def run_task(repo, args, task, historical, judge, arms, verify, task_id, original_source=None):
    trace = {}
    base = g.gather(repo, args, trace)
    all_paths = [p for p, _ in trace["ranked"]]
    paths = all_paths[:legacy.WIDTH]
    ordinary = readout(repo, legacy.focused(repo, base, trace), all_paths, task)
    paths = [p for p, _ in historical["scores"]]
    denied = {p.casefold() for mode, p in fixture.EXCLUSIONS.get(task["repository"], [])}
    if (len(paths) != legacy.WIDTH or len(set(paths)) != len(paths)
            or any(p.casefold() in denied or p not in repo.texts or p not in all_paths for p in paths)):
        raise ValueError("Historical pool includes an unavailable, duplicate or excluded file")
    original_paths = all_paths
    all_paths = paths + [p for p in original_paths if p not in set(paths)]
    def pinned(ranked, held, index, needs):
        if ranked != original_paths:
            raise ValueError("Sanitized extraction changed during replay")
        return all_paths
    wide = {}
    with patch.object(g, "TOP_FILES", legacy.WIDTH):
        g.gather(repo, args, wide, rank_rule=pinned)
    ordinary["historical_delta"] = {k: {p: {"historical": v, "sanitized": ordinary[k].get(p)}
                                       for p, v in historical["control"][k].items() if ordinary[k].get(p) != v}
                                    for k in ("notes", "ranks")}
    ordinary["live_top40"] = original_paths[:legacy.WIDTH]
    locs = defaultdict(list)
    for row in wide["all_locations"]:
        locs[row["at"].rsplit(":", 1)[0]].append(row["at"])
    cards = {p: legacy.document(repo, p, locs[p]) for p in paths}
    short = {p: complete_card(repo, p, locs[p]) for p in paths}
    identity = [{"file": p, "source_sha256": common.sha256_file(repo.source / p), "original_card_sha256": sha(cards[p]),
                 "direct_card_sha256": sha(short[p][0]), "scored_at": short[p][1], "returned_at": short[p][2], "locations": locs[p]} for p in paths]
    if original_source is not None and original_source != repo.source:
        for r in identity:
            if common.sha256_file(original_source / r["file"]) != r["source_sha256"]:
                raise ValueError("Allowed pooled source differs from historical source")
    needs, anchors = args["needs"], args.get("anchors", [])
    if not 1 <= len(needs) <= 8 or not paths:
        raise ValueError("Invalid need count or empty file pool")
    cache_ = ModelCache(judge)
    outputs = {"deterministic": dict(ordinary, per_need={}, pairs=0, clipped=0)}
    for arm in arms:
        start = perf_counter()
        bundled, passage = arm == "control", arm == "direct"
        instruction = DIRECT if arm in ("direct-wording", "direct") else legacy.INSTRUCTION
        groups = [list(enumerate(paths))] if bundled else [
            [(i, p) for i, p in enumerate(paths) if arm == "per-need" or i % len(needs) == n] for n in range(len(needs))]
        candidates = []
        for n, group in enumerate(groups):
            query = "\n".join([args["task"], *(needs if bundled else [needs[n]]), *(anchors if arm != "no-anchors" else [])])
            if arm in ("labeled", "direct-wording", "direct"):
                query = f"Task: {args['task']}\nNeed: {needs[n]}\nExact strings: " + "\n".join(anchors)
            for i, path in group:
                doc = short[path][0] if passage else cards[path]
                candidates.append((n, i, path, query, doc))
        pairs = [(q, d) for _, _, _, q, d in candidates]
        scores, executed, clipped, seconds = cache_.score(instruction, pairs)
        rosters = [[] for _ in groups]
        values = []
        for (n, i, path, q, d), score in zip(candidates, scores):
            values.append({"need": None if bundled else n+1, "file": path, "pool_position": i+1, "score": score,
                           "rendered_sha256": sha(PREFIX + pair_text(instruction, q, d) + SUFFIX)})
            rosters[n].append((path, score))
        orders = [legacy.reorder([p for p, _ in r], [s for _, s in r]) for r in rosters]
        head = interleave(orders)
        order = head + all_paths[len(paths):]
        if sorted(head) != sorted(paths):
            raise ValueError("Lost or duplicated a pooled candidate")
        def rank_rule(ranked, held, index, needs):
            if ranked != original_paths:
                raise ValueError("Source pool changed during replay")
            return order
        reranked = {}
        brief = g.gather(repo, args, reranked, rank_rule=rank_rule)
        assigned = {}
        if passage:
            for n, roster in enumerate(orders, 1):
                for path in roster:
                    if short[path][1]:
                        assigned[short[path][2]] = [n]
            focused = legacy.focused(repo, brief, reranked)
            for row in focused["passages"]:
                row["needs"] = assigned.get(row["at"], [])
            focused["needs"] = [{"need": n, "text": text, "status": "candidate_evidence" if cited else "no_candidate_evidence",
                                 "where": [{"at": at} for at in cited]}
                                for n, text in enumerate(needs, 1)
                                for cited in [[r["at"] for r in focused["passages"] if n in r["needs"]]]]
        else:
            focused = legacy.focused(repo, brief, reranked)
        out = readout(repo, focused, order, task, assigned)
        per_need = {} if bundled else {str(n): [{"file": p, "at": [short[p][1]] if passage and short[p][1] else [],
                                                  "scored_at": short[p][1] if passage else None,
                                                  "returned_at": short[p][2] if passage else None}
                                                 for p in r] for n, r in enumerate(orders, 1)}
        if passage:
            out["scored_support"] = [{"need": n, "scored_at": short[p][1], "returned_at": short[p][2]}
                                     for n, r in enumerate(orders, 1) for p in r
                                     if short[p][2] in {x["at"] for x in focused["passages"]}]
            out["reply_needs"] = focused["needs"]
        if arm == "fixed40" and task_id in ("bookbuilder-A1", "munarium-A1"):
            measured = ModelCache(judge)
            fresh, actual_count, actual_clipped, elapsed = measured.score(instruction, pairs)
            independent_orders = []
            for n in range(len(groups)):
                ns = [(c[2], value) for c, value in zip(candidates, fresh) if c[0] == n]
                independent_orders.append(legacy.reorder([p for p, _ in ns], [s for _, s in ns]))
            out["standalone_timing"] = {"executed_pairs": actual_count, "clipped": actual_clipped, "model_seconds": elapsed,
                                        "max_abs_score_delta": max(abs(a-b) for a,b in zip(scores,fresh)),
                                        "same_per_need_ranks": independent_orders == orders, "scores": fresh}
        q, doc = pairs[0]
        ids = judge.tokenizer.encode(pair_text(instruction, q, doc), add_special_tokens=False)
        tokens = judge.prefix + ids[:judge.room] + judge.suffix
        example = {"instruction": instruction, "query": q, "document": doc, "file": candidates[0][2],
                   "token_ids": tokens, "decoded_model_input": judge.tokenizer.decode(tokens, skip_special_tokens=False)}
        if bundled:
            original_query = "\n".join([args["task"], *args.get("needs", []), *args.get("anchors", [])])
            old_pairs = [(original_query, legacy.document(repo, p, locs[p])) for p in paths]
            out["control_check"] = verify(paths, pairs, scores, head, old_pairs)
            old_order = legacy.reorder(paths, [v for _, v in historical["scores"]])
            out["control_check"].update(historical_rank_checked=True, historical_rank_equal=head == old_order,
                                        historical_score_max_delta=max(abs(v-h) for v, (_,h) in zip(scores,historical["scores"])))
            out["historical_delta"] = {k: {p: {"historical": v, "sanitized": out[k].get(p)}
                                           for p, v in historical["qwen"][k].items() if out[k].get(p) != v}
                                       for k in ("notes", "ranks")}
        outputs[arm] = dict(out, pairs=len(pairs), executed_pairs=executed, executed_clipped=clipped,
                            clipped=sum(cache_.truncated[instruction, p] for p in pairs), model_seconds=seconds,
                            arm_wall_seconds=perf_counter()-start, example=example, scores=values, per_need=per_need)
    return identity, outputs


def safe_repository(campaign, row):
    src = row["snapshot"]
    map_path = campaign / "topical" / f"{src.name}.map.json"
    source = src
    if row["repository"] == "bookbuilder":
        source = campaign / "agent-cost/E2E-r1/snapshots/bookbuilder"
        if not source.is_dir() or any((source / p).exists() for rule, p in fixture.EXCLUSIONS["bookbuilder"]):
            raise ValueError("Sanitized Bookbuilder snapshot missing or protected path present")
        index = g.topical_index.build(source)
    else:
        index = common.read_json(map_path)
    return g.Repository(source, index)


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("campaign", type=Path)
    parser.add_argument("checkpoint", type=Path)
    parser.add_argument("--arms", nargs="+", choices=ARMS, default=list(ARMS))
    parser.add_argument("--output", type=Path, required=True)
    args = parser.parse_args()
    if args.arms[0] != "control" or len(set(args.arms)) != len(args.arms):
        parser.error("Start with control and specify each arm once")
    from qwen_replay_conformance import load_verified_judge, verify_control
    campaign = args.campaign
    calls_path = campaign / "replay/first-calls-r1.json"
    control_path = campaign / "replay/qwen-file-top40-r1.jsonl"
    calls = common.read_json(calls_path)["calls"]
    past = {r["task"]: r for r in common.read_jsonl(control_path) if "task" in r}
    rows = {r["id"]: r for r in agent_cost_batch.working_set(campaign)}
    if calls.keys() != rows.keys() or past.keys() != rows.keys() or len(rows) != 20:
        raise ValueError("Frozen twenty-task panel mismatch")
    with args.output.open("x", encoding="utf-8") as stream:
        def emit(data):
            print(json.dumps(data, ensure_ascii=True), file=stream, flush=True)
        judge, receipt = load_verified_judge(args.checkpoint)
        emit({"type": "run", "model": receipt, "arms": args.arms, "calls_sha256": common.sha256_file(calls_path),
              "historical_sha256": common.sha256_file(control_path), "tasks": sorted(rows), "width": legacy.WIDTH,
              "excerpt_chars": legacy.EXCERPT_CHARS, "focus_bytes": BUDGET, "fixed_allocation": "pool position modulo need count"})
        repos = {}
        for key, arguments in sorted(calls.items()):
            src = rows[key]["snapshot"]
            map_path = campaign / "topical" / f"{src.name}.map.json"
            if src not in repos:
                repos[src] = safe_repository(campaign, rows[key])
            repo = repos[src]
            task_path = rows[key]["task"]
            with patch.object(repo, "search", cache(repo.search)):
                pool, result = run_task(repo, arguments, common.read_json(task_path), past[key], judge, args.arms, verify_control, key, src)
            emit({"type": "task", "task": key, "arguments": arguments, "task_sha256": common.sha256_file(task_path),
                  "historical_map_sha256": common.sha256_file(map_path), "actual_file_records_sha256":
                  sha(json.dumps(repo.index["files"], sort_keys=True, separators=(",", ":"))),
                  "source": str(repo.source), "pool": pool, "variants": result})


if __name__ == "__main__":
    main()
