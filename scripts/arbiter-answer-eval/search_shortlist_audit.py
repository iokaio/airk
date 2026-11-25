# SPDX-License-Identifier: Apache-2.0
"""Match a private Search shortlist trace to the frozen Focus off/on pair."""

from __future__ import annotations

import argparse
from collections import Counter
import json
from pathlib import Path
import sys

HERE = Path(__file__).resolve().parent
if str(HERE) not in sys.path:
    sys.path.insert(0, str(HERE))

import common
import depth
import search_focus_ab as search
from search_vector_audit import log_rows, trace_identity, unit_span


SCHEMA = "airk.private.search-shortlist-audit/1"
MARKER = "AIRK_SEARCH_SHORTLIST "


def trace_pairs(raw, requests, indexes):
    expected = {}
    for request in requests:
        _, incarnation, sequence, digest = indexes[request["repo"]]["publication_token"].split(":")
        key = trace_identity(common.sha256_bytes(request["arguments"]["query"].encode("utf-8")),
                             {"incarnation": incarnation, "sequence": int(sequence), "digest": digest})
        common.require(key not in expected, "ambiguous shortlist query and publication")
        expected[key] = (request["repo"], request["slot"])

    traces = {}
    for row in log_rows(raw, MARKER):
        key = trace_identity(row["query_sha256"], row["publication"])
        common.require(key in expected, "unexpected shortlist query or publication")
        traces.setdefault(expected[key], []).append(row)
    common.require(set(traces) == set(expected.values()), "shortlist trace roster differs")

    for key, pair in traces.items():
        common.require(len(pair) == 2 and pair[0] == pair[1],
                       f"{key}: expected identical pre-Focus shortlists and selections")
    return {key: pair[0] for key, pair in traces.items()}


def validated_projection(trace, request, off, focus):
    candidates, selected = trace["candidates"], trace["selected"]
    common.require(isinstance(candidates, list) and all(isinstance(item, list) and len(item) == 2
                   and isinstance(item[0], dict) and type(item[1]) is bool for item in candidates),
                   "invalid shortlist candidate")
    common.require(isinstance(selected, list) and len(selected) <= request["limit"]
                   and all(type(i) is int and 0 <= i < len(candidates) for i in selected)
                   and len(set(selected)) == len(selected), "invalid shortlist projection")
    citations = [candidate[0] for candidate in candidates]
    common.require(off["hit_citations"] == [citations[i] for i in selected],
                   "Focus-off citations differ from the native selection")

    off_hits, on_hits = off["hit_sha256s"], focus["hit_sha256s"]
    common.require(focus["focus"]["state"] == "cuda_applied"
                   and focus["focus"]["original_hit_sha256s"] == off_hits
                   and len(set(off_hits)) == len(off_hits) == len(selected)
                   and sorted(off_hits) == sorted(on_hits), "ordinary Focus changed shortlist membership")
    selected_index = dict(zip(off_hits, selected, strict=True))
    on_indexes = [selected_index[hit] for hit in on_hits]
    common.require(focus["hit_citations"] == [citations[i] for i in on_indexes],
                   "Focus-on citations differ from the native permutation")
    return candidates, selected, on_indexes


def note_ranks(request, candidates, selected, on_indexes, off, focus):
    spans = [([span] if (span := unit_span(citation)) is not None else []) for citation, _ in candidates]
    semantic_spans = [span if semantic else [] for (_, semantic), span in zip(candidates, spans, strict=True)]
    notes = []
    for note, a, b in zip(request["essential"], off["notes"], focus["notes"], strict=True):
        common.require(note["id"] == a["id"] == b["id"], "shortlist essential note roster differs")
        shortlist_rank = depth.ranks(spans, note)["rule2"]
        ranks = [depth.ranks([spans[i] for i in indexes], note)["rule2"] for indexes in (selected, on_indexes)]
        common.require(ranks == [a["rule2"], b["rule2"]],
                       "projected citations disagree with measured Rule 2 coverage")
        notes.append({"id": note["id"], "shortlist_rank": shortlist_rank,
                      "semantic_shortlist_rank": depth.ranks(semantic_spans, note)["rule2"],
                      "off_rank": ranks[0], "focus_rank": ranks[1],
                      "from_below_cutoff": shortlist_rank is not None and shortlist_rank > request["limit"]
                      and ranks[0] is not None})
    return notes


def summarize(rows):
    counts = Counter()
    for row in rows:
        counts["requests"] += 1
        counts["candidates"] += row["candidates"]
        counts["semantic_candidates"] += row["semantic_candidates"]
        for note in row["notes"]:
            off, focus = note["off_rank"], note["focus_rank"]
            counts["essentials"] += 1
            counts["shortlist_covered"] += note["shortlist_rank"] is not None
            counts["semantic_shortlist_covered"] += note["semantic_shortlist_rank"] is not None
            counts["off_covered"] += off is not None
            counts["focus_covered"] += focus is not None
            counts["returned_from_below_cutoff"] += note["from_below_cutoff"]
            counts["focus_improved_rank"] += off is not None and focus is not None and focus < off
            counts["focus_worsened_rank"] += off is not None and focus is not None and focus > off
    return dict(counts)


def audit(cohort, off, focus, raw, pins):
    requests = cohort["rows"]
    common.require(cohort["schema"] == search.SCHEMA and cohort["kind"] == "cohort"
                   and cohort["series"] == search.SERIES and search.roster(requests) == search.roster(off["rows"])
                   and pins["cohort"] == off["cohort_sha256"], "frozen shortlist cohort differs")
    comparison = search.compare(off, focus)
    common.require(comparison["complete"], "expected a complete ordinary Focus pair")
    traces = trace_pairs(raw, requests, off["identity"]["indexes"])

    rows = []
    for request, left, right in zip(requests, off["rows"], focus["rows"], strict=True):
        key = request["repo"], request["slot"]
        common.require(all(request[k] == left[k] for k in
                           ("key", "request_sha256", "limit", "max_bytes", "response_mode")),
                       f"{key}: shortlist request identity differs")
        candidates, selected, on_indexes = validated_projection(traces[key], request, left, right)
        notes = note_ranks(request, candidates, selected, on_indexes, left, right)
        rows.append({"repo": key[0], "slot": key[1], "candidates": len(candidates),
                     "semantic_candidates": sum(candidate[1] for candidate in candidates),
                     "off_selected_shortlist_indexes": selected, "focus_selected_shortlist_indexes": on_indexes,
                     "notes": notes})

    return {"schema": SCHEMA, "input_sha256": pins, "identity": off["identity"],
            "counts": {"all": summarize(rows), **{repo: summarize(r for r in rows if r["repo"] == repo)
                                                  for repo in common.REPOS}},
            "rows": rows,
            "scope": "Rule 2 source line coverage of the recorded pre-Focus shortlist and its final projection. "
                     "Indexes within the shortlist are zero-based; measured note ranks are one-based."}


def main(argv=None):
    parser = argparse.ArgumentParser(description=__doc__)
    for name in ("cohort", "off", "focus", "trace"):
        parser.add_argument(f"--{name}", type=Path, required=True)
        parser.add_argument(f"--{name}-sha256", required=True)
    parser.add_argument("--output", type=Path, required=True)
    args = parser.parse_args(argv)
    pins = {name: getattr(args, f"{name}_sha256") for name in ("cohort", "off", "focus", "trace")}
    inputs = {name: search.pinned(getattr(args, name), pin) for name, pin in pins.items()}
    common.require(args.output.parent.is_dir() and not args.output.exists(),
                   "use a fresh output in an existing private directory")
    result = audit(*(json.loads(inputs[name]) for name in ("cohort", "off", "focus")), inputs["trace"], pins)
    search.write_new(args.output, result)
    return 0


if __name__ == "__main__":
    try:
        sys.exit(main())
    except (common.HarnessError, OSError, KeyError, ValueError, TypeError) as error:
        print(str(error), file=sys.stderr)
        sys.exit(1)
