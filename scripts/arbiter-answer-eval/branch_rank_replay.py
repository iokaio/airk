# SPDX-License-Identifier: Apache-2.0
"""Replay the frozen development queries for ranked-lexical and fusion coverage."""

from __future__ import annotations

import argparse
from pathlib import Path
import sys

HERE = Path(__file__).resolve().parent
if str(HERE) not in sys.path:
    sys.path.insert(0, str(HERE))

import common
import search_focus_ab as search
import search_vector_audit as audit
import vector_rank_replay as vector


FOCUSED = "AIRK_SEARCH_FOCUSED_CENSUS "
LEXICAL = "AIRK_SEARCH_LEXICAL_CENSUS "
FUSION = "AIRK_SEARCH_FUSION_CENSUS "


def observations(raw, marker, query, publication):
    _, incarnation, sequence, digest = publication.split(":")
    token = {"incarnation": incarnation, "sequence": int(sequence), "digest": digest}
    query_sha256 = common.sha256_bytes(query.encode("utf-8"))
    values = []
    for row in audit.log_rows(raw, marker):
        common.require(row["publication"] == token and row["query_sha256"] == query_sha256,
                       "branch trace belongs to a different query or publication")
        values.append(row["observations"])
    return values


def focused_queries(query, plan):
    raw = query.encode("utf-8")
    ranges = plan["byte_ranges"]
    embedded = plan["embedded"]
    common.require(isinstance(ranges, list) and len(ranges) <= 3
                   and type(embedded) is int and embedded in (0, len(ranges)),
                   "invalid native focused-query roster")
    pieces = []
    for start, end in ranges:
        common.require(type(start) is int and type(end) is int and 0 <= start < end <= len(raw),
                       "focused byte range outside query")
        pieces.append(raw[start:end].decode("utf-8"))
    return pieces, embedded


def request(args, handle, row, query, identity, service_log):
    raw, delivered = vector.native_request(args, handle, row, query, identity, service_log)
    publication = identity["publication_token"]
    focused, lexical, fusion = (
        observations(raw, marker, query, publication) for marker in (FOCUSED, LEXICAL, FUSION)
    )
    common.require(len(focused) == len(fusion) == 1, "missing or repeated focused or fusion trace")
    by_branch = {item["branch"]: item for item in lexical}
    common.require(len(by_branch) == len(lexical) and set(by_branch) <= {"keyword", "identifier"},
                   "invalid lexical branch roster")
    return focused_queries(query, focused[0]), by_branch, fusion[0], delivered


def rank_query(response, note, inventory, union_only):
    _, lexical, fusion, delivered = response
    ranked_lexical = {
        branch: {
            "rank": audit.first_covering_rank(
                ((rank, audit.unit_span(citation)) for rank, citation in item["ranks"]),
                note, union_only=union_only,
            ),
            "admitted": item["admitted"],
            "dropped_groups": item["dropped_groups"],
        }
        for branch, item in lexical.items()
    }
    note_paths = {audit.note_path(location) for location in audit.note_locations(note)}
    ranked_fusion = []
    for item in fusion:
        if "citation" in item:
            span = audit.unit_span(item["citation"])
        elif item["path"] in note_paths:
            key = (item["path"], item["ordinal"])
            common.require(key in inventory, "fused vector occurrence is absent from the authenticated note inventory")
            span = inventory[key]
        else:
            continue
        ranked_fusion.append((item["rank"], span))
    return {
        "lexical": ranked_lexical,
        "fused_candidate_rank": audit.first_covering_rank(ranked_fusion, note, union_only=union_only),
        "delivered_rule2_rank": search.depth.ranks(delivered, note)["rule2"],
    }


def collect(args):
    cohort, snapshots, paths = vector.load_cohort(args)
    handle, identity, before, stage = vector.open_fixture(args, snapshots, branch_only=True)
    log = handle.path.parent / "service.log"
    results = []
    rows = [row for row in cohort["rows"]
            if (args.repo is None or row["repo"] == args.repo)
            and (args.case is None or f"{row['repo']}/{row['slot']}" == args.case)]
    if args.omit_case:
        common.require(any(f"{row['repo']}/{row['slot']}" == args.omit_case for row in rows),
                       "omitted branch case is outside the selected cohort")
        rows = [row for row in rows if f"{row['repo']}/{row['slot']}" != args.omit_case]
    common.require(bool(rows), "branch replay selector matched no frozen development scenarios")
    for row in rows:
        repo = row["repo"]
        cache = {}

        def observe(query):
            if query not in cache:
                cache[query] = request(args, handle, row, query, identity["indexes"][repo], log)
            return cache[query]

        instruction = observe(row["arguments"]["query"])
        parts, embedded = instruction[0]
        focused = [observe(part) for part in parts]
        for note in row["essential"]:
            inventory, union_only = vector.note_inventory(note, audit.REPOSITORIES[repo], paths)
            fact = note["fact"]
            common.require(isinstance(fact, str) and bool(fact.strip()), "census note has no fact sentence")
            results.append({
                "repo": repo, "slot": row["slot"], "id": note["id"],
                "instruction": rank_query(instruction, note, inventory, union_only),
                "focused": [rank_query(response, note, inventory, union_only) for response in focused],
                "embedded_auxiliaries": embedded,
                "fact": rank_query(observe(fact), note, inventory, union_only),
            })
    vector.check_settled(args, handle, before, stage)
    return {"schema": "airk.private.branch-rank-census/1", "identity": identity, "notes": results,
            "repo": args.repo, "case": args.case, "omitted_case": args.omit_case,
            "collector_sha256": common.sha256_file(Path(__file__)),
            "shared_replay_sha256": common.sha256_file(Path(vector.__file__)),
            "input_sha256": {name: getattr(args, f"{name}_sha256") for name in
                             ("cohort", "snapshots", "audit_input", "audit_native", "handle")},
            "scope": "Lexical ranks cover scored groups before the native cut. An empty lexical roster means the "
                     "query did not enter a ranked keyword/identifier branch. Fusion is before evidence packing. "
                     "Focused standalone requests use the exact native task fragments. Labeled fact ranks are diagnostic."}


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    vector.add_fixture_inputs(parser)
    selector = parser.add_mutually_exclusive_group()
    selector.add_argument("--repo", choices=sorted(audit.REPOSITORIES))
    selector.add_argument("--case", help="replay one frozen repository/scenario pair")
    parser.add_argument("--omit-case", help="omit one already measured repository/scenario pair")
    args = parser.parse_args()
    search.write_new(args.output, collect(args))


if __name__ == "__main__":
    try:
        main()
    except (common.HarnessError, OSError, ValueError, KeyError, TypeError) as error:
        print(str(error), file=sys.stderr)
        sys.exit(1)
