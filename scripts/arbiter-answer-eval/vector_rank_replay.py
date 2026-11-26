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
    return verified_ranks(events[0], query, token, pipeline)


def verified_ranks(event, query, token, pipeline):
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
    common.require(outcome.ok, f"{row['repo']}/{row['slot']} Search failed after {outcome.elapsed_ms} ms "
                   f"({search.depth.error_code(outcome)})")
    common.require(len(trace) <= 32 * 1024 * 1024, "native census trace exceeded its per-request bound")
    body = common.envelope_body(outcome.envelope)
    publication = identity["publication_token"]
    common.require((body.get("publication") or {}).get("token") == publication
                   and body["execution"]["served_mode"] == "hybrid" and body.get("focus") is None,
                   "vector census requires the pinned hybrid execution with optional stages off")
    ranked = read_event(trace, query, publication, identity["pipeline_digest"])
    return ranked, [evidence.extract(hit) for hit in body["hits"]]


def load_cohort(args):
    cohort = json.loads(search.pinned(args.cohort, args.cohort_sha256))
    snapshots = json.loads(search.pinned(args.snapshots, args.snapshots_sha256))["snapshots"]
    audit_input = json.loads(search.pinned(args.audit_input, args.audit_input_sha256))
    common.require(audit_input == audit.prepare(cohort, snapshots), "census and detached audit cohorts differ")
    paths = audit.native_paths(search.pinned(args.audit_native, args.audit_native_sha256), audit_input)
    return cohort, snapshots, paths


def note_inventory(note, repository, paths):
    status, _ = audit.classify(repository, note, paths)
    locations = {audit.note_path(loc) for loc in audit.note_locations(note)}
    citations = {(path, unit["ordinal"]): audit.unit_span(unit["citation"])
                 for path in locations for unit in paths[(repository, path)]["units"]}
    return citations, status == "union_only"


def collect(args):
    common.require(math.isfinite(args.timeout) and args.timeout > 0, "invalid Search timeout")
    cohort, snapshots, paths = load_cohort(args)
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
    rows = [row for row in cohort["rows"] if args.case is None or f"{row['repo']}/{row['slot']}" == args.case]
    common.require(bool(rows), "case selector matched no frozen development scenario")
    for row in rows:
        repo = row["repo"]
        repository = audit.REPOSITORIES[repo]
        instruction, delivered = (None, []) if args.facts_only else request(
            args, handle, row, row["arguments"]["query"], indexes[repo], service_log,
        )
        facts = {}
        for note in row["essential"]:
            fact = note["fact"]
            common.require(isinstance(fact, str) and bool(fact.strip()), "census note has no fact sentence")
            if fact not in facts:
                facts[fact] = None if args.instruction_only else request(
                    args, handle, row, fact, indexes[repo], service_log,
                )[0]
            citations, union_only = note_inventory(note, repository, paths)
            results.append({
                "repo": repo, "slot": row["slot"], "id": note["id"],
                "instruction_rank": None if instruction is None else rank_note(instruction[0], note, citations, union_only),
                "focused_ranks": None if instruction is None else
                [rank_note(branch, note, citations, union_only) for branch in instruction[1:]],
                "fact_rank": None if facts[fact] is None else rank_note(facts[fact][0], note, citations, union_only),
                "semantic_subquery_arm_delivered_rank": search.depth.ranks(delivered, note)["rule2"],
            })
    after = {repo: settled.snapshot(handle, repo, args.timeout) for repo in indexes}
    common.require(common.sha256_file(handle.path) == args.handle_sha256
                   and search.stage(handle, None, args.timeout)["revision"] == stage["revision"]
                   and all(after[repo]["settled"] and settled.compare(before[repo], after[repo])["unchanged"]
                           for repo in indexes),
                   "census publication, stage configuration or handle changed")
    return {"schema": "airk.private.vector-rank-census/1", "identity": identity, "notes": results,
            "case": args.case, "facts_only": args.facts_only, "instruction_only": args.instruction_only,
            "input_sha256": {name: getattr(args, f"{name}_sha256") for name in
                             ("cohort", "snapshots", "audit_input", "audit_native", "handle")},
            "scope": "Unbounded exact vector occurrence rank within the admitted publication. An empty focused list "
                     "means no auxiliary query was available. Fact-derived ranks are for diagnostics only."}


def recover(args):
    """Recover measured vector ranks from a stopped replay's authenticated log."""
    cohort, snapshots, paths = load_cohort(args)
    handle = json.loads(search.pinned(args.handle, args.handle_sha256))
    common.require(handle["state"] == "closed"
                   and handle.get("service_environment", {}).get("AIRK_ARBITER_EVALUATION_SEARCH")
                   == "semantic_subqueries", "trace recovery requires the closed census handle")
    common.require(args.closed_trace.resolve() == args.handle.resolve().with_name("service.log"),
                   "trace must belong to the pinned lifetime")
    events = iter(audit.log_rows(search.pinned(args.closed_trace, args.closed_trace_sha256), MARKER))
    skipped = []
    for _ in range(args.skip_initial_events):
        event = next(events, None)
        common.require(event is not None, "closed trace has fewer startup events than declared")
        skipped.append({"query_sha256": event["query_sha256"], "publication": event["publication"]})
    observed = 0
    results = []
    for row in cohort["rows"]:
        repository = audit.REPOSITORIES[row["repo"]]
        frozen = snapshots[repository]
        pinned_index = handle["indexes"][repository]
        common.require(pinned_index["publication_token"] == frozen["publication_token"]
                       and pinned_index["pipeline_digest"] == frozen["pipeline"]
                       and pinned_index["source_commit"] == frozen["source_commit"],
                       "closed trace index differs from the frozen control")

        def next_ranks(query):
            nonlocal observed
            event = next(events, None)
            if event is None:
                return None
            try:
                ranks = verified_ranks(event, query, frozen["publication_token"], frozen["pipeline"])
            except common.HarnessError as error:
                raise common.HarnessError(f"{row['repo']}/{row['slot']} request {observed + 1}: {error}") from error
            observed += 1
            return ranks

        instruction = next_ranks(row["arguments"]["query"])
        facts = {}
        for note in row["essential"]:
            fact = note["fact"]
            if fact not in facts:
                facts[fact] = next_ranks(fact)
            citations, union_only = note_inventory(note, repository, paths)
            rank = lambda branch: rank_note(branch, note, citations, union_only)
            results.append({
                "repo": row["repo"], "slot": row["slot"], "id": note["id"],
                "instruction_traced": instruction is not None,
                "instruction_rank": None if instruction is None else rank(instruction[0]),
                "focused_ranks": None if instruction is None else [rank(branch) for branch in instruction[1:]],
                "fact_traced": facts[fact] is not None,
                "fact_rank": None if facts[fact] is None else rank(facts[fact][0]),
            })
    common.require(observed and next(events, None) is None, "invalid closed trace length for the frozen cohort")
    return {"schema": "airk.private.vector-rank-census-retained/1", "trace_sha256": args.closed_trace_sha256,
            "cohort_sha256": args.cohort_sha256, "handle_sha256": args.handle_sha256,
            "startup_events": skipped, "observed_requests": observed, "notes": results,
            "scope": "Native vector stages only. A retained trace can precede a failed Search delivery."}


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    for name in ("cohort", "snapshots", "audit-input", "audit-native", "handle"):
        parser.add_argument(f"--{name}", type=Path, required=True)
        parser.add_argument(f"--{name}-sha256", required=True)
    parser.add_argument("--timeout", type=float, default=90)
    parser.add_argument("--output", type=Path, required=True)
    parser.add_argument("--closed-trace", type=Path)
    parser.add_argument("--closed-trace-sha256")
    parser.add_argument("--skip-initial-events", type=int, default=0)
    parser.add_argument("--case", help="replay one frozen repository/scenario pair")
    query_filter = parser.add_mutually_exclusive_group()
    query_filter.add_argument("--facts-only", action="store_true")
    query_filter.add_argument("--instruction-only", action="store_true")
    args = parser.parse_args()
    common.require(not (args.facts_only or args.instruction_only) or bool(args.case),
                   "query filtering requires an explicit case")
    common.require(not args.closed_trace or not args.case, "case selection applies only to live recovery")
    common.require(0 <= args.skip_initial_events <= 16 and (args.closed_trace or not args.skip_initial_events),
                   "an explicit, bounded startup offset requires a closed trace")
    common.require(bool(args.closed_trace) == bool(args.closed_trace_sha256), "closed trace and its digest are both required")
    search.write_new(args.output, recover(args) if args.closed_trace else collect(args))


if __name__ == "__main__":
    try:
        main()
    except (common.HarnessError, OSError, ValueError, KeyError, TypeError) as error:
        print(str(error), file=sys.stderr)
        sys.exit(1)
