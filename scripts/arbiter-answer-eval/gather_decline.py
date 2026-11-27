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

# The refusal an agent receives (user wording, 27 November 2026), without a file list: served on
# bookbuilder-N-C4 the list named the files defining the task's setting flags, not the files it was
# about, and the user dropped it (``decline-sonnet-r1``).
DECLINE_TEXT = "This request is specific enough that gather_context is unlikely to be a good fit. Read files directly instead."
# Rule 1 reads the task text only: agents write such words in their needs on narrow tasks too.
WIDE = re.compile(r"\b(every|everywhere|all (?:the )?places|each place|wherever|anywhere|across|throughout|trace|flow|"
                  r"end[- ]to[- ]end|what happens when|passes through|all (?:callers|call sites|usages?|uses))\b", re.IGNORECASE)
NARROW_FILES = 3
# A language the task names narrows where its symbols count (the same API in several languages is
# common). "Go" counts only capitalized, since "go" is an ordinary word.
LANGUAGES = [(re.compile(r"\bpython\b", re.IGNORECASE), (".py",)), (re.compile(r"\brust\b", re.IGNORECASE), (".rs",)),
             (re.compile(r"\bjava\b", re.IGNORECASE), (".java",)), (re.compile(r"\b(?:Go|golang)\b"), (".go",)),
             (re.compile(r"\btypescript\b", re.IGNORECASE), (".ts", ".tsx")),
             (re.compile(r"\bjavascript\b", re.IGNORECASE), (".js", ".mjs", ".cjs", ".jsx")),
             (re.compile(r"(?:\bc#|\bcsharp\b|\.net\b)", re.IGNORECASE), (".cs",)),
             (re.compile(r"\bkotlin\b", re.IGNORECASE), (".kt", ".kts")), (re.compile(r"\bswift\b", re.IGNORECASE), (".swift",)),
             (re.compile(r"\bruby\b", re.IGNORECASE), (".rb",)), (re.compile(r"\bphp\b", re.IGNORECASE), (".php",)),
             (re.compile(r"(?:\bc\+\+|\bcpp\b)", re.IGNORECASE), (".cpp", ".cc", ".hpp", ".h"))]

def features(repository: live_gather.Repository, arguments: dict) -> dict:
    """What the tool can see about a request at call time (rubric approved 27 November 2026)."""
    index = repository.index
    task = str(arguments.get("task") or "")
    defined_in: dict[str, set[str]] = {}
    for path, record in index["files"].items():
        for name in record.get("defines", {}):
            defined_in.setdefault(name, set()).add(path)
    suffixes = tuple(suffix for pattern, kinds in LANGUAGES if pattern.search(task) for suffix in kinds)
    if suffixes:
        scoped = {name: {path for path in paths if path.endswith(suffixes)} for name, paths in defined_in.items()}
        defined_in = {name: paths for name, paths in scoped.items() if paths}
    named = []
    for token in live_gather.IDENTIFIER.findall(task) + [str(anchor) for anchor in arguments.get("anchors") or []]:
        name = token.split(".")[-1].split(":")[-1]
        # A qualified name (``reports.timeseries``) is code by its shape, whatever its last part looks like.
        qualified = name != token and bool(name)
        if (qualified or live_gather.is_name(name)) and name in defined_in and name not in named:
            named.append(name)
    # Rule 3: count only files that define the named symbols. Rule 4: a name defined widely (a
    # common example name) is ignored when the task also names a narrowly defined one.
    narrow = [name for name in named if len(defined_in[name]) <= NARROW_FILES]
    counted = narrow if narrow else named
    files = set().union(*(defined_in[name] for name in counted)) if counted else set()
    paths = []
    for mention in live_gather.PATH_MENTION.findall(task):
        mention = mention.replace("\\", "/").lstrip("./")
        hits = [path for path in repository.texts if "/" in mention and (path == mention or path.endswith("/" + mention))]
        if hits:
            paths.append(mention)
            files.update(hits)
    return {"names_in_task": len(named), "narrow_names": len(narrow), "paths_in_task": len(paths), "languages": list(suffixes),
            "files": sorted(files), "wide_words": sorted({match.group(0).lower() for match in WIDE.finditer(task)}),
            "needs": len(arguments.get("needs") or []), "deliverable": arguments.get("deliverable")}


def decide(seen: dict) -> tuple[bool, str]:
    """Decline only when the task asks for nothing wide, names its place, and that place is small."""
    if seen["wide_words"]:
        return False, "asks for something wide: " + ", ".join(seen["wide_words"])
    if not seen["names_in_task"] and not seen["paths_in_task"]:
        return False, "names no defined symbol or path"
    if len(seen["files"]) > NARROW_FILES:
        return False, f"its names are defined in {len(seen['files'])} files"
    return True, f"names its place, defined in {len(seen['files'])} file(s)"


def refusal(seen: dict, with_files: bool = False) -> str:
    """The refusal text. ``with_files`` (kept for reproducing ``decline-sonnet-r1``) appends the files
    the rule found, unless the task named only paths."""
    if with_files and seen["files"] and seen["names_in_task"]:
        return f"{DECLINE_TEXT[:-1]}: {', '.join(seen['files'])}."
    return DECLINE_TEXT

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
        seen = features(repositories[folder], arguments)
        decline, why = decide(seen)
        declined += decline
        known = " ".join(f"{condition.split(':')[0]}={value:+.0f}%" for condition, value in sorted(effects.get(task_id, {}).items()))
        print(f"{task_id:<20} {'DECLINE' if decline else 'answer ':<8} names {seen['names_in_task']}/{seen['narrow_names']} "
              f"files {len(seen['files'])} paths {seen['paths_in_task']} | {why[:60]:<60} | {known}")
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
