# SPDX-License-Identifier: Apache-2.0
"""Audit committed passage coordinates for the frozen Search notes."""

from __future__ import annotations

import argparse
from collections import Counter, defaultdict
import json
from pathlib import Path, PurePosixPath
import sys

HERE = Path(__file__).resolve().parent
if str(HERE) not in sys.path:
    sys.path.insert(0, str(HERE))

import common
import evidence
import search_focus_ab as search


REPOSITORIES = {"bookbuilder": "bookbuilder-v2", "munarium": "munarium"}
NATIVE_MARKER = "AIRK_COMMITTED_VECTOR_CITATIONS "
TRACE_MARKER = "AIRK_SEARCH_SEMANTIC_RANKS "


def note_locations(note):
    return (note, *(note.get("alternatives") or []))


def note_path(location):
    path = evidence.norm_path(location["path"])
    parts = path.split("/")
    common.require(not PurePosixPath(path).is_absolute() and not any(p in {"", ".", ".."} for p in parts)
                   and ":" not in parts[0], "expected a collection-relative note path")
    lines = location["lines"]
    common.require(isinstance(lines, list) and len(lines) == 2 and all(type(n) is int for n in lines)
                   and 1 <= lines[0] <= lines[1], "expected positive source lines for every note location")
    return path


def prepare(cohort, snapshots):
    rows = cohort["rows"]
    search.roster(rows)
    common.require(cohort["schema"] == search.SCHEMA and cohort["kind"] == "cohort"
                   and sum(len(row["essential"]) for row in rows) == 113,
                   "expected the original 113-note Search cohort")
    common.require(set(snapshots) == set(REPOSITORIES.values()), "snapshot collection roster differs")
    collections = []
    for repo, repository in REPOSITORIES.items():
        snapshot = snapshots[repository]
        paths = sorted({note_path(location) for row in rows if row["repo"] == repo
                        for note in row["essential"] for location in note_locations(note)})
        collections.append({"repository": repository, "database": snapshot["directory"],
                            "publication_sequence": snapshot["sequence"], "pipeline": snapshot["pipeline"],
                            "committed_rows": snapshot["committed_rows"], "db_sha256": snapshot["db_sha256"],
                            "paths": paths})
    return {"collections": collections}


def log_rows(raw, marker):
    for line in raw.decode("utf-8-sig").splitlines():
        if marker in line:
            yield json.loads(line.partition(marker)[2])


def native_paths(raw, audit_input):
    requested = {(c["repository"], path): c["committed_rows"] for c in audit_input["collections"]
                 for path in c["paths"]}
    paths = {}
    for record in log_rows(raw, NATIVE_MARKER):
        key = (record["repository"], record["path"])
        common.require(key in requested and key not in paths, "unexpected or repeated native audit path")
        units = record["units"]
        common.require(type(record["source_present"]) is bool and isinstance(units, list)
                       and (record["source_present"] or not units), "invalid native path result")
        for unit in units:
            row = unit["row"]
            common.require(type(row) is int and 0 <= row < requested[key] and unit["citation"]["path"] == key[1],
                           "native vector unit differs from requested path or committed rows")
        paths[key] = record
    common.require(set(paths) == set(requested), "native audit did not return every requested path")
    return paths


def unit_span(citation):
    native = citation["location"].get("Lines")
    if native is None:
        return None
    start, end = native["start"], native["end"]
    common.require(type(start) is int and type(end) is int and 1 <= start <= end
                   and (start, end) == (citation["start_line"], citation["end_line"]),
                   "native line citation differs from its coordinates")
    # Native materialization verified the presence of text. Retain only coordinates.
    return evidence.Span(citation["path"], start, end, "", "text")


def union_overlap(spans, lines):
    first, last = lines
    intervals = sorted((max(span.start, first), min(span.end, last)) for span in spans
                       if evidence.overlaps(span, lines))
    total, last_counted = 0, first - 1
    for start, end in intervals:
        total += max(0, end - max(last_counted + 1, start) + 1)
        last_counted = max(last_counted, end)
    return total


def classify(repository, note, paths):
    records = {note_path(location): paths[(repository, note_path(location))]
               for location in note_locations(note)}
    present = [r for r in records.values() if r["source_present"]]
    if not present:
        return "source_absent", []
    units = [unit for record in present for unit in record["units"]]
    if not units:
        return "no_committed_units", []
    native = [(unit, span) for unit in units if (span := unit_span(unit["citation"])) is not None]
    if not native:
        return "no_native_lines", []
    covered = [unit for unit, span in native if evidence.covers([span], note, rule=2)]
    if covered:
        return "single_unit", covered
    for location in note_locations(note):
        path, lines = note_path(location), location["lines"]
        # Restrict a union to one actual source path and one accepted line range.
        spans = [span for _, span in native if span.path == path]
        if union_overlap(spans, lines) >= evidence.required_overlap(lines, rule=2):
            return "union_only", []
    return "no_coverage", []


def ranked_report(report, cohort, cohort_pin, snapshots):
    rows = search.report_rows(report)
    common.require(report["cohort_sha256"] == cohort_pin, "Focus comparison uses a different cohort")
    identities = report["identity"]["indexes"]
    common.require(set(identities) == set(REPOSITORIES), "Focus comparison index roster differs")
    for repo, repository in REPOSITORIES.items():
        observed, expected = identities[repo], snapshots[repository]
        common.require(observed["physical"] == repository and observed["pipeline_digest"] == expected["pipeline"]
                       and observed["source_commit"] == expected["source_commit"]
                       and observed["publication_token"] == expected["publication_token"],
                       f"{repo}: audit and Focus comparison publication differ")
    fields = ("repo", "slot", "request_sha256", "limit", "max_bytes", "response_mode")
    frozen = [(tuple(row[k] for k in fields), [n["id"] for n in row["essential"]]) for row in cohort["rows"]]
    found = [(tuple(row[k] for k in fields), [n["id"] for n in row["notes"]]) for row in rows]
    common.require(found == frozen, "Focus comparison requests or note order differ from the cohort")
    return {(row["repo"], row["slot"], note["id"]): {k: note[k] for k in ("off_rank", "focus_rank")}
            for row in rows for note in row["notes"]}


def trace_identity(query_sha, publication):
    return query_sha, publication["incarnation"], publication["sequence"], publication["digest"]


def trace_ranks(raw, cohort, snapshots):
    expected = {}
    for row in cohort["rows"]:
        repo = row["repo"]
        token = snapshots[REPOSITORIES[repo]]["publication_token"]
        _, incarnation, sequence, digest = token.split(":")
        key = trace_identity(common.sha256_bytes(row["arguments"]["query"].encode("utf-8")),
                             {"incarnation": incarnation, "sequence": int(sequence), "digest": digest})
        common.require(key not in expected, "ambiguous Search trace query identity")
        expected[key] = (repo, row["slot"])
    observed = defaultdict(list)
    for row in log_rows(raw, TRACE_MARKER):
        key = trace_identity(row["query_sha256"], row["publication"])
        common.require(key in expected, "unexpected Search trace query or publication")
        observed[expected[key]].append(row["ranked"])
    common.require(bool(observed), "no retained semantic ranks found")
    for arms in observed.values():
        common.require(len(arms) == 2 and arms[0] == arms[1],
                       "expected identical bounded vector lists from the two matched arms")
        common.require(1 <= len(arms[0]) <= 3 and sum(len(branch) for branch in arms[0]) <= 100,
                       "retained trace exceeds its semantic occurrence budget")
    return {key: arms[0] for key, arms in observed.items()}


def occurrence_ranks(ranked, units):
    citations = {(unit["citation"]["path"], search.digest(unit["members"])): unit["citation"] for unit in units}
    result = []
    for query, branch in enumerate(ranked):
        for rank, item in enumerate(branch, start=1):
            citation = citations.get((item["path"], search.digest(item["members"])))
            if citation is not None:
                source = {k: citation[k] for k in ("path", "start_line", "end_line")}
                result.append({"query": query, "rank": rank, "occurrence": item["occurrence"], "source": source})
    return result


def summarize(cohort, snapshots, inputs, pins):
    audit_input = json.loads(inputs["input"])
    common.require(audit_input == prepare(cohort, snapshots), "native input and authenticated roster differ")
    paths = native_paths(inputs["native"], audit_input)
    original = ranked_report(json.loads(inputs["original"]), cohort, pins["cohort"], snapshots)
    current = ranked_report(json.loads(inputs["current"]), cohort, pins["cohort"], snapshots)
    traces = trace_ranks(inputs["semantic_trace"], cohort, snapshots) if "semantic_trace" in inputs else {}
    notes = []
    for request in cohort["rows"]:
        repo, slot = request["repo"], request["slot"]
        retained = traces.get((repo, slot))
        for note in request["essential"]:
            key = (repo, slot, note["id"])
            status, units = classify(REPOSITORIES[repo], note, paths)
            notes.append({"repo": repo, "slot": slot, "id": note["id"], "coverage": status,
                          "qualifying_units": len(units), "original": original[key], "current": current[key],
                          "retained_semantic_ranks": None if retained is None else occurrence_ranks(retained, units)})
    groups = {"all": notes, **{repo: [n for n in notes if n["repo"] == repo] for repo in REPOSITORIES}}
    counts = {}
    for name, group in groups.items():
        counts[name] = {"all": dict(Counter(n["coverage"] for n in group))}
        for arm in ("off", "focus"):
            missed = [n for n in group if n["current"][f"{arm}_rank"] is None]
            counts[name][f"current_{arm}_misses"] = dict(Counter(n["coverage"] for n in missed))
    return {"schema": "airk.private.search-vector-coverage-audit/1", "input_sha256": pins,
            "counts": counts, "audited_paths": len(paths), "traced_requests": len(traces),
            "scope": "Exact normalized cohort paths, same published source revisions. Rule 2 uses the actual native "
                     "line citations. Union-only is a diagnostic over distinct lines in one accepted location. "
                     "Trace ranks are positions within individual bounded pre-fusion occurrence lists, with query "
                     "zero the full task. Null means untraced and an empty list means no qualifying occurrence in "
                     "the retained lists. Neither indicates a global vector rank.",
            "notes": notes}


def main(argv=None):
    parser = argparse.ArgumentParser(description=__doc__)
    commands = parser.add_subparsers(dest="command", required=True)
    prepare_cmd, summary_cmd = (commands.add_parser(name) for name in ("prepare", "summarize"))
    for command in (prepare_cmd, summary_cmd):
        command.add_argument("--output", type=Path, required=True)
        for name in ("cohort", "snapshots"):
            command.add_argument(f"--{name}", type=Path, required=True)
            command.add_argument(f"--{name}-sha256", required=True)
    for name in ("input", "native", "original", "current", "semantic-trace"):
        required = name != "semantic-trace"
        summary_cmd.add_argument(f"--{name}", type=Path, required=required)
        summary_cmd.add_argument(f"--{name}-sha256", required=required)
    args = parser.parse_args(argv)
    names = ["cohort", "snapshots"]
    if args.command == "summarize":
        names.extend(("input", "native", "original", "current"))
        common.require(bool(args.semantic_trace) == bool(args.semantic_trace_sha256),
                       "semantic trace and its pin must be supplied together")
        if args.semantic_trace:
            names.append("semantic_trace")
    pins = {name: getattr(args, f"{name}_sha256") for name in names}
    inputs = {name: search.pinned(getattr(args, name), pins[name]) for name in names}
    cohort = json.loads(inputs["cohort"])
    snapshots = json.loads(inputs["snapshots"])["snapshots"]
    result = prepare(cohort, snapshots) if args.command == "prepare" else summarize(cohort, snapshots, inputs, pins)
    search.write_new(args.output, result)
    return 0


if __name__ == "__main__":
    try:
        sys.exit(main())
    except (common.HarnessError, OSError, ValueError, KeyError, TypeError) as error:
        print(str(error), file=sys.stderr)
        sys.exit(1)
