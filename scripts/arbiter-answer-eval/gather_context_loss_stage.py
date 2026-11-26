# SPDX-License-Identifier: Apache-2.0
"""Where each essential passage was lost in a delivered Gather Context reply.

Evaluator-side and read-only. For every essential note of a scenario the
script reads the reply an agent received and reports the furthest stage the
note's location reached:

* ``opened``: a Read File or Context read returned text that overlaps the
  note's lines by the coverage rule's amount;
* ``listed``: another read (search, symbols, references, outline) cited those
  lines, and no source read opened them;
* ``file``: the note's file appears in the reply, at other lines;
* ``never``: the note's file does not appear in the reply.

``opened`` and ``listed`` use the harness's line-overlap rule
(``evidence.required_overlap``). The stages describe what the reply shows.
They do not prove why a read was or was not chosen.

    python -B gather_context_loss_stage.py --scenarios <dir> --replies <dir> [--output <file>]

``--scenarios`` holds ``<repository>/<slot>.json`` scenario files and
``--replies`` holds ``<repository>-<slot>.reply.json`` MCP results.
"""

from __future__ import annotations

import argparse
import json
import pathlib
import sys

import evidence

SCHEMA = "airk.private.gather-context-loss-stage/1"
STAGES = ("opened", "listed", "file", "never")
SOURCE_READS = frozenset({"read_file", "context"})


def _reply_body(reply: dict) -> dict:
    """The Answer body of an MCP result, or the body itself."""
    body = reply.get("structuredContent") if isinstance(reply, dict) else None
    if isinstance(body, dict):
        return body
    if not isinstance(reply, dict) or not isinstance(reply.get("records"), list):
        raise ValueError("reply has neither structuredContent nor records")
    return reply


def _record_spans(body: dict) -> list[tuple[str, evidence.Span]]:
    """Every cited location of every record, with the operation that returned it."""
    found = []
    for record in body.get("records") or []:
        operation = record.get("operation")
        if not isinstance(operation, str):
            raise ValueError("a record has no operation")
        for span in evidence.extract(record.get("canonical_result")):
            found.append((operation, span))
        arguments = record.get("arguments")
        if operation in SOURCE_READS and isinstance(arguments, dict) and isinstance(arguments.get("path"), str):
            # A source read that returned no cited span still shows its file was reached.
            found.append((operation, evidence.Span(evidence.norm_path(arguments["path"]), None, None, "", "path")))
    return found


def _locations(note: dict) -> list[dict]:
    rows = [{"path": note["path"], "lines": note.get("lines")}]
    for alternative in note.get("alternatives") or []:
        rows.append({"path": alternative["path"], "lines": alternative.get("lines")})
    return rows


def stage_of(note: dict, spans: list[tuple[str, evidence.Span]]) -> str:
    """The furthest stage any of the note's locations reached."""
    best = "never"
    for location in _locations(note):
        same = [(operation, span) for operation, span in spans
                if evidence.path_matches(span.path, location["path"])]
        if not same:
            continue
        if best == "never":
            best = "file"
        lines = location["lines"]
        for operation, span in same:
            if lines is None:
                reached = span.start is not None or operation in SOURCE_READS
            else:
                reached = evidence.overlap_lines(span, lines) >= evidence.required_overlap(lines)
            if not reached:
                continue
            if operation in SOURCE_READS:
                return "opened"
            best = "listed"
    return best


FOLLOW_UP_LINES = 300
"""Lines one follow-up read is taken to cover when counting the reads a case still needs."""


def follow_up_reads(notes: list[dict], stages: dict[str, str], lines: int = FOLLOW_UP_LINES) -> list[dict]:
    """The fewest bounded reads, at the notes' own locations, that would reach every essential note the
    reply did not show at its lines. It is a lower bound for the sufficiency rule's two follow-up calls: an
    agent that did not know these locations would need at least as many. A note without a line range needs
    one read of its file."""
    by_path: dict[str, list[tuple[int, int, str]]] = {}
    for note in notes:
        if note.get("mark") != "essential" or stages[note["id"]] in ("opened", "listed"):
            continue
        span = note.get("lines") or [1, 1]
        by_path.setdefault(evidence.norm_path(note["path"]), []).append((span[0], span[1], note["id"]))
    reads = []
    for path, rows in sorted(by_path.items()):
        current = None
        for start, end, note_id in sorted(rows):
            if current is not None and end - current["start_line"] < lines:
                current["end_line"] = max(current["end_line"], end)
                current["notes"].append(note_id)
            else:
                current = {"path": path, "start_line": start, "end_line": end, "notes": [note_id]}
                reads.append(current)
    return reads


def classify(scenario: dict, reply: dict) -> dict:
    """One case: the stage of each essential note and the counts by stage."""
    notes = scenario.get("notes")
    if not isinstance(notes, list):
        raise ValueError("scenario has no notes")
    spans = _record_spans(_reply_body(reply))
    rows = []
    for note in notes:
        if note.get("mark") != "essential":
            continue
        if not isinstance(note.get("id"), str) or not isinstance(note.get("path"), str):
            raise ValueError("an essential note has no id or path")
        rows.append({"id": note["id"], "stage": stage_of(note, spans)})
    counts = {stage: sum(row["stage"] == stage for row in rows) for stage in STAGES}
    reads = follow_up_reads(notes, {row["id"]: row["stage"] for row in rows})
    return {"essentials": len(rows), "counts": counts, "notes": rows, "follow_up_reads": reads}


def classify_all(scenarios: pathlib.Path, replies: pathlib.Path) -> dict:
    cases = []
    for scenario_path in sorted(scenarios.glob("*/*.json")):
        case_id = f"{scenario_path.parent.name}-{scenario_path.stem}"
        reply_path = replies / f"{case_id}.reply.json"
        if not reply_path.is_file():
            continue
        scenario = json.loads(scenario_path.read_text(encoding="utf-8"))
        reply = json.loads(reply_path.read_text(encoding="utf-8"))
        cases.append({"id": case_id, "repository": scenario_path.parent.name, **classify(scenario, reply)})
    if not cases:
        raise ValueError("no scenario has a matching reply")
    totals = {stage: sum(case["counts"][stage] for case in cases) for stage in STAGES}
    by_repository: dict[str, dict] = {}
    for case in cases:
        row = by_repository.setdefault(case["repository"], {stage: 0 for stage in STAGES})
        for stage in STAGES:
            row[stage] += case["counts"][stage]
    return {"schema": SCHEMA, "coverage_rule": evidence.COVERAGE_RULE, "cases": cases,
            "essentials": sum(case["essentials"] for case in cases), "totals": totals,
            "by_repository": by_repository}


def main(argv: list[str] | None = None) -> int:
    parser = argparse.ArgumentParser(description=__doc__.splitlines()[0])
    parser.add_argument("--scenarios", required=True, type=pathlib.Path)
    parser.add_argument("--replies", required=True, type=pathlib.Path)
    parser.add_argument("--output", type=pathlib.Path)
    args = parser.parse_args(argv)
    result = classify_all(args.scenarios, args.replies)
    text = json.dumps(result, indent=2, sort_keys=True) + "\n"
    if args.output is not None:
        with args.output.open("x", encoding="utf-8", newline="\n") as handle:
            handle.write(text)
    for case in result["cases"]:
        counts = case["counts"]
        print(f"{case['id']:<16} essentials={case['essentials']:>2} " +
              " ".join(f"{stage}={counts[stage]}" for stage in STAGES) +
              f" follow_up_reads_needed>={len(case['follow_up_reads'])}")
    print(f"total essentials={result['essentials']} " +
          " ".join(f"{stage}={result['totals'][stage]}" for stage in STAGES))
    for repository, counts in sorted(result["by_repository"].items()):
        print(f"{repository:<16} " + " ".join(f"{stage}={counts[stage]}" for stage in STAGES))
    return 0


if __name__ == "__main__":
    sys.exit(main())
