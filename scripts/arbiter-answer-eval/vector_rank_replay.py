# SPDX-License-Identifier: Apache-2.0
"""Replay the frozen development queries against a private vector census lifetime."""

from __future__ import annotations

import argparse
import json
import math
from pathlib import Path
import sys

HERE = Path(__file__).resolve().parent
if str(HERE) not in sys.path:
    sys.path.insert(0, str(HERE))

import common
import evidence
import search_focus_ab as search
import search_vector_audit as audit
import settled


MARKER = "AIRK_SEARCH_VECTOR_CENSUS "


def read_event(raw, query, token, pipeline):
    events = list(audit.log_rows(raw, MARKER))
    common.require(len(events) == 1, "expected one native vector census event per Search request")
    event = events[0]
    _, incarnation, sequence, digest = token.split(":")
    common.require(event["query_sha256"] == common.sha256_bytes(query.encode("utf-8"))
                   and event["publication"] == {"incarnation": incarnation, "sequence": int(sequence), "digest": digest}
                   and event["pipeline"] == pipeline,
                   "vector census event belongs to a different query, pipeline or publication")
    common.require(isinstance(event["ranks"], list) and 1 <= len(event["ranks"]) <= 4,
                   "vector census event has an invalid embedding roster")
    return event["ranks"]


def rank_note(ranked, note, citations, union_only):
    return audit.first_covering_rank(
        ((rank, citations.get((path, ordinal))) for rank, path, ordinal in ranked),
        note, union_only=union_only,
    )


def request(args, handle, row, query, identity, service_log):
    wire = {**row["arguments"], "query": query}
    with service_log.open("rb") as log:
        log.seek(0, 2)
        outcome = common.call_tool(handle, "airk_search", row["repo"], wire, args.timeout,
                                   strip_env=common.AGENT_STRIPPED_ENV)
        trace = log.read(32 * 1024 * 1024 + 1)
    common.require(outcome.ok and len(trace) <= 32 * 1024 * 1024, "native Search or its census trace failed")
    body = common.envelope_body(outcome.envelope)
    publication = identity["publication_token"]
    common.require((body.get("publication") or {}).get("token") == publication
                   and body["execution"]["served_mode"] == "hybrid" and body.get("focus") is None,
                   "vector census requires the pinned hybrid execution with optional stages off")
    ranked = read_event(trace, query, publication, identity["pipeline_digest"])
    return ranked, [evidence.extract(hit) for hit in body["hits"]]


def collect(args):
    common.require(math.isfinite(args.timeout) and args.timeout > 0, "invalid Search timeout")
    cohort = json.loads(search.pinned(args.cohort, args.cohort_sha256))
    snapshots = json.loads(search.pinned(args.snapshots, args.snapshots_sha256))["snapshots"]
    audit_input = json.loads(search.pinned(args.audit_input, args.audit_input_sha256))
    common.require(audit_input == audit.prepare(cohort, snapshots), "census and detached audit cohorts differ")
    paths = audit.native_paths(search.pinned(args.audit_native, args.audit_native_sha256), audit_input)
    raw = json.loads(search.pinned(args.handle, args.handle_sha256))
    handle = common.load_handle(args.handle)
    common.require(handle.sha256 == args.handle_sha256 and handle.state == "ready", "census lifetime is not pinned Ready")
    gpu = json.loads((handle.path.parent / "gpu.json").read_bytes())
    common.require(gpu.get("vector_census_sha256") == args.audit_input_sha256
                   and gpu.get("search") == "semantic_subqueries" and gpu.get("reuse_settled") is True,
                   "census requires the pinned roster and the reviewed focused-query profile")
    identity = search.live_identity(handle, raw, sorted(audit.REPOSITORIES))
    indexes = identity["indexes"]
    before = {repo: settled.snapshot(handle, repo, args.timeout) for repo in indexes}
    stage = search.stage(handle, None, args.timeout)
    for repo, observed in indexes.items():
        expected = snapshots[audit.REPOSITORIES[repo]]
        common.require(observed["physical"] == audit.REPOSITORIES[repo]
                       and observed["pipeline_digest"] == expected["pipeline"]
                       and observed["source_commit"] == expected["source_commit"]
                       and observed["publication_token"] == before[repo]["publication_token"] == expected["publication_token"]
                       and before[repo]["settled"], "census differs from the authenticated control publication")

    service_log = handle.path.parent / "service.log"
    results = []
    for row in cohort["rows"]:
        repo = row["repo"]
        repository = audit.REPOSITORIES[repo]
        instruction, delivered = request(args, handle, row, row["arguments"]["query"], indexes[repo], service_log)
        facts = {}
        for note in row["essential"]:
            fact = note["fact"]
            common.require(isinstance(fact, str) and bool(fact.strip()), "census note has no fact sentence")
            if fact not in facts:
                facts[fact], _ = request(args, handle, row, fact, indexes[repo], service_log)
            status, _ = audit.classify(repository, note, paths)
            locations = {audit.note_path(loc) for loc in audit.note_locations(note)}
            citations = {(path, unit["ordinal"]): audit.unit_span(unit["citation"])
                         for path in locations for unit in paths[(repository, path)]["units"]}
            union_only = status == "union_only"
            results.append({
                "repo": repo, "slot": row["slot"], "id": note["id"],
                "instruction_rank": rank_note(instruction[0], note, citations, union_only),
                "focused_ranks": [rank_note(branch, note, citations, union_only) for branch in instruction[1:]],
                "fact_rank": rank_note(facts[fact][0], note, citations, union_only),
                "instruction_delivered_rank": search.depth.ranks(delivered, note)["rule2"],
            })
    after = {repo: settled.snapshot(handle, repo, args.timeout) for repo in indexes}
    common.require(common.sha256_file(handle.path) == args.handle_sha256
                   and search.stage(handle, None, args.timeout)["revision"] == stage["revision"]
                   and all(settled.compare(before[repo], after[repo])["unchanged"] for repo in indexes),
                   "census publication, stage configuration or handle changed")
    return {"schema": "airk.private.vector-rank-census/1", "identity": identity, "notes": results,
            "input_sha256": {name: getattr(args, f"{name}_sha256") for name in
                             ("cohort", "snapshots", "audit_input", "audit_native", "handle")},
            "scope": "Unbounded exact vector occurrence rank within the admitted publication. An empty focused list "
                     "means no auxiliary query was available. Fact-derived ranks are for diagnostics only."}


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    for name in ("cohort", "snapshots", "audit-input", "audit-native", "handle"):
        parser.add_argument(f"--{name}", type=Path, required=True)
        parser.add_argument(f"--{name}-sha256", required=True)
    parser.add_argument("--timeout", type=float, default=90)
    parser.add_argument("--output", type=Path, required=True)
    args = parser.parse_args()
    search.write_new(args.output, collect(args))


if __name__ == "__main__":
    try:
        main()
    except (common.HarnessError, OSError, ValueError, KeyError, TypeError) as error:
        print(str(error), file=sys.stderr)
        sys.exit(1)
