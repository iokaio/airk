# SPDX-License-Identifier: Apache-2.0
"""Check and freeze a repository's narrow task set.

Evaluation tooling for the Gather Context strategy of 27 November 2026. A
narrow set is seven private task files, ``N-C1`` to ``N-C4``, ``N-A1``,
``N-A2`` and ``N-W1``, each with an instruction and the truth a correct
result needs: notes with a path and line range in the indexed snapshot, or
a searched absence.

``check`` verifies the set against the snapshot it is posed on: the seven
slots and their kinds, exactly one every-place coding task, every cited file
present and every line range inside it, and at least three essential notes
a task. It prints, without failing, any instruction that names a file its
own truth cites, for a reviewer to weigh. ``freeze``
runs the same check and then writes ``freeze.json`` with the SHA-256 of
every task file, which is what a session records as the task's identity.
The truth is frozen before any tool or agent runs on a task.

    python -B narrow_scenarios.py check --tasks <dir> --snapshot <source dir>
    python -B narrow_scenarios.py freeze --tasks <dir> --snapshot <source dir>
"""

from __future__ import annotations

import argparse
import json
import pathlib
import sys

import common

SCHEMA = "airk.private.narrow-scenario/1"
FREEZE_SCHEMA = "airk.private.narrow-scenario-freeze/1"
SLOTS = {"N-C1": "coding", "N-C2": "coding", "N-C3": "coding", "N-C4": "coding",
         "N-A1": "architecture", "N-A2": "architecture", "N-W1": "writing"}
MIN_ESSENTIAL = 3


def problems(tasks: pathlib.Path, snapshot: pathlib.Path) -> list[str]:
    found: list[str] = []
    files = {path.stem: path for path in tasks.glob("N-*.json")}
    if set(files) != set(SLOTS):
        return [f"the set holds {sorted(files)}, not {sorted(SLOTS)}"]
    lengths: dict[str, int] = {}
    every = []
    for slot, path in sorted(files.items()):
        task = json.loads(path.read_text(encoding="utf-8"))
        if task.get("schema") != SCHEMA or task.get("slot") != slot or task.get("kind") != SLOTS[slot]:
            found.append(f"{slot}: schema, slot or kind differs")
        instruction = task.get("instruction") or ""
        if not instruction.strip() or len(instruction) > 1200:
            found.append(f"{slot}: the instruction is empty or too long")
        if task.get("every_place") is True:
            every.append(slot)
        notes = task.get("notes") or []
        if sum(note.get("mark") == "essential" for note in notes) < MIN_ESSENTIAL:
            found.append(f"{slot}: fewer than {MIN_ESSENTIAL} essential notes")
        if len({note.get("id") for note in notes}) != len(notes):
            found.append(f"{slot}: note ids repeat")
        for note in notes:
            name = f"{slot} {note.get('id')}"
            if note.get("mark") not in ("essential", "helpful") or not (note.get("fact") or "").strip():
                found.append(f"{name}: mark or fact missing")
            if note.get("absence") is True:
                scope = note.get("scope") or {}
                if not scope.get("paths") or not scope.get("patterns"):
                    found.append(f"{name}: an absence needs the paths and patterns searched")
                continue
            path_text, lines = note.get("path"), note.get("lines")
            if not isinstance(path_text, str) or "\\" in path_text or path_text.startswith("/"):
                found.append(f"{name}: path is not repository-relative with forward slashes")
                continue
            if path_text not in lengths:
                file = snapshot / path_text
                lengths[path_text] = len(file.read_bytes().splitlines()) if file.is_file() else -1
            if lengths[path_text] < 0:
                found.append(f"{name}: {path_text} is not in the snapshot")
            elif not (isinstance(lines, list) and len(lines) == 2 and all(isinstance(n, int) for n in lines)
                      and 1 <= lines[0] <= lines[1] <= lengths[path_text]):
                found.append(f"{name}: lines {lines} are outside {path_text} ({lengths[path_text]} lines)")
            if pathlib.PurePosixPath(path_text).name in instruction and note.get("mark") == "essential":
                # Not a fault by itself: a developer may name the very file a question is about. It is
                # printed so a reviewer decides whether the instruction gives away what should be found.
                print(f"note: {name}: the instruction names the file {pathlib.PurePosixPath(path_text).name}")
    coding_every = [slot for slot in every if SLOTS[slot] == "coding"]
    if len(coding_every) != 1 or len(every) != 1:
        found.append(f"exactly one coding task asks for every place, found {every}")
    return found


def main(argv: list[str] | None = None) -> int:
    parser = argparse.ArgumentParser(description=__doc__.splitlines()[0])
    parser.add_argument("command", choices=("check", "freeze"))
    parser.add_argument("--tasks", required=True, type=pathlib.Path)
    parser.add_argument("--snapshot", required=True, type=pathlib.Path)
    args = parser.parse_args(argv)
    found = problems(args.tasks, args.snapshot.resolve(strict=True))
    for line in found:
        print(line)
    if found:
        print(f"{len(found)} problem(s); nothing frozen")
        return 1
    rows = []
    for slot in sorted(SLOTS):
        task = json.loads((args.tasks / f"{slot}.json").read_text(encoding="utf-8"))
        notes = task["notes"]
        rows.append({"slot": slot, "kind": task["kind"], "every_place": bool(task.get("every_place")),
                     "essential": sum(note["mark"] == "essential" for note in notes),
                     "absences": sum(note.get("absence") is True for note in notes),
                     "sha256": common.sha256_file(args.tasks / f"{slot}.json")})
        print(f"{slot} {task['kind']:<12} essential={rows[-1]['essential']} absences={rows[-1]['absences']} "
              f"{task['instruction'][:90]}")
    if args.command == "freeze":
        freeze = {"schema": FREEZE_SCHEMA, "at": common.now_iso(), "snapshot": str(args.snapshot), "tasks": rows}
        with (args.tasks / "freeze.json").open("x", encoding="utf-8", newline="\n") as file:
            file.write(json.dumps(freeze, indent=2, sort_keys=True) + "\n")
        print("frozen")
    return 0


if __name__ == "__main__":
    sys.exit(main())
