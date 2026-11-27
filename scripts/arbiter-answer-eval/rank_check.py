# SPDX-License-Identifier: Apache-2.0
"""Where each task's truth files fall in the topical prototype's file ranking.

Evaluation tooling for ``docs/arbiter/topical-index-research.md`` (ranking is
the prototype's limit, 27 November 2026). For every recorded agent call it
runs ``live_gather.gather`` with a trace and reports, per truth file: its rank,
the request terms it matched, or why it was not ranked at all (no request term
reached it). The totals say how many truth files are in the first six (the
focused layout) and the first sixteen (the full layout). Free: no agent runs.

    python -B rank_check.py --campaign <dir> --calls <calls.json> --maps <dir of *.map.json> [--tasks <id> ...]
"""

from __future__ import annotations

import argparse
import json
import pathlib
import sys

import agent_cost_batch
import live_gather

FOCUS = 6
FULL = live_gather.TOP_FILES


def truth_files(task: dict) -> dict[str, bool]:
    """Each located truth file, and whether any of its notes is essential."""
    files: dict[str, bool] = {}
    for note in task.get("notes") or []:
        if note.get("path") and not note.get("absence"):
            files[note["path"]] = files.get(note["path"], False) or note.get("mark") == "essential"
    return files


def check(args) -> int:
    recorded = json.loads(args.calls.read_text(encoding="utf-8"))["calls"]
    rows = {row["id"]: row for row in agent_cost_batch.working_set(args.campaign)}
    repositories: dict[str, live_gather.Repository] = {}
    total = {"files": 0, "focus": 0, "full": 0, "later": 0, "unranked": 0}
    for task_id in sorted(recorded):
        if task_id not in rows or (args.tasks and task_id not in args.tasks):
            continue
        row = rows[task_id]
        folder = row["snapshot"].name
        if folder not in repositories:
            index = json.loads((args.maps / f"{folder}.map.json").read_text(encoding="utf-8"))
            repositories[folder] = live_gather.Repository(row["snapshot"], index)
        repository = repositories[folder]
        trace: dict = {}
        live_gather.gather(repository, recorded[task_id], trace)
        position = {path: rank for rank, (path, _) in enumerate(trace["ranked"], start=1)}
        task = json.loads(row["task"].read_text(encoding="utf-8"))
        print(f"## {task_id}: {len(trace['ranked'])} files ranked; request terms: {', '.join(trace['terms'][:12])}")
        for path, essential in sorted(truth_files(task).items()):
            total["files"] += 1
            rank = position.get(path)
            role = repository.index["files"].get(path, {}).get("role", "?")
            if rank is None:
                total["unranked"] += 1
                where = "unranked: no request term reached it"
            else:
                total["focus" if rank <= FOCUS else "full" if rank <= FULL else "later"] += 1
                where = f"rank {rank}: " + ", ".join(trace["held"][path][:6])
            print(f"   {'E' if essential else ' '} {path} [{role}] {where}")
        for rank, (path, score) in enumerate(trace["ranked"][:FOCUS], start=1):
            print(f"      top {rank}: {path} {score}")
    print(json.dumps(total))
    return 0


def main(argv: list[str] | None = None) -> int:
    parser = argparse.ArgumentParser(description=__doc__.splitlines()[0])
    parser.add_argument("--campaign", required=True, type=pathlib.Path)
    parser.add_argument("--calls", required=True, type=pathlib.Path)
    parser.add_argument("--maps", required=True, type=pathlib.Path)
    parser.add_argument("--tasks", nargs="*")
    args = parser.parse_args(argv)
    sys.stdout.reconfigure(encoding="utf-8", errors="replace")
    return check(args)


if __name__ == "__main__":
    sys.exit(main())
