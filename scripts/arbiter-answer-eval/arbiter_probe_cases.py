# SPDX-License-Identifier: Apache-2.0
"""Build evaluator-side pair groups for ``arbiter_pair_probe.py``.

Evaluator-side and read-only. From the private scenarios, the pinned source
exports and delivered Gather Context replies it builds three kinds of group
for each scenario. Reference notes locate the right candidates. They are
evaluator knowledge and never reach a product prompt.

* ``passage``: the task against whole passages. Right candidates are the
  essential notes' own lines from the export. Wrong candidates are passages
  the settled reply delivered that overlap no note location.
* ``action``: the task against short descriptions of a read: a path, a line
  range and the first lines found there. Right and wrong as for ``passage``.
* ``coverage``: one essential fact against evidence. The right candidate is
  the note's whole range. Wrong candidates are the first part of that range
  and another essential note's range, which is relevant and does not
  establish this fact.

    python -B arbiter_probe_cases.py --scenarios <dir> --sources <repository>=<export dir> ... \
        --replies <dir> --output <new file> [--cases <id> ...]
"""

from __future__ import annotations

import argparse
import json
import pathlib
import sys

import evidence
import gather_context_loss_stage as loss

PASSAGE = "Given a software development task, judge whether the Document holds source evidence needed to carry out the task."
ACTION = "Given a software development task, judge whether reading the source location described in the Document would help carry out the task."
COVERAGE = "Judge whether the Document by itself fully establishes the statement in the Query."
PUBLISHER = "Given a web search query, retrieve relevant passages that answer the query"
ROSTER = "Choose the one source location whose reading would most help carry out the task in the question."
MAX_CHARS = 6000
MAX_WRONG = 16
HEAD_LINES = 3
ROSTER_OPTIONS = 8


def clauses(task: str) -> list[str]:
    """The task cut at sentence ends, colons, semicolons and ", and". Pieces under four words join the piece
    before them. This is a fixed rule a kernel could apply; it uses nothing from the reference notes."""
    pieces, current = [], ""
    text = task.replace(", and ", "\x00").replace("; ", "\x00").replace(": ", "\x00")
    for mark in (". ", "? ", "! "):
        text = text.replace(mark, mark.strip() + "\x00")
    for piece in text.split("\x00"):
        piece = piece.strip()
        if not piece:
            continue
        if pieces and len(piece.split()) < 4:
            pieces[-1] = pieces[-1] + " " + piece
        else:
            pieces.append(piece)
    return pieces or [task]


def read_lines(root: pathlib.Path, path: str, start: int, end: int) -> str | None:
    file = root / path
    if not file.is_file():
        return None
    lines = file.read_text(encoding="utf-8", errors="replace").splitlines()
    if start < 1 or start > len(lines):
        return None
    return "\n".join(lines[start - 1:end])[:MAX_CHARS]


def describe(path: str, start: int, end: int, text: str, head_lines: int = HEAD_LINES) -> str:
    head = "\n".join(text.splitlines()[:head_lines])
    return f"{path} lines {start}-{end}\n{head}"


def _overlaps_a_note(span: evidence.Span, notes: list[dict]) -> bool:
    for note in notes:
        for location in loss._locations(note):
            if evidence.path_matches(span.path, location["path"]) and (
                    location["lines"] is None or span.start is None
                    or evidence.overlap_lines(span, location["lines"]) > 0):
                return True
    return False


def build_case(case_id: str, scenario: dict, reply: dict, source: pathlib.Path, query_mode: str = "whole",
               head_lines: int = HEAD_LINES, passage_instruction: str = PASSAGE, roster: bool = False,
               caller_needs: list[str] | None = None) -> list[dict]:
    notes = [note for note in scenario["notes"] if note.get("mark") == "essential" and note.get("lines")]
    right = []
    for note in notes:
        text = read_lines(source, note["path"], note["lines"][0], note["lines"][1])
        if text:
            right.append((note, text))
    wrong, seen = [], set()
    for _, span in loss._record_spans(loss._reply_body(reply)):
        key = (span.path, span.start, span.end)
        if (span.kind != "text" or span.start is None or len(span.text) < 80 or key in seen
                or _overlaps_a_note(span, scenario["notes"])):
            continue
        seen.add(key)
        wrong.append(span)
        if len(wrong) == MAX_WRONG:
            break
    if not right or not wrong:
        return []
    task = scenario["instruction"]
    if query_mode == "needs":
        # What a calling agent wrote from the task alone, before reading the repository.
        if not caller_needs:
            raise ValueError(f"{case_id} has no caller needs")
        asked = {"queries": list(caller_needs)}
    elif query_mode == "clauses":
        asked = {"queries": clauses(task)}
    elif query_mode == "fact":
        # An upper bound: the evaluator's own statement of each need, which no product prompt may use.
        asked = {"queries": [note["fact"] for note, _ in right]}
    else:
        asked = {"query": task}
    if roster:
        position = sum(case_id.encode()) % ROSTER_OPTIONS
        note, text = right[0]
        options = [{"id": f"w{index}", "label": 0,
                    "text": describe(span.path, span.start, span.end, span.text, head_lines)}
                   for index, span in enumerate(wrong[:ROSTER_OPTIONS - 1])]
        options.insert(min(position, len(options)),
                       {"id": note["id"], "label": 1,
                        "text": describe(note["path"], note["lines"][0], note["lines"][1], text, head_lines)})
        pair_form = {"id": f"{case_id}/roster-as-pairs", "use": "roster_as_pairs", "instruction": ACTION,
                     "query": task, "candidates": [dict(option) for option in options]}
        return [{"id": f"{case_id}/roster", "use": "roster", "form": "roster", "instruction": ROSTER,
                 "query": task, "candidates": options}, pair_form]
    passage = {"id": f"{case_id}/passage", "use": "passage", "instruction": passage_instruction, **asked,
               "candidates": [{"id": note["id"], "text": text, "label": 1} for note, text in right] +
                             [{"id": f"w{index}", "text": span.text[:MAX_CHARS], "label": 0}
                              for index, span in enumerate(wrong)]}
    action = {"id": f"{case_id}/action", "use": "action", "instruction": ACTION, **asked,
              "candidates": [{"id": note["id"], "label": 1,
                              "text": describe(note["path"], note["lines"][0], note["lines"][1], text, head_lines)}
                             for note, text in right] +
                            [{"id": f"w{index}", "label": 0,
                              "text": describe(span.path, span.start, span.end, span.text, head_lines)}
                             for index, span in enumerate(wrong)]}
    groups = [passage, action]
    for position, (note, text) in enumerate(right):
        lines = text.splitlines()
        candidates = [{"id": "whole", "text": text, "label": 1}]
        if len(lines) >= 6:
            candidates.append({"id": "first-part", "text": "\n".join(lines[:max(2, len(lines) * 2 // 5)]), "label": 0})
        if len(right) > 1:
            other = right[(position + 1) % len(right)][1]
            candidates.append({"id": "other-note", "text": other, "label": 0})
        if len(candidates) >= 2:
            groups.append({"id": f"{case_id}/coverage/{note['id']}", "use": "coverage", "instruction": COVERAGE,
                           "query": note["fact"], "candidates": candidates})
    return groups


def main(argv: list[str] | None = None) -> int:
    parser = argparse.ArgumentParser(description=__doc__.splitlines()[0])
    parser.add_argument("--scenarios", required=True, type=pathlib.Path)
    parser.add_argument("--sources", required=True, nargs="+", help="<repository>=<export directory>")
    parser.add_argument("--replies", required=True, type=pathlib.Path)
    parser.add_argument("--output", required=True, type=pathlib.Path)
    parser.add_argument("--cases", nargs="*")
    parser.add_argument("--query-mode", choices=("whole", "clauses", "fact", "needs"), default="whole",
                        help="whole task, the task's clauses, the evaluator's facts as an upper bound, or "
                             "needs a calling agent wrote from the task alone")
    parser.add_argument("--needs", type=pathlib.Path,
                        help='for --query-mode needs: {"cases": {"<id>": {"needs": [...]}}}')
    parser.add_argument("--head-lines", type=int, default=HEAD_LINES)
    parser.add_argument("--publisher-instruction", action="store_true",
                        help="use the publisher's default instruction for passage groups")
    parser.add_argument("--roster", action="store_true",
                        help="build one-pass roster groups, and the same options as pairs, in place of the others")
    args = parser.parse_args(argv)
    sources = {}
    for item in args.sources:
        repository, _, folder = item.partition("=")
        if not repository or not pathlib.Path(folder).is_dir():
            raise ValueError(f"invalid --sources entry {item}")
        sources[repository] = pathlib.Path(folder)
    if (args.query_mode == "needs") != (args.needs is not None):
        raise ValueError("--needs goes with --query-mode needs")
    written = json.loads(args.needs.read_text(encoding="utf-8"))["cases"] if args.needs else {}
    groups = []
    for scenario_path in sorted(args.scenarios.glob("*/*.json")):
        repository = scenario_path.parent.name
        case_id = f"{repository}-{scenario_path.stem}"
        reply_path = args.replies / f"{case_id}.reply.json"
        if repository not in sources or not reply_path.is_file() or (args.cases and case_id not in args.cases):
            continue
        groups.extend(build_case(case_id, json.loads(scenario_path.read_text(encoding="utf-8")),
                                 json.loads(reply_path.read_text(encoding="utf-8")), sources[repository],
                                 args.query_mode, args.head_lines,
                                 PUBLISHER if args.publisher_instruction else PASSAGE, args.roster,
                                 (written.get(case_id) or {}).get("needs")))
    if not groups:
        raise ValueError("no group was built")
    with args.output.open("x", encoding="utf-8", newline="\n") as handle:
        handle.write(json.dumps({"groups": groups}, indent=2) + "\n")
    counts: dict[str, int] = {}
    for group in groups:
        counts[group["use"]] = counts.get(group["use"], 0) + 1
    print(json.dumps({"groups": len(groups), "by_use": counts,
                      "pairs": sum(len(group["candidates"]) for group in groups)}))
    return 0


if __name__ == "__main__":
    sys.exit(main())
