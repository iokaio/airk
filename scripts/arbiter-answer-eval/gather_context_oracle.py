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

Two switches vary the layout with the content held perfect, to find which
layout an agent spends least with before any retrieval is built for it
(plan, "Experiment protocol", ceiling first):

- ``layout="ranges"`` carries no source text: each location comes with the
  ``Read`` offset and limit that fetch exactly it;
- ``trust=True`` adds a ``use`` line saying what the reply's content is and
  what the agent need not do again.
"""

from __future__ import annotations

import pathlib

import gather_context_dose

WHY_BYTES = 80
LAYOUTS = ("passages", "ranges")
USE = {"passages": "The passages below are the exact current text of the listed line ranges, and together they are what this task "
                   "needs. Work from them. Do not read those ranges again and do not search for more; read a file only for "
                   "lines that are not shown here, with offset and limit.",
       "ranges": "The ranges below are what this task needs. Read each with the Read tool using the offset and limit given, "
                 "several in one turn. Do not read whole files and do not search for more unless a range turns out to lack "
                 "something."}


def lines_of(snapshot: pathlib.Path, note: dict) -> str:
    text = (snapshot / note["path"]).read_text(encoding="utf-8", errors="replace").splitlines()
    first, last = note["lines"][0], note["lines"][-1]
    return "\n".join(text[first - 1:last]) + "\n"


def brief(task: dict, snapshot: pathlib.Path, arguments: dict, layout: str = "passages", trust: bool = False) -> dict:
    """The oracle brief for ``task``; ``arguments`` is what the agent sent."""
    located = [note for note in task.get("notes") or [] if note.get("path") and not note.get("absence")]
    essential = [note for note in located if note.get("mark") == "essential"]
    where, passages, omitted, spent = [], [], [], 0
    for note in essential:
        at = f"{note['path']}:{note['lines'][0]}-{note['lines'][-1]}"
        text = lines_of(snapshot, note)
        first = next((line.strip() for line in text.splitlines() if line.strip()), "")
        where.append({"at": at, "why": first[:WHY_BYTES],
                      **({"read": {"file_path": note["path"], "offset": note["lines"][0],
                                   "limit": note["lines"][-1] - note["lines"][0] + 1}} if layout == "ranges" else {})})
        size = len(text.encode("utf-8"))
        if layout == "ranges":
            continue
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
    if trust:
        out = {"schema": out["schema"], "use": USE[layout], **{key: value for key, value in out.items() if key != "schema"}}
    if absent:
        out["not_found"] = absent
    if omitted:
        out["omitted"] = omitted[:gather_context_dose.OMITTED_LIMIT]
    out["next"] = {"ask": "Ask again with narrower needs for what is still open, the same anchors, and known ranges for what you already hold."}
    return out
