# SPDX-License-Identifier: Apache-2.0
"""Can the index's own lookups find what a task's truth names?

Evaluation tooling for the Gather Context strategy of 27 November 2026. An
acting agent with plain file tools finds its evidence by searching for exact
names. This probe asks whether the index can do the same when it is given
the right name, which separates two causes of a weak Gather Context reply:
the index cannot find the place, or the kernel never asks for it.

For every essential note of a task that names a ``symbol`` and a location,
it looks the name up three ways against a fixture lifetime, by native tool:

- ``symbols``: the definition lookup, by the name's last segment;
- ``lexical``: an exact lexical search of that segment, 32 hits;
- ``semantic``: the default search of the note's fact text, 16 hits.

A lookup ``hits`` when a returned location overlaps the note's lines, and
reaches the ``file`` when it returns the note's file at other lines.

    python -B index_recall_probe.py --campaign <dir> --handle <handle.json> --output <new file.json>
"""

from __future__ import annotations

import argparse
import concurrent.futures
import json
import pathlib
import re
import sys

import agent_cost_batch
import common
import evidence

SCHEMA = "airk.private.index-recall-probe/1"
LOOKUPS = ("symbols", "lexical", "semantic")


def last_segment(symbol: str) -> str:
    names = re.findall(r"[A-Za-z_][A-Za-z0-9_]*", symbol)
    return names[-1] if names else symbol


def outcome_of(spans: list[evidence.Span], note: dict) -> str:
    first, last = note["lines"][0], note["lines"][-1]
    same = [span for span in spans if evidence.path_matches(span.path, note["path"])]
    if any(span.start is not None and span.start <= last and (span.end or span.start) >= first for span in same):
        return "hit"
    return "file" if same else "miss"


def main(argv: list[str] | None = None) -> int:
    parser = argparse.ArgumentParser(description=__doc__.splitlines()[0])
    parser.add_argument("--campaign", required=True, type=pathlib.Path)
    parser.add_argument("--handle", required=True, type=pathlib.Path)
    parser.add_argument("--output", required=True, type=pathlib.Path)
    parser.add_argument("--workers", type=int, default=4)
    parser.add_argument("--timeout", type=float, default=120.0)
    args = parser.parse_args(argv)
    handle = common.load_handle(args.handle.resolve(strict=True))
    jobs = []
    for row in agent_cost_batch.working_set(args.campaign):
        task = json.loads(row["task"].read_text(encoding="utf-8"))
        for note in task.get("notes") or []:
            if note.get("mark") == "essential" and note.get("path") and not note.get("absence") and note.get("symbol"):
                jobs.append((row, note))

    def spans(index: str, tool: str, arguments: dict) -> list[evidence.Span]:
        outcome, _ = common.call_tool_retrying(handle, tool, index, {**arguments, "response_mode": "full"}, args.timeout)
        return evidence.extract(common.envelope_body(outcome.envelope)) if outcome.ok else []

    def one(job) -> dict:
        row, note = job
        name = last_segment(note["symbol"])
        found = {"symbols": spans(row["index"], "airk_symbols", {"name": name}),
                 "lexical": spans(row["index"], "airk_search", {"query": name, "mode": "lexical", "limit": 32}),
                 "semantic": spans(row["index"], "airk_search", {"query": note.get("fact") or name, "limit": 16})}
        return {"task": row["id"], "note": note["id"], "name": name, "path": note["path"],
                **{lookup: outcome_of(found[lookup], note) for lookup in LOOKUPS}}

    with concurrent.futures.ThreadPoolExecutor(args.workers) as pool:
        rows = list(pool.map(one, jobs))
    total = {lookup: {state: sum(row[lookup] == state for row in rows) for state in ("hit", "file", "miss")} for lookup in LOOKUPS}
    total["any_hit"] = sum(any(row[lookup] == "hit" for lookup in LOOKUPS) for row in rows)
    total["exact_hit"] = sum(row["symbols"] == "hit" or row["lexical"] == "hit" for row in rows)
    for row in rows:
        print(f"{row['task']:<20} {row['note']:<4} {row['name'][:34]:<34} " + " ".join(f"{lookup}={row[lookup]}" for lookup in LOOKUPS))
    print(json.dumps({"notes": len(rows), **total}))
    args.output.write_text(json.dumps({"schema": SCHEMA, "handle": str(args.handle), "total": total, "notes": rows},
                                      indent=2, sort_keys=True) + "\n", encoding="utf-8", newline="\n")
    return 0


if __name__ == "__main__":
    sys.exit(main())
