# SPDX-License-Identifier: Apache-2.0
"""Why the loop prototype missed an essential passage.

Evaluator-side and read-only. For every essential note of a case it reads
the prototype's ``<case>.trace.json`` and reports the furthest point the
note's lines reached:

* ``delivered``: a delivered passage overlaps them by the coverage rule;
* ``judged``: a fetched candidate overlapped them and was scored, with its
  best score, the need it scored best for and its rank for that need;
* ``file``: candidates came from the note's file, at other lines;
* ``never``: no candidate came from the note's file.

    python -B arbiter_loop_misses.py --scenarios <dir> --traces <dir>
"""

from __future__ import annotations

import argparse
import json
import pathlib
import sys

import evidence


def reach(note: dict, trace: dict) -> dict:
    places = [{"path": note["path"], "lines": note.get("lines")}, *(note.get("alternatives") or [])]

    def hit(path: str, start: int, end: int) -> bool:
        span = evidence.Span(evidence.norm_path(path), start, end, "", "text")
        return any(evidence.path_matches(span.path, place["path"]) and (
            place.get("lines") is None
            or evidence.overlap_lines(span, place["lines"]) >= evidence.required_overlap(place["lines"]))
            for place in places)

    if any(hit(item["path"], item["start"], item["end"]) for item in trace["delivered"]):
        return {"stage": "delivered"}
    best = None
    in_file = False
    for index, rows in enumerate(trace["judged"]):
        for rank, (path, start, end, score) in enumerate(rows, 1):
            in_file = in_file or any(evidence.path_matches(evidence.norm_path(path), place["path"])
                                     for place in places)
            if hit(path, start, end) and (best is None or score > best["score"]):
                best = {"stage": "judged", "score": score, "need": index, "rank": rank, "lines": [start, end]}
    return best or {"stage": "file" if in_file else "never"}


def main(argv: list[str] | None = None) -> int:
    parser = argparse.ArgumentParser(description=__doc__.splitlines()[0])
    parser.add_argument("--scenarios", required=True, type=pathlib.Path)
    parser.add_argument("--traces", required=True, type=pathlib.Path)
    args = parser.parse_args(argv)
    counts: dict[str, int] = {}
    for path in sorted(args.traces.glob("*.trace.json")):
        case_id = path.name.removesuffix(".trace.json")
        repository, slot = case_id.split("-", 1)
        scenario = json.loads((args.scenarios / repository / f"{slot}.json").read_text(encoding="utf-8"))
        trace = json.loads(path.read_text(encoding="utf-8"))
        for note in scenario["notes"]:
            if note.get("mark") != "essential":
                continue
            row = reach(note, trace)
            counts[row["stage"]] = counts.get(row["stage"], 0) + 1
            detail = (f" score={row['score']} need={row['need']} rank={row['rank']} lines={row['lines']}"
                      if row["stage"] == "judged" else "")
            print(f"{case_id} {note['id']} {row['stage']}{detail} :: {note['path']} {note.get('lines')}")
    print(" ".join(f"{stage}={count}" for stage, count in sorted(counts.items())))
    return 0


if __name__ == "__main__":
    sys.exit(main())
