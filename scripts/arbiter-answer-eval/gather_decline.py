# SPDX-License-Identifier: Apache-2.0
"""Research prototype: when Gather Context should decline a request.

Evaluation tooling for ``docs/arbiter/topical-index-research.md``
("Record every case where cost goes up, and learn when to decline", user
direction of 27 November 2026). On short tasks that already say where the
work is, any reply costs the agent more than doing the task itself. The
tool can decline such a request so the agent proceeds unaided.

``features`` computes, from the call and the tool's own census alone, what
a rule may use; a task's baseline cost and truth are never read. ``decide``
is the rule under test. ``check`` applies it to recorded agent calls and
prints it beside what each task did in finished cost series, which is the
free check before any decline is served to an agent.

    python -B gather_decline.py check --campaign <dir> --calls <calls.json> --maps <dir of *.map.json> \
        [--series oracle-sonnet-r1:oracle live2-sonnet-r1:live]
"""

from __future__ import annotations

import argparse
import json
import pathlib
import re
import sys

import agent_cost_batch
import live_gather

DECLINE_TEXT = ("gather_context cannot process this request effectively: it already names where the work is and the search "
                "finds it in very few files. Open those files directly.")
WIDE = re.compile(r"\b(every|everywhere|all (?:the )?places|each place|wherever|anywhere|across|throughout|trace|flow|"
                  r"end[- ]to[- ]end|what happens when|passes through|all (?:call|usage)s?)\b", re.IGNORECASE)
NARROW_FILES = 3


def features(repository: live_gather.Repository, arguments: dict, brief: dict) -> dict:
    """What the tool can see about a request at call time."""
    terms, _ = live_gather.request_terms(arguments, repository.index)
    source = {term["term"]: term["source"] for term in terms}
    rows = {row["term"]: row for row in brief.get("census") or []}
    task = str(arguments.get("task") or "")
    request = " ".join([task] + [str(need) for need in arguments.get("needs") or []])
    given = [term for term, kind in source.items() if kind in ("named", "quoted")]
    in_task = [term for term in given if term in task]
    found = [term for term in in_task if rows.get(term, {}).get("occurrences")]
    files: set[str] = set()
    for term in found:
        for entry in rows[term].get("in") or []:
            files.add(entry.rstrip("…").rpartition(":")[0])
        if rows[term].get("more_files") or rows[term].get("note"):
            files.update(f"more:{term}:{index}" for index in range(NARROW_FILES + 1))
    paths = [mention for mention in live_gather.PATH_MENTION.findall(task) if "/" in mention or "\\" in mention]
    return {"names_in_task": len(in_task), "names_found": len(found), "files_holding_them": len(files),
            "widest_name_files": max((rows[term].get("files", 0) for term in found), default=0),
            "paths_in_task": len(paths), "wide_words": sorted({match.group(0).lower() for match in WIDE.finditer(request)}),
            "needs": len(arguments.get("needs") or []), "deliverable": arguments.get("deliverable"),
            "task_bytes": len(task.encode("utf-8"))}


def decide(seen: dict) -> tuple[bool, str]:
    """The rule under test: decline a request that names its place, is found in few files, and asks for nothing wide."""
    if seen["wide_words"]:
        return False, "asks for something wide: " + ", ".join(seen["wide_words"])
    if not seen["names_found"] and not seen["paths_in_task"]:
        return False, "names no symbol or path that the search finds"
    if seen["files_holding_them"] > NARROW_FILES:
        return False, f"its names are spread over {seen['files_holding_them']} files"
    return True, f"names its place and the search finds it in {seen['files_holding_them']} file(s)"


def check(args) -> int:
    recorded = json.loads(args.calls.read_text(encoding="utf-8"))["calls"]
    rows = {row["id"]: row for row in agent_cost_batch.working_set(args.campaign)}
    effects: dict[str, dict[str, float]] = {}
    for condition in args.series or []:
        series, _, arm = condition.partition(":")
        report = args.campaign / "agent-cost" / series / "report.json"
        if report.is_file():
            for row in json.loads(report.read_text(encoding="utf-8"))["tasks"]:
                if f"{arm}_cost_lower_pct" in row:
                    effects.setdefault(row["task"], {})[condition] = row[f"{arm}_cost_lower_pct"]
    repositories: dict[str, live_gather.Repository] = {}
    declined = 0
    for task_id in sorted(recorded):
        if task_id not in rows:
            continue
        row = rows[task_id]
        folder = row["snapshot"].name
        if folder not in repositories:
            repositories[folder] = live_gather.Repository(row["snapshot"], json.loads((args.maps / f"{folder}.map.json").read_text(encoding="utf-8")))
        arguments = recorded[task_id]
        seen = features(repositories[folder], arguments, live_gather.gather(repositories[folder], arguments))
        decline, why = decide(seen)
        declined += decline
        known = " ".join(f"{condition.split(':')[0]}={value:+.0f}%" for condition, value in sorted(effects.get(task_id, {}).items()))
        print(f"{task_id:<20} {'DECLINE' if decline else 'answer ':<8} names {seen['names_in_task']}/{seen['names_found']} "
              f"files {seen['files_holding_them']} paths {seen['paths_in_task']} needs {seen['needs']} | {why[:70]:<70} | {known}")
    print(json.dumps({"calls": len(recorded), "declined": declined}))
    return 0


def main(argv: list[str] | None = None) -> int:
    parser = argparse.ArgumentParser(description=__doc__.splitlines()[0])
    parser.add_argument("command", choices=("check",))
    parser.add_argument("--campaign", required=True, type=pathlib.Path)
    parser.add_argument("--calls", required=True, type=pathlib.Path)
    parser.add_argument("--maps", required=True, type=pathlib.Path)
    parser.add_argument("--series", nargs="*", help="<series>:<arm> whose report.json gives each task's cost effect, for reading beside the rule")
    args = parser.parse_args(argv)
    sys.stdout.reconfigure(encoding="utf-8", errors="replace")
    return check(args)


if __name__ == "__main__":
    sys.exit(main())
