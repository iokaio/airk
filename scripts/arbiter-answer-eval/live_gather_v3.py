# SPDX-License-Identifier: Apache-2.0
"""Research prototype, third version: the chosen aspects joined to the map and the reply.

Evaluation tooling for ``docs/arbiter/topical-index-research.md``
("Aspects", chosen by the user on 27 November 2026). It is kept in its own
module while a cost series is running on the second version
(``live_gather.py``), which must not change under that series. It adds, on
top of the second version:

- to the map: configuration and script keys among the defined names (6),
  named values and stated quantities (5), guidance and document sections
  (7, 10), entry points and public names (9), test links (4), and where
  each code name is documented (10);
- to the reply: the tests of the best files, places that hold the same
  quantity the request states, the rule sections that govern the files
  touched, entry points for a tracing request, and the document sections
  that mention the names found.

    python -B live_gather_v3.py build --source <repository folder> --output <map.json>
    python -B live_gather_v3.py replay --campaign <dir> --calls <calls.json> --maps <dir of v3 maps> --output <new dir>
"""

from __future__ import annotations

import argparse
import json
import pathlib
import sys
import time

import agent_cost_batch
import gather_context_call_replay as replay_check
import live_gather
import topical_aspects as aspects
import topical_index

SCHEMA = "airk.private.topical-index/3"
VALUES_PER_FILE = 80
TOP_FOR_TESTS = 8
RULE_SECTIONS = 2
SAME_QUANTITY = 8
DOCUMENTED = 5
ENTRY_POINTS = 5
TRACE_WORDS = ("trace", "flow", "path", "end to end", "from the moment", "passes through", "what happens when")


def build(source: pathlib.Path) -> dict:
    """The second version's map with the chosen aspects added."""
    started = time.monotonic()
    index = topical_index.build(source)
    index["schema"] = SCHEMA
    files = index["files"]
    word_names = {word: set(names) for word, names in index["word_names"].items()}
    word_files = {word: set(paths) for word, paths in index["word_files"].items()}
    defined = {name for record in files.values() for name in record["defines"]}
    documented: dict[str, list[str]] = {}
    for path, entry in topical_index.text_files(source):
        record = files.get(path)
        text = topical_index.read_text(entry) if record else None
        if text is None:
            continue
        role = record["role"]
        if role in ("configuration", "build"):
            for key, lines in aspects.config_keys(path, text).items():
                if key not in record["defines"]:
                    record["defines"][key] = lines
                    for word in topical_index.words_of(key):
                        word_names.setdefault(word, set()).add(key)
                        word_files.setdefault(word, set()).add(path)
        if role in ("document", "guidance"):
            sections = aspects.sections_of(text)
            record["sections"] = [{"heading": row["heading"], "start": row["start"], "end": row["end"],
                                   **({"words": row["words"]} if role == "guidance" else {})} for row in sections]
            for row in sections:
                for name in row["names"]:
                    if name in defined and len(documented.setdefault(name, [])) < 12:
                        documented[name].append(f"{path}:{row['start']}-{row['end']}")
            record["quantities"] = [[row["line"], row["kind"], row["base"]] for row in aspects.quantities_of(text)[:VALUES_PER_FILE]]
        else:
            record["values"] = [[row["name"], row["line"], row["kind"], row["base"]] for row in aspects.values_of(text)
                                if "kind" in row][:VALUES_PER_FILE]
            if role == "source":
                record["entry"] = aspects.entry_point(path, text)
    index["word_names"] = {word: sorted(names) for word, names in sorted(word_names.items())}
    index["word_files"] = {word: sorted(paths) for word, paths in sorted(word_files.items())}
    index["tests_of"] = aspects.test_links(files)
    index["documented_in"] = dict(sorted(documented.items()))
    index["seconds"] = round(time.monotonic() - started, 2)
    return index


def gather(repository: live_gather.Repository, arguments: dict) -> dict:
    """The second version's brief with the aspect sections added."""
    started = time.monotonic()
    brief = live_gather.gather(repository, arguments)
    index, files = repository.index, repository.index["files"]
    request = " ".join([str(arguments.get("task") or "")] + [str(need) for need in arguments.get("needs") or []])
    words = set(topical_index.words_of(request))
    where = brief["needs"][0]["where"]
    top = list(dict.fromkeys(row["at"].rpartition(":")[0] for row in where))
    terms = [row["term"] for row in brief["census"] if row.get("occurrences")]
    term_words = words | {word for term in terms for word in topical_index.words_of(term)}

    tests = [{"file": path, "tests": index["tests_of"][path][:4]} for path in top[:TOP_FOR_TESTS] if path in index.get("tests_of", {})]

    same = []
    for stated in aspects.quantities_of(request):
        if stated["kind"] not in ("time", "size") and stated["base"] < 2:
            continue
        for path, record in files.items():
            for name, line, kind, base in record.get("values", []):
                if kind == stated["kind"] and base == stated["base"] and words & set(topical_index.words_of(name + " " + path)):
                    same.append((0, f"{path}:{line}", f"{name} is {stated['text']}"))
            for line, kind, base in record.get("quantities", []):
                if kind == stated["kind"] and base == stated["base"]:
                    text = repository.texts[path][line - 1] if line <= len(repository.texts.get(path, [])) else ""
                    shared = len(words & set(topical_index.words_of(text)))
                    if shared >= 2:
                        same.append((1 + 1 / shared, f"{path}:{line}", text.strip()[:90]))
    same = [{"at": at, "why": why} for _, at, why in sorted(set(same))[:SAME_QUANTITY]]

    governing = list(dict.fromkeys(guide for path in top[:TOP_FOR_TESTS] for guide in aspects.governing(path, files)))
    rules = []
    for guide in governing:
        for row in files[guide].get("sections", []):
            shared = term_words & set(row.get("words", []))
            if len(shared) >= 5:
                rules.append((-len(shared), guide, row["start"], row["end"], row["heading"]))
    rules = [{"at": f"{guide}:{start}-{end}", "why": f"rule section: {heading}"[:90]}
             for _, guide, start, end, heading in sorted(rules)[:RULE_SECTIONS]]

    tracing = str(arguments.get("deliverable") or "") == "analysis" or any(word in request.lower() for word in TRACE_WORDS)
    holding = {entry.rpartition(":")[0] for row in brief["census"] for entry in row.get("in") or []}
    entries = sorted(path for path in holding if files.get(path, {}).get("entry"))[:ENTRY_POINTS] if tracing else []

    documented = []
    for term in terms:
        for at in index.get("documented_in", {}).get(term, [])[:2]:
            if len(documented) < DOCUMENTED:
                documented.append(f"{term}: {at}")

    for key, value in (("same_quantity", same), ("rules", rules), ("tests", tests), ("entry_points", entries), ("documented", documented)):
        if value:
            brief[key] = value
    brief.pop("elapsed_ms", None)
    while len(json.dumps(brief, ensure_ascii=False, separators=(",", ":")).encode("utf-8")) > live_gather.MAX_BRIEF_BYTES:
        if brief["passages"]:
            brief["passages"].pop()
        elif len(where) > 8:
            where.pop()
        elif brief.get("documented"):
            brief["documented"].pop()
        else:
            break
    brief["elapsed_ms"] = round((time.monotonic() - started) * 1000)
    return brief


def scored(brief: dict) -> dict:
    """The brief as the placement check reads it: every location any section names."""
    extra = live_gather.census_spans(brief)
    extra += [row["at"] if "-" in row["at"].rpartition(":")[2] else f"{row['at']}-{row['at'].rpartition(':')[2]}"
              for row in (brief.get("same_quantity") or []) + (brief.get("rules") or [])]
    extra += [entry.split(": ", 1)[1] for entry in brief.get("documented") or []]
    paths = list((brief.get("next") or {}).get("paths") or []) + list(brief.get("entry_points") or [])
    paths += [test for row in brief.get("tests") or [] for test in row["tests"]]
    return {**brief, "omitted": extra, "next": {"paths": paths}}


def replay(args) -> int:
    recorded = json.loads(args.calls.read_text(encoding="utf-8"))["calls"]
    rows = {row["id"]: row for row in agent_cost_batch.working_set(args.campaign)}
    args.output.mkdir(parents=True)
    repositories: dict[str, live_gather.Repository] = {}
    total = {stage: 0 for stage in replay_check.STAGES}
    results = []
    for task_id in sorted(recorded):
        if task_id not in rows or (args.tasks and task_id not in args.tasks):
            continue
        row = rows[task_id]
        folder = row["snapshot"].name
        if folder not in repositories:
            repositories[folder] = live_gather.Repository(row["snapshot"], json.loads((args.maps / f"{folder}.map.json").read_text(encoding="utf-8")))
        brief = gather(repositories[folder], recorded[task_id])
        (args.output / f"{task_id}.brief.json").write_text(json.dumps(brief, ensure_ascii=False, indent=1) + "\n", encoding="utf-8", newline="\n")
        stages = replay_check.classify(json.loads(row["task"].read_text(encoding="utf-8")), scored(brief), {})
        counts = {stage: sum(value == stage for value in stages.values()) for stage in replay_check.STAGES}
        for stage in replay_check.STAGES:
            total[stage] += counts[stage]
        size = len(json.dumps(brief, ensure_ascii=False, separators=(",", ":")).encode("utf-8"))
        added = {key: len(brief.get(key) or []) for key in ("same_quantity", "rules", "tests", "entry_points", "documented")}
        results.append({"task": task_id, "notes": stages, **counts, "brief_bytes": size, "elapsed_ms": brief["elapsed_ms"], "added": added})
        print(f"{task_id:<22} " + " ".join(f"{stage}={counts[stage]}" for stage in replay_check.STAGES if stage != "gathered")
              + f" | {size} bytes, {brief['elapsed_ms']} ms, added {' '.join(f'{key[:5]}={value}' for key, value in added.items())} | not at lines: "
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
    parser.add_argument("command", choices=("build", "replay"))
    parser.add_argument("--source", type=pathlib.Path)
    parser.add_argument("--campaign", type=pathlib.Path)
    parser.add_argument("--calls", type=pathlib.Path)
    parser.add_argument("--maps", type=pathlib.Path)
    parser.add_argument("--output", required=True, type=pathlib.Path)
    parser.add_argument("--tasks", nargs="*")
    args = parser.parse_args(argv)
    sys.stdout.reconfigure(encoding="utf-8", errors="replace")
    if args.command == "build":
        index = build(args.source.resolve(strict=True))
        data = json.dumps(index, ensure_ascii=False, separators=(",", ":"))
        args.output.write_text(data, encoding="utf-8", newline="\n")
        print(json.dumps({"files": len(index["files"]), "map_bytes": len(data.encode("utf-8")), "seconds": index["seconds"],
                          "tested_sources": len(index["tests_of"]), "documented_names": len(index["documented_in"]),
                          "entry_points": sum(bool(record.get("entry")) for record in index["files"].values())}))
        return 0
    return replay(args)


if __name__ == "__main__":
    sys.exit(main())
