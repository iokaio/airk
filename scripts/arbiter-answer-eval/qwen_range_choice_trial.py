# SPDX-License-Identifier: Apache-2.0
"""Compare 40 and 80 need/range judgments using only frozen caller inputs, eligible files and source ranges.

Run with the same verified CUDA Python and immutable snapshots as qwen_need_trial.py:
    python -B qwen_range_choice_trial.py CAMPAIGN CHECKPOINT --paired MERGED_JSONL --output NEW_JSONL
All selection is in range_choice.py. No grading notes or prior scores are inputs to the scorer.
"""
import argparse
import json
from pathlib import Path, PureWindowsPath
import stat
from time import perf_counter
from types import SimpleNamespace

import agent_cost_batch
from arbiter_pair_probe import PREFIX, SUFFIX, pair_text
import common
import fixture
from qwen_need_trial import DIRECT
from qwen_replay_conformance import load_verified_judge
import range_choice
import topical_index


def checked_source(root, relative, denies):
    parts = relative.split("/")
    if (PureWindowsPath(relative).drive or "\\" in relative or ":" in relative
            or any(p in ("", ".", "..") or p.endswith((" ", ".")) for p in parts)):
        raise ValueError("Unsafe source path")
    folded = relative.casefold()
    for mode, denied in denies:
        if mode not in ("exact", "subtree"):
            raise ValueError("Unknown source policy")
        prefix = denied.casefold()
        if folded == prefix or (mode == "subtree" and folded.startswith(prefix + "/")):
            raise ValueError("Source policy forbids candidate path")
    path = root
    for part in parts:
        path = path / part
        info = path.lstat()
        if stat.S_ISLNK(info.st_mode) or getattr(info, "st_file_attributes", 0) & stat.FILE_ATTRIBUTE_REPARSE_POINT:
            raise ValueError("Source path has a symbolic link")
    if not stat.S_ISREG(path.stat().st_mode):
        raise ValueError("Expected regular source file")
    return path


def case_inputs(campaign, paired, inventory):
    task = inventory[paired["task"]]
    root = (campaign / "agent-cost/E2E-r1/snapshots/bookbuilder" if task["repository"] == "bookbuilder"
            else task["snapshot"]).resolve(strict=True)
    if root != Path(paired["source"]).resolve(strict=True):
        raise ValueError("Source binding differs from the frozen replay")
    pool = [{key: p[key] for key in ("file", "locations", "source_sha256")} for p in paired["pool"]]
    if len(pool) != 40 or len({p["file"] for p in pool}) != 40:
        raise ValueError("Unmatched file pool")
    denies = fixture.EXCLUSIONS.get(task["repository"], [])
    texts, records = {}, {}
    for row in pool:
        path = row["file"]
        raw = checked_source(root, path, denies).read_bytes()
        if common.sha256_bytes(raw) != row["source_sha256"]:
            raise ValueError("Pooled source bytes changed")
        text = raw.decode("utf-8", errors="replace")
        texts[path] = text.splitlines()
        records[path] = topical_index.file_record(path, text, len(raw))
    return SimpleNamespace(texts=texts, index={"files": records}), pool


def measure(judge, args, pool, repo, *, generated=False):
    started = perf_counter()
    source_audit = {}
    if generated:
        import source_range_generation
        proposed, source_audit = source_range_generation.nominate(repo, pool, args, 80)
    else:
        proposed = range_choice.nominate(repo, pool, args, 80)
    pairs = [(r.query, r.document) for r in proposed]
    tokens = [judge.tokenizer.encode(pair_text(DIRECT, q, d), add_special_tokens=False) for q, d in pairs]
    before = judge.clipped
    score_start = perf_counter()
    scores = judge.score(DIRECT, pairs) if pairs else []
    seconds = perf_counter() - score_start
    clipped = sum(len(ids) > judge.room for ids in tokens)
    if judge.clipped - before != clipped:
        raise ValueError("Tokenizer clipping count inconsistent with loaded model")
    full = []
    for i, (item, score) in enumerate(zip(proposed, scores)):
        rendered = PREFIX + pair_text(DIRECT, item.query, item.document) + SUFFIX
        full.append({"need": item.need, "file": item.source.path, "pool_position": item.source.file_rank,
                     "scored_at": item.scored_at, "returned_at": item.source.at,
                     "source_sha256": pool[item.source.file_rank-1]["source_sha256"],
                     "rendered_sha256": common.sha256_bytes(rendered.encode()), "score": score,
                     "tokens_before_cap": len(tokens[i]) + len(judge.prefix) + len(judge.suffix)})
    if len(scores) != len(proposed):
        raise ValueError("Incomplete score response")
    variants = {}
    for limit in (40, 80):
        rows = proposed[:limit]
        out = range_choice.assemble(args.get("needs") or [args["task"]], rows, scores[:limit])
        for row in out["passages"]:
            row["sha256"] = common.sha256_bytes(row.pop("text").encode())
        variants["range" + str(limit)] = dict(out, pairs=len(rows), scores=full[:limit],
                                              clipped=sum(len(ids) > judge.room for ids in tokens[:limit]))
    sample = None
    if proposed:
        r = proposed[0]
        sample = {"query": r.query, "document": r.document, "need": r.need, "scored_at": r.scored_at,
                  "decoded_model_input": judge.tokenizer.decode(judge.prefix + tokens[0][:judge.room] + judge.suffix,
                                                               skip_special_tokens=False)}
    return {"variants": variants, "sample": sample, "executed_pairs": len(scores),
            "model_seconds": seconds, "wall_seconds": perf_counter() - started, **source_audit}


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("campaign", type=Path)
    parser.add_argument("checkpoint", type=Path)
    parser.add_argument("--paired", type=Path, required=True)
    parser.add_argument("--output", type=Path, required=True)
    parser.add_argument("--generate-ranges", action="store_true", help="alternate original and source-generated nominations")
    args = parser.parse_args()
    header, *rows = common.read_jsonl(args.paired)
    inventory = {r["id"]: r for r in agent_cost_batch.working_set(args.campaign)}
    ids = [r["task"] for r in rows]
    if (header["type"] != "run" or set(header["tasks"]) != set(inventory)
            or len(ids) != 20 or set(ids) != set(inventory)):
        raise ValueError("Expected complete, unique 20-task paired receipt")
    caller_path = args.campaign / "replay/first-calls-r1.json"
    if common.sha256_file(caller_path) != header["calls_sha256"]:
        raise ValueError("Caller input freeze changed")
    calls = common.read_json(caller_path)["calls"]
    judge, audit = load_verified_judge(args.checkpoint)
    if audit != header["model"]:
        raise ValueError("Loaded scorer differs from the original paired experiment")
    with args.output.open("x", encoding="utf-8") as output:
        def emit(row):
            print(json.dumps(row, ensure_ascii=True), file=output, flush=True)
        generation_sha = None
        if args.generate_ranges:
            import source_range_generation
            generation_sha = common.sha256_file(Path(source_range_generation.__file__))
        emit({"type":"run", "generated_sha256":generation_sha, "model":audit, "paired_sha256":common.sha256_file(args.paired), "tasks":ids,
              "implementation_sha256":common.sha256_file(Path(range_choice.__file__)),
              "scoring_basis":"one 80-pair measurement; range40 reuses its prefix scores"})
        for row in rows:
            tid = row["task"]
            if row["arguments"] != calls[tid]:
                raise ValueError("Scorer arguments differ from frozen task")
            repo, pool = case_inputs(args.campaign, row, inventory)
            emit({"type":"task", "task":tid, "source":row["source"],
                  **measure(judge, calls[tid], pool, repo, generated=args.generate_ranges)})


if __name__ == "__main__":
    main()
