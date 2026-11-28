# SPDX-License-Identifier: Apache-2.0
"""Report evidence received before the first edit in a skill evaluation session."""

import argparse
from collections import defaultdict
from itertools import groupby
import json
from pathlib import Path
import re

import common
import evidence
from gather_context_call_replay import span_of


def line_spans(path, numbers):
    for _, group in groupby(enumerate(sorted(set(numbers))), lambda pair: pair[1] - pair[0]):
        lines = [n for _, n in group]
        yield evidence.Span(path, lines[0], lines[-1], "", "text")


def grep_spans(value, scope):
    numbers = defaultdict(list)
    for line in value.splitlines():
        single = re.match(r"^(\d+)([:-])", line)
        if single:
            numbers[scope].append(int(single[1]))
            continue
        multi = re.match(r"^(.+?)([:-])(\d+)\2", line)
        if multi:
            numbers[multi[1]].append(int(multi[3]))
    for path, hits in numbers.items():
        if path:
            yield from line_spans(path, hits)


def measure(folder):
    session = json.loads((folder / "session.json").read_text(encoding="utf-8"))
    task = json.loads(Path(session["task"]["path"]).read_text(encoding="utf-8"))
    notes = [n for n in task["notes"] if n.get("mark") == "essential" and n.get("path") and not n.get("absence")]
    calls = {}
    reads_after_gather = 0
    received_gather = False
    editing = False
    passages, reads, searches, skills = [], [], [], []
    for event in common.read_jsonl(folder / "stream.jsonl"):
        for block in (event.get("message") or {}).get("content") or []:
            if not isinstance(block, dict):
                continue
            if block.get("type") == "tool_use":
                name = block["name"]
                calls[block["id"]] = (name, block["input"])
                if name in ("Edit", "Write"):
                    editing = True
                if name == "Read" and received_gather:
                    reads_after_gather += 1
            if block.get("type") != "tool_result" or block.get("is_error"):
                continue
            name, arguments = calls.get(block["tool_use_id"], ("", {}))
            value = block.get("content") or ""
            if isinstance(value, list):
                value = "\n".join(part.get("text", "") for part in value if isinstance(part, dict))
            if name == "Skill":
                skills.append(arguments["skill"])
            if name == "mcp__airk__gather_context":
                received_gather = True
                if not editing:
                    for item in json.loads(value).get("passages", []):
                        span = span_of(item["at"], item["text"])
                        if span:
                            passages.append(span)
            if name == "Read" and not editing:
                lines = [int(n) for n in re.findall(r"(?m)^(\d+)\t", value)]
                reads.extend(line_spans(arguments["file_path"], lines))
            if name == "Grep" and not editing and arguments.get("output_mode") == "content":
                searches.extend(grep_spans(value, arguments.get("path", "")))
    return {"session": folder.name, "skill_results": skills, "followup_reads": reads_after_gather,
            "gather_notes": evidence.covered_notes(passages, notes),
            "gather_and_read_notes": evidence.covered_notes(passages + reads, notes),
            "all_notes": evidence.covered_notes(passages + reads + searches, notes)}


if __name__ == "__main__":
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("series", type=Path)
    args = parser.parse_args()
    for path in sorted(args.series.glob("*/session.json")):
        print(json.dumps(measure(path.parent)))
