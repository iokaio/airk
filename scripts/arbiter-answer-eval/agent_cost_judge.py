# SPDX-License-Identifier: Apache-2.0
"""Lay out one series' sessions against each task's frozen truth for judging.

Evaluation tooling for the Gather Context strategy of 27 November 2026. It
does not judge. A reviewer judges each session directly; this prints what
the reviewer needs side by side and keeps the verdicts.

``sheet`` prints, per session, the task's essential notes (path, lines and
fact), whether the session's answer or patch names each note's file, the
files the patch changed and the answer text. The file-name screen is a
reading aid only: naming a file is not being right about it.

``verdict`` stores the reviewer's verdict for sessions in
``<series>/verdicts.json``: ``correct``, ``partial`` or ``wrong``, with a
reason. ``summary`` counts verdicts per arm.

    python -B agent_cost_judge.py sheet --campaign <dir> --series <name> [--tasks <id> ...] [--answer-chars 3000]
    python -B agent_cost_judge.py verdict --campaign <dir> --series <name> --session <folder name> --is correct --reason "<text>"
    python -B agent_cost_judge.py summary --campaign <dir> --series <name>
"""

from __future__ import annotations

import argparse
import json
import pathlib
import re
import sys

import agent_cost_batch

VERDICTS = ("correct", "partial", "wrong")
SCHEMA = "airk.private.agent-cost-verdicts/1"


def changed_files(patch: str) -> list[str]:
    return sorted(set(re.findall(r"^\+\+\+ b/(.+)$", patch, flags=re.MULTILINE)))


def sheet_for(session: pathlib.Path, task: dict, answer_chars: int) -> str:
    answer = (session / "answer.md").read_text(encoding="utf-8", errors="replace") if (session / "answer.md").exists() else ""
    patch = (session / "changes.patch").read_text(encoding="utf-8", errors="replace") if (session / "changes.patch").exists() else ""
    record = json.loads((session / "session.json").read_text(encoding="utf-8"))
    lines = [f"=== {session.name}  completed={record.get('completed')} cost=${record.get('cost_usd')} "
             f"trips={record.get('round_trips')}", f"TASK: {task['instruction']}"]
    if task.get("expected_result"):
        lines.append(f"EXPECTED: {task['expected_result']}")
    for note in task.get("notes", []):
        if note.get("mark") != "essential":
            continue
        if note.get("absence"):
            where, named = "absence in " + ", ".join(note.get("scope", {}).get("paths", [])), "-"
        else:
            name = pathlib.PurePosixPath(note["path"]).name
            where = f"{note['path']}:{note['lines'][0]}-{note['lines'][1]}"
            named = ("A" if name in answer else ".") + ("P" if note["path"] in patch else ".")
        lines.append(f"  {note['id']} [{named}] {where} :: {note['fact']}")
    lines.append("PATCH FILES: " + (", ".join(changed_files(patch)) or "none"))
    lines.append("ANSWER:\n" + answer[:answer_chars] + ("\n[cut]" if len(answer) > answer_chars else ""))
    return "\n".join(lines)


def tasks_by_id(campaign: pathlib.Path) -> dict[str, dict]:
    return {row["id"]: json.loads(row["task"].read_text(encoding="utf-8")) for row in agent_cost_batch.working_set(campaign)}


def main(argv: list[str] | None = None) -> int:
    parser = argparse.ArgumentParser(description=__doc__.splitlines()[0])
    parser.add_argument("command", choices=("sheet", "verdict", "summary"))
    parser.add_argument("--campaign", required=True, type=pathlib.Path)
    parser.add_argument("--series", required=True)
    parser.add_argument("--tasks", nargs="*")
    parser.add_argument("--answer-chars", type=int, default=3000)
    parser.add_argument("--session")
    parser.add_argument("--is", dest="verdict", choices=VERDICTS)
    parser.add_argument("--reason", default="")
    args = parser.parse_args(argv)
    series = args.campaign / "agent-cost" / args.series
    store = series / "verdicts.json"
    verdicts = json.loads(store.read_text(encoding="utf-8"))["sessions"] if store.exists() else {}
    if args.command == "sheet":
        tasks = tasks_by_id(args.campaign)
        for session in sorted(path.parent for path in series.glob("*/session.json")):
            task_id = session.name.rsplit(".", 2)[0]
            if args.tasks and task_id not in args.tasks:
                continue
            print(sheet_for(session, tasks[task_id], args.answer_chars))
            if session.name in verdicts:
                print(f"VERDICT: {verdicts[session.name]['verdict']} — {verdicts[session.name]['reason']}")
            print()
        return 0
    if args.command == "verdict":
        if not args.session or not args.verdict or not (series / args.session / "session.json").exists():
            parser.error("verdict needs --session naming a finished session and --is")
        verdicts[args.session] = {"verdict": args.verdict, "reason": args.reason}
        store.write_text(json.dumps({"schema": SCHEMA, "series": args.series, "sessions": verdicts}, indent=2, sort_keys=True) + "\n",
                         encoding="utf-8", newline="\n")
        return 0
    counts: dict[str, dict[str, int]] = {}
    for name, entry in verdicts.items():
        arm = name.rsplit(".", 2)[1]
        counts.setdefault(arm, {key: 0 for key in VERDICTS})[entry["verdict"]] += 1
    finished = len(list(series.glob("*/session.json")))
    print(json.dumps({"judged": len(verdicts), "finished": finished, "by_arm": counts}, sort_keys=True))
    return 0


if __name__ == "__main__":
    sys.exit(main())
