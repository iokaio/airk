# SPDX-License-Identifier: Apache-2.0
"""Replay recorded Gather Context calls and score the replies against the truth.

Evaluation tooling for the Gather Context strategy of 27 November 2026. An
agent cost series is the measure, and it is dear. This is the cheap check
used between kernel changes: it sends the calls that acting agents really
wrote, with no agent, and says how much of each task's frozen truth the
reply puts in front of an agent. It is a diagnostic of the reply, not a
measure of success; cost is measured only by a series.

``collect`` takes each task's first Gather Context call from the sessions
of a series and writes them to one replay file.

``run`` sends every recorded call to a fixture lifetime twice, in the
default mode (the brief an agent receives) and in full mode, keeps both
replies and classifies each essential note of the task that has a location:

- ``passage``: its lines are in a passage the brief carries;
- ``where``: the brief names its location without carrying the text;
- ``gathered``: only the full reply holds it, so the kernel fetched it and
  the brief left it out;
- ``missed``: the kernel never fetched it.

    python -B gather_context_call_replay.py collect --campaign <dir> --series <name> --output <calls.json>
    python -B gather_context_call_replay.py run --campaign <dir> --calls <calls.json> --handle <handle.json> --output <new dir> [--tasks <id> ...]
"""

from __future__ import annotations

import argparse
import concurrent.futures
import json
import pathlib
import sys

import agent_cost_batch
import common
import evidence

SCHEMA = "airk.private.gather-context-replay/1"
STAGES = ("passage", "where", "gathered", "missed")


def span_of(at: str, text: str = "x") -> evidence.Span | None:
    """A brief location ``path:start-end`` as a span."""
    path, _, lines = at.rpartition(":")
    start, _, end = lines.partition("-")
    if not path or not start.isdigit() or not end.isdigit():
        return None
    return evidence.Span(evidence.norm_path(path), int(start), int(end), text, "text")


def classify(task: dict, brief: dict, full: dict) -> dict[str, str]:
    """Where each located essential note of ``task`` stands in the two replies."""
    passages = [span for item in brief.get("passages") or [] if (span := span_of(item.get("at", ""), item.get("text") or "x"))]
    where = [span for need in brief.get("needs") or [] for row in need.get("where") or [] if (span := span_of(row.get("at", "")))]
    where += [span for at in brief.get("omitted") or [] if isinstance(at, str) and (span := span_of(at))]
    gathered = [span for record in full.get("records") or [] for span in evidence.extract(record.get("canonical_result"))
                if span.kind == "text" and span.start is not None]
    stages = {}
    for note in task.get("notes") or []:
        if note.get("mark") != "essential" or note.get("absence") or not note.get("path"):
            continue
        stages[note["id"]] = ("passage" if evidence.covers(passages, note) else "where" if evidence.covers(where, note)
                              else "gathered" if evidence.covers(gathered, note) else "missed")
    return stages


def collect(args) -> int:
    series = args.campaign / "agent-cost" / args.series
    calls = {}
    for path in sorted(series.glob("*/gather/calls.jsonl")):
        task_id = path.parent.parent.name.rsplit(".", 2)[0]
        rows = common.read_jsonl(path)
        if rows and task_id not in calls:
            calls[task_id] = rows[0]["arguments"]
    args.output.write_text(json.dumps({"schema": SCHEMA, "series": args.series, "calls": calls}, indent=2, sort_keys=True) + "\n",
                           encoding="utf-8", newline="\n")
    print(f"{len(calls)} first call(s) from {args.series}")
    return 0


def run(args) -> int:
    recorded = json.loads(args.calls.read_text(encoding="utf-8"))["calls"]
    rows = {row["id"]: row for row in agent_cost_batch.working_set(args.campaign)}
    wanted = [task_id for task_id in sorted(recorded) if task_id in rows and (not args.tasks or task_id in args.tasks)]
    handle = common.load_handle(args.handle.resolve(strict=True))
    args.output.mkdir(parents=True)

    def one(task_id: str) -> dict:
        row, arguments = rows[task_id], recorded[task_id]
        replies = {}
        for mode in ("terse", "full"):
            sent = arguments if mode == "terse" else {**arguments, "response_mode": "full"}
            outcome = common.call_tool(handle, "airk_answer", row["index"], sent, args.timeout, strip_env=common.AGENT_STRIPPED_ENV)
            text = common.envelope_text(outcome.envelope) or ""
            (args.output / f"{task_id}.{mode}.json").write_text(text, encoding="utf-8", newline="\n")
            if not outcome.ok:
                return {"task": task_id, "error": (outcome.error or text)[:300]}
            replies[mode] = json.loads(text)
        brief, full = replies["terse"], replies["full"]
        stages = classify(json.loads(row["task"].read_text(encoding="utf-8")), brief, full)
        counts = {stage: sum(value == stage for value in stages.values()) for stage in STAGES}
        return {"task": task_id, "set": row["set"], "notes": stages, **counts,
                "brief_bytes": len(json.dumps(brief, ensure_ascii=False, separators=(",", ":")).encode("utf-8")),
                "passages": len(brief.get("passages") or []), "result": brief.get("result"), "stop": brief.get("stop"),
                "elapsed_ms": full.get("elapsed_ms"), "records": len(full.get("records") or []),
                "gathering": brief.get("gathering")}

    with concurrent.futures.ThreadPoolExecutor(args.workers) as pool:
        results = list(pool.map(one, wanted))
    total = {stage: 0 for stage in STAGES}
    for result in results:
        if "error" in result:
            print(f"{result['task']:<22} ERROR {result['error']}")
            continue
        for stage in STAGES:
            total[stage] += result[stage]
        missing = [note for note, stage in result["notes"].items() if stage != "passage"]
        print(f"{result['task']:<22} passage={result['passage']} where={result['where']} gathered={result['gathered']} "
              f"missed={result['missed']} | {result['brief_bytes']} bytes, {result['records']} records, {result['elapsed_ms']} ms, "
              f"{result['stop']} | not carried: {' '.join(f'{note}:{result['notes'][note]}' for note in missing)}")
    print(json.dumps({"tasks": len(results), **total}))
    (args.output / "replay.json").write_text(json.dumps({"schema": SCHEMA, "handle": str(args.handle), "total": total,
                                                         "tasks": results}, indent=2, sort_keys=True) + "\n",
                                             encoding="utf-8", newline="\n")
    return 0


def main(argv: list[str] | None = None) -> int:
    parser = argparse.ArgumentParser(description=__doc__.splitlines()[0])
    parser.add_argument("command", choices=("collect", "run"))
    parser.add_argument("--campaign", required=True, type=pathlib.Path)
    parser.add_argument("--series")
    parser.add_argument("--calls", type=pathlib.Path)
    parser.add_argument("--handle", type=pathlib.Path)
    parser.add_argument("--output", required=True, type=pathlib.Path)
    parser.add_argument("--tasks", nargs="*")
    parser.add_argument("--workers", type=int, default=2)
    parser.add_argument("--timeout", type=float, default=600.0)
    args = parser.parse_args(argv)
    if args.command == "collect":
        if not args.series:
            parser.error("collect needs --series")
        return collect(args)
    if args.calls is None or args.handle is None:
        parser.error("run needs --calls and --handle")
    return run(args)


if __name__ == "__main__":
    sys.exit(main())
