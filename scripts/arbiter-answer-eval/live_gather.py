# SPDX-License-Identifier: Apache-2.0
"""Research prototype: answer a Gather Context call from the live files.

Evaluation tooling for ``docs/arbiter/topical-index-research.md`` (user
direction, 27 November 2026). No index of file contents and no model: the
topical map (``topical_index.py``) says which names exist and which files
are about what, and everything else is read from the files at request time.

One call does, in order:

1. terms: the caller's anchors, the identifiers and quoted strings in the
   task and needs, and the names the map knows that hold the request's
   feature words (``retention`` finds ``DefaultRetentionDays``);
2. census: an exact search of every text file for each term, all
   occurrences counted, so the reply can say how many files were searched;
3. files: ranked by the distinct terms they hold, rarer terms and defining
   files counting more, the role fitted to the deliverable;
4. ranges: in the best files, each hit widened to the definition around it
   from the map's definition lines;
5. the reply: the service's brief layout with a census section first and
   ranges before text, short ranges carried as passages.

``replay`` scores the reply for each recorded agent call with the same
placement as ``gather_context_call_replay.py``.

    python -B live_gather.py replay --campaign <dir> --calls <calls.json> --maps <dir of *.map.json> --output <new dir>
"""

from __future__ import annotations

import argparse
import json
import math
import pathlib
import re
import sys
import time

import agent_cost_batch
import evidence
import gather_context_call_replay as replay_check
import gather_context_dose
import topical_index

SCHEMA = "airk.private.live-gather/1"
MAX_BRIEF_BYTES = gather_context_dose.MAX_BRIEF_BYTES
PASSAGE_BYTES = 8_000
EXPANDED_NAMES = 28
TOP_FILES = 16
RANGES_PER_FILE = 4
RANGE_LINES = 60
PASSAGE_LINES = 14
CENSUS_FILES = 10
CENSUS_LINES = 8
IDENTIFIER = re.compile(r"[A-Za-z_][A-Za-z0-9_]*(?:[.:][A-Za-z_][A-Za-z0-9_]*)*")
QUOTED = re.compile(r"`([^`\n]{2,80})`|'([^'\n]{3,60})'|\"([^\"\n]{3,60})\"")
STOP = frozenset("""the and for that this with from have has are was were will would should could into than then them they
    their there which where when what how why who does done make makes made add adds added use uses used using need needs
    needed change changes changed update updates find finds show shows shown each every all any some more most must only
    also not but can may its it's our your you his her after before between about above below under over such same new
    code file files test tests place places thing things way ways one two also still just like want wants instead keep
    case cases call calls called give gives given take takes taken work works does exist exists existing currently
    default defaults value values setting settings option options name names""".split())
ROLE_WEIGHT = {"source": 1.0, "test": 0.8, "mock": 0.8, "configuration": 0.7, "build": 0.6, "document": 0.35, "guidance": 0.9}


class Repository:
    """The live files of one snapshot, read once a process, and its topical map."""

    def __init__(self, source: pathlib.Path, index: dict):
        self.source, self.index = source, index
        self.texts: dict[str, list[str]] = {}
        for path, entry in topical_index.text_files(source):
            text = topical_index.read_text(entry)
            if text is not None:
                self.texts[path] = text.splitlines()

    def search(self, term: str, exact: bool) -> dict[str, list[int]]:
        """Every line of every file that holds ``term``: a whole identifier, or a phrase without regard to case."""
        pattern = (re.compile(r"(?<![A-Za-z0-9_])" + re.escape(term) + r"(?![A-Za-z0-9_])") if exact
                   else re.compile(re.escape(term), re.IGNORECASE))
        probe = term if exact else None
        found = {}
        for path, lines in self.texts.items():
            hits = [number for number, line in enumerate(lines, 1)
                    if (probe is None or probe in line) and pattern.search(line)]
            if hits:
                found[path] = hits
        return found


def is_name(token: str) -> bool:
    """Whether a token reads as an identifier an author chose, not a plain word."""
    return bool(re.search(r"[a-z][A-Z]|_|[A-Za-z]\d|\d[A-Za-z]|[.:]", token) or (token.isupper() and len(token) >= 3))


def request_terms(arguments: dict, index: dict) -> tuple[list[dict], list[str]]:
    """The terms to search for, most telling first, and the request's feature words."""
    text = " ".join([str(arguments.get("task") or "")] + [str(need) for need in arguments.get("needs") or []])
    terms: dict[str, dict] = {}

    def add(term: str, source: str, exact: bool) -> None:
        term = term.strip()
        if len(term) >= 3 and term.lower() not in STOP and term not in terms:
            terms[term] = {"term": term, "source": source, "exact": exact}

    for anchor in arguments.get("anchors") or []:
        anchor = str(anchor)
        add(anchor, "anchor", bool(IDENTIFIER.fullmatch(anchor)) and " " not in anchor)
    for match in QUOTED.finditer(text):
        phrase = next(group for group in match.groups() if group)
        add(phrase, "quoted", bool(IDENTIFIER.fullmatch(phrase)))
    for token in IDENTIFIER.findall(text):
        if is_name(token):
            add(token.split(".")[-1].split(":")[-1] if "." in token or ":" in token else token, "named", True)
            if "." in token:
                add(token, "named", False)
    words = []
    for word in topical_index.words_of(text + " " + " ".join(str(anchor) for anchor in arguments.get("anchors") or [])):
        if len(word) >= 4 and word not in STOP and word not in words:
            words.append(word)
    # Names the map knows that hold the request's feature words: more words held and rarer words first.
    word_names = index["word_names"]
    held: dict[str, set[str]] = {}
    for word in words:
        for name in word_names.get(word, []) + (word_names.get(word[:-1], []) if word.endswith("s") else []):
            if " " not in name:
                held.setdefault(name, set()).add(word)
    rarity = {word: math.log(1 + len(index["files"]) / (1 + len(index["word_files"].get(word, [])))) for word in words}
    ranked = sorted(held, key=lambda name: (-len(held[name]), -sum(rarity[word] for word in held[name]), len(name), name))
    for name in ranked[:EXPANDED_NAMES]:
        if len(held[name]) >= 2 or len(word_names.get(next(iter(held[name])), [])) <= 12:
            add(name, "map", True)
    return list(terms.values()), words


def gather(repository: Repository, arguments: dict) -> dict:
    """The brief for one call, from the live files."""
    started = time.monotonic()
    index = repository.index
    terms, words = request_terms(arguments, index)
    deliverable = str(arguments.get("deliverable") or "")
    wants_test = deliverable == "test" or "test" in str(arguments.get("task") or "").lower()
    census, score, held = [], {}, {}
    files_total = len(repository.texts)
    for term in terms:
        found = repository.search(term["term"], term["exact"])
        occurrences = sum(len(lines) for lines in found.values())
        if not found:
            census.append({"term": term["term"], "occurrences": 0, "files": 0})
            continue
        if len(found) > files_total * 0.2:
            census.append({"term": term["term"], "occurrences": occurrences, "files": len(found), "note": "too common to list"})
            continue
        weight = math.log(1 + files_total / len(found)) * (2.0 if term["source"] in ("anchor", "quoted", "named") else 1.0)
        for path, lines in found.items():
            defines = term["term"] in index["files"].get(path, {}).get("defines", {})
            score[path] = score.get(path, 0.0) + weight * (1.6 if defines else 1.0)
            held.setdefault(path, {})[term["term"]] = lines
        census.append({"term": term["term"], "occurrences": occurrences, "files": len(found), "found": found})
    rarity = {word: math.log(1 + files_total / (1 + len(index["word_files"].get(word, [])))) for word in words}
    for word in words:
        for path in index["word_files"].get(word, []):
            if word in index["files"][path]["path_words"]:
                score[path] = score.get(path, 0.0) + 0.5 * rarity[word]

    def fitted(path: str) -> float:
        role = index["files"].get(path, {}).get("role", "source")
        weight = ROLE_WEIGHT.get(role, 1.0)
        if role == "test" and wants_test:
            weight = 1.2
        if role == "document" and deliverable == "document":
            weight = 1.0
        return score[path] * weight * (1 + 0.25 * len(held.get(path, {})))

    ranked = sorted((path for path in score if path in held), key=lambda path: (-fitted(path), path))
    top = ranked[:TOP_FILES]
    ranges = []
    for path in top:
        record, lines = index["files"].get(path, {}), repository.texts[path]
        starts = sorted({line for numbers in record.get("defines", {}).values() for line in numbers})
        spans: list[list] = []
        for term, numbers in held[path].items():
            for number in numbers:
                before = [line for line in starts if line <= number]
                after = [line for line in starts if line > number]
                first = before[-1] if before and number - before[-1] <= RANGE_LINES else max(1, number - 3)
                last = min(after[0] - 1 if after else len(lines), first + RANGE_LINES - 1, len(lines))
                last = max(last, min(number + 2, len(lines)))
                spans.append([first, last, {term}])
        spans.sort(key=lambda span: (span[0], span[1]))
        merged: list[list] = []
        for span in spans:
            if merged and span[0] <= merged[-1][1] + 1 and max(span[1], merged[-1][1]) - merged[-1][0] < RANGE_LINES * 2:
                merged[-1][1] = max(merged[-1][1], span[1])
                merged[-1][2] |= span[2]
            else:
                merged.append(span)
        merged.sort(key=lambda span: (-len(span[2]), span[0]))
        for first, last, matched in merged[:RANGES_PER_FILE]:
            head = next((line.strip() for line in lines[first - 1:last] if line.strip()), "")
            ranges.append({"at": f"{path}:{first}-{last}", "why": head[:80] + " | " + ", ".join(sorted(matched))[:80],
                           "rank": fitted(path), "lines": last - first + 1, "path": path, "first": first, "last": last})
    passages, spent = [], 0
    for item in ranges:
        if item["lines"] > PASSAGE_LINES:
            continue
        text = "\n".join(repository.texts[item["path"]][item["first"] - 1:item["last"]]) + "\n"
        size = len(text.encode("utf-8"))
        if spent + size <= PASSAGE_BYTES:
            passages.append({"at": item["at"], "needs": [1], "text": text})
            spent += size
    shown = []
    for entry in census:
        row = {key: entry[key] for key in ("term", "occurrences", "files", "note") if key in entry}
        if "found" in entry:
            order = sorted(entry["found"], key=lambda path: (-(fitted(path) if path in score else 0.0), path))
            row["in"] = [f"{path}:{','.join(str(line) for line in entry['found'][path][:CENSUS_LINES])}"
                         + ("…" if len(entry["found"][path]) > CENSUS_LINES else "") for path in order[:CENSUS_FILES]]
            if len(order) > CENSUS_FILES:
                row["more_files"] = len(order) - CENSUS_FILES
        shown.append(row)
    guidance = sorted(path for path, record in index["files"].items() if record["role"] == "guidance")
    brief = {"schema": gather_context_dose.BRIEF_SCHEMA, "task": str(arguments.get("task") or "")[:512], "result": "complete",
             "searched": f"{files_total} text files read live; every occurrence of each term is counted",
             "census": shown,
             "needs": [{"need": 1, "text": "everything this task needs", "origin": "caller", "status": "covered",
                        "where": [{"at": item["at"], "why": item["why"]} for item in ranges]}],
             "passages": passages,
             "next": {"paths": ranked[TOP_FILES:TOP_FILES + 12] + guidance[:4],
                      "ask": "Ask again with narrower needs or exact names for what is still open."}}
    while len(json.dumps(brief, ensure_ascii=False, separators=(",", ":")).encode("utf-8")) > MAX_BRIEF_BYTES:
        if brief["passages"]:
            brief["passages"].pop()
        elif len(brief["needs"][0]["where"]) > 8:
            brief["needs"][0]["where"].pop()
        elif any(len(row.get("in", [])) > 3 for row in brief["census"]):
            for row in brief["census"]:
                if len(row.get("in", [])) > 3:
                    row["more_files"] = row.get("more_files", 0) + len(row["in"]) - 3
                    row["in"] = row["in"][:3]
        else:
            break
    brief["elapsed_ms"] = round((time.monotonic() - started) * 1000)
    return brief


def census_spans(brief: dict) -> list[str]:
    """Census lines as single-line locations, for the placement check."""
    found = []
    for row in brief.get("census") or []:
        for entry in row.get("in") or []:
            path, _, numbers = entry.rstrip("…").rpartition(":")
            found += [f"{path}:{number}-{number}" for number in numbers.split(",") if number.isdigit()]
    return found


def replay(args) -> int:
    recorded = json.loads(args.calls.read_text(encoding="utf-8"))["calls"]
    rows = {row["id"]: row for row in agent_cost_batch.working_set(args.campaign)}
    args.output.mkdir(parents=True)
    repositories: dict[str, Repository] = {}
    total = {stage: 0 for stage in replay_check.STAGES}
    results = []
    for task_id in sorted(recorded):
        if task_id not in rows or (args.tasks and task_id not in args.tasks):
            continue
        row = rows[task_id]
        folder = row["snapshot"].name
        if folder not in repositories:
            repositories[folder] = Repository(row["snapshot"], json.loads((args.maps / f"{folder}.map.json").read_text(encoding="utf-8")))
        brief = gather(repositories[folder], recorded[task_id])
        (args.output / f"{task_id}.brief.json").write_text(json.dumps(brief, ensure_ascii=False, indent=1) + "\n", encoding="utf-8", newline="\n")
        scored = {**brief, "omitted": census_spans(brief)}
        stages = replay_check.classify(json.loads(row["task"].read_text(encoding="utf-8")), scored, {})
        counts = {stage: sum(value == stage for value in stages.values()) for stage in replay_check.STAGES}
        for stage in replay_check.STAGES:
            total[stage] += counts[stage]
        size = len(json.dumps(brief, ensure_ascii=False, separators=(",", ":")).encode("utf-8"))
        results.append({"task": task_id, "notes": stages, **counts, "brief_bytes": size, "elapsed_ms": brief["elapsed_ms"],
                        "terms": len(brief["census"]), "ranges": len(brief["needs"][0]["where"]), "passages": len(brief["passages"])})
        print(f"{task_id:<22} " + " ".join(f"{stage}={counts[stage]}" for stage in replay_check.STAGES if stage != "gathered")
              + f" | {size} bytes, {results[-1]['terms']} terms, {results[-1]['ranges']} ranges, {brief['elapsed_ms']} ms | not at lines: "
              + " ".join(f"{note}:{stage}" for note, stage in stages.items() if stage in ("file", "missed")))
    notes = sum(total.values())
    print(json.dumps({"tasks": len(results), "notes": notes, **total,
                      "at_the_lines": total["passage"] + total["where"] + total["near"],
                      "pointed_at_the_file": notes - total["missed"] - total["gathered"]}))
    (args.output / "replay.json").write_text(json.dumps({"schema": SCHEMA, "total": total, "tasks": results}, indent=2, sort_keys=True) + "\n",
                                             encoding="utf-8", newline="\n")
    return 0


def main(argv: list[str] | None = None) -> int:
    parser = argparse.ArgumentParser(description=__doc__.splitlines()[0])
    parser.add_argument("command", choices=("replay",))
    parser.add_argument("--campaign", required=True, type=pathlib.Path)
    parser.add_argument("--calls", required=True, type=pathlib.Path)
    parser.add_argument("--maps", required=True, type=pathlib.Path)
    parser.add_argument("--output", required=True, type=pathlib.Path)
    parser.add_argument("--tasks", nargs="*")
    args = parser.parse_args(argv)
    sys.stdout.reconfigure(encoding="utf-8", errors="replace")
    return replay(args)


if __name__ == "__main__":
    sys.exit(main())
