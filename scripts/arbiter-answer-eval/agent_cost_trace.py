# SPDX-License-Identifier: Apache-2.0
"""Print what an acting agent did in one cost session, round trip by round trip.

Evaluation tooling for the one-task-at-a-time experiments of
``docs/arbiter/topical-index-research.md``. Cost is new content entering
the context, so the useful view of a session is: in each round trip, which
tools the agent called, on what, and how many bytes came back. It reads a
session folder's ``stream.jsonl`` and judges nothing.

    python -B agent_cost_trace.py <session folder> [<session folder> ...] [--summary]
"""

from __future__ import annotations

import argparse
import json
import pathlib
import sys


def trace(folder: pathlib.Path) -> list[dict]:
    """One row a round trip: the tool calls it made and the bytes each returned."""
    rounds, by_message, pending = [], {}, {}
    for line in (folder / "stream.jsonl").read_text(encoding="utf-8", errors="replace").splitlines():
        try:
            event = json.loads(line)
        except ValueError:
            continue
        message = event.get("message") or {}
        if event.get("type") == "assistant":
            row = by_message.get(message.get("id"))
            if row is None:
                row = by_message[message.get("id")] = {"calls": [], "text": 0}
                rounds.append(row)
            for block in message.get("content") or []:
                if block.get("type") == "tool_use":
                    arguments = block.get("input") or {}
                    target = (arguments.get("file_path") or arguments.get("pattern") or arguments.get("path")
                              or arguments.get("task") or "")
                    extra = ""
                    if "offset" in arguments or "limit" in arguments:
                        extra = f"@{arguments.get('offset', 1)}+{arguments.get('limit', '')}"
                    call = {"tool": block.get("name", "").replace("mcp__airk__", ""), "target": str(target)[-70:] + extra, "bytes": None}
                    row["calls"].append(call)
                    pending[block.get("id")] = call
                elif block.get("type") == "text":
                    row["text"] += len(block.get("text") or "")
        elif event.get("type") == "user":
            for block in message.get("content") or []:
                if isinstance(block, dict) and block.get("type") == "tool_result" and block.get("tool_use_id") in pending:
                    content = block.get("content")
                    text = content if isinstance(content, str) else "".join(
                        part.get("text", "") for part in content or [] if isinstance(part, dict))
                    pending[block["tool_use_id"]]["bytes"] = len(text.encode("utf-8"))
    return rounds


def main(argv: list[str] | None = None) -> int:
    parser = argparse.ArgumentParser(description=__doc__.splitlines()[0])
    parser.add_argument("sessions", nargs="+", type=pathlib.Path)
    parser.add_argument("--summary", action="store_true", help="one line a session")
    args = parser.parse_args(argv)
    sys.stdout.reconfigure(encoding="utf-8", errors="replace")
    for folder in args.sessions:
        rounds = trace(folder)
        totals: dict[str, list[int]] = {}
        for row in rounds:
            for call in row["calls"]:
                entry = totals.setdefault(call["tool"], [0, 0])
                entry[0] += 1
                entry[1] += call["bytes"] or 0
        record = json.loads((folder / "session.json").read_text(encoding="utf-8")) if (folder / "session.json").is_file() else {}
        print(f"== {folder.name}: {len(rounds)} round trips, cost {record.get('cost_usd')}, "
              + ", ".join(f"{tool} {count}x {size}b" for tool, (count, size) in sorted(totals.items())))
        if args.summary:
            continue
        for number, row in enumerate(rounds, 1):
            calls = "; ".join(f"{call['tool']}({call['target']})={call['bytes']}" for call in row["calls"])
            print(f"  {number:>2}. " + (calls or f"text {row['text']} chars"))
    return 0


if __name__ == "__main__":
    sys.exit(main())
