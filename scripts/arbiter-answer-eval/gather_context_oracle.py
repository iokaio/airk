# SPDX-License-Identifier: Apache-2.0
"""Oracle reply: the brief a perfect retrieval would return for a task.

Evaluation tooling for the Gather Context strategy of 27 November 2026. It
answers one question before more retrieval work is done: if the tool
returned exactly the passages a task needs, how much less would an acting
agent spend? That is the ceiling any real reply is measured against. If the
ceiling is low, better retrieval cannot make the tool pay on such tasks.

The oracle is built from the task's frozen truth and the repository
snapshot, never from the service. It gives only what retrieval could give:
locations and their source text, and the searched absences. It leaves out
each note's ``fact``, which is analysis an agent must still do.

Shape: the service's brief (``airk.gather-context.brief/1``), so the agent
reads the same layout in both arms. One need, the task itself; ``where``
lists every essential location; ``passages`` carries their text within the
brief's own allowance, in the truth's order; the rest are ``omitted``;
helpful locations are listed under ``omitted`` too.
"""

from __future__ import annotations

import pathlib

import gather_context_dose

WHY_BYTES = 80


def lines_of(snapshot: pathlib.Path, note: dict) -> str:
    text = (snapshot / note["path"]).read_text(encoding="utf-8", errors="replace").splitlines()
    first, last = note["lines"][0], note["lines"][-1]
    return "\n".join(text[first - 1:last]) + "\n"


def brief(task: dict, snapshot: pathlib.Path, arguments: dict) -> dict:
    """The oracle brief for ``task``; ``arguments`` is what the agent sent."""
    located = [note for note in task.get("notes") or [] if note.get("path") and not note.get("absence")]
    essential = [note for note in located if note.get("mark") == "essential"]
    where, passages, omitted, spent = [], [], [], 0
    for note in essential:
        at = f"{note['path']}:{note['lines'][0]}-{note['lines'][-1]}"
        text = lines_of(snapshot, note)
        first = next((line.strip() for line in text.splitlines() if line.strip()), "")
        where.append({"at": at, "why": first[:WHY_BYTES]})
        size = len(text.encode("utf-8"))
        if spent + size <= gather_context_dose.PASSAGE_BYTES:
            passages.append({"at": at, "needs": [1], "text": text})
            spent += size
        else:
            omitted.append(at)
    omitted += [f"{note['path']}:{note['lines'][0]}-{note['lines'][-1]}" for note in located if note.get("mark") != "essential"]
    absent = []
    for note in task.get("notes") or []:
        if note.get("absence"):
            scope = note.get("scope") or {}
            absent.append("nothing found for " + ", ".join(f'"{pattern}"' for pattern in scope.get("patterns") or [])
                          + "; searched: " + ", ".join(scope.get("paths") or []))
    out = {"schema": gather_context_dose.BRIEF_SCHEMA, "task": str(arguments.get("task") or task.get("instruction") or "")[:512],
           "result": "complete", "stop": "covered", "full": "response_mode=full returns the complete reply.",
           "needs": [{"need": 1, "text": "everything this task needs", "origin": "caller", "status": "covered", "where": where}],
           "passages": passages}
    if absent:
        out["not_found"] = absent
    if omitted:
        out["omitted"] = omitted[:gather_context_dose.OMITTED_LIMIT]
    out["next"] = {"ask": "Ask again with narrower needs for what is still open, the same anchors, and known ranges for what you already hold."}
    return out
