# SPDX-License-Identifier: Apache-2.0
"""Bind private native Search windows to a matched 16/40 CUDA Focus replay."""

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
import depth
import search_focus_ab as search
from search_vector_audit import log_rows, trace_identity, unit_span


MARKER = "AIRK_FOCUS_SHORTLIST_INPUTS "
WINDOWS = "airk.private.focus-shortlist-input/1"
BINDING = "airk.private.focus-shortlist-binding/1"
SCORE = "airk.private.focus-shortlist-cuda/1"
MANIFEST = "a3157dd1a6de8f6b5c6ea5348a7d2e53d70c0853f8d66833a12a39ca91f2d183"
GRAPH = "a16fcaa1bf2d67b019b30cda56d76ba23d589c346ef2ae76d75c229ebe866810"


def prepare(cohort, off, raw_log, pins):
    common.require(cohort["schema"] == off["schema"] == search.SCHEMA
                   and cohort["kind"] == "cohort" and off["kind"] == "off"
                   and off["cohort_sha256"] == pins["cohort"]
                   and off["identity"]["search_profile"] == "wide_shortlist"
                   and search.roster(cohort["rows"]) == search.roster(off["rows"]),
                   "wide Focus requires the matching frozen Search-off cohort")
    indexes = off["identity"]["indexes"]
    keys = {}
    for request in cohort["rows"]:
        _, incarnation, sequence, digest = indexes[request["repo"]]["publication_token"].split(":")
        token = {"incarnation": incarnation, "sequence": int(sequence), "digest": digest}
        query_hash = common.sha256_bytes(request["arguments"]["query"].encode("utf-8"))
        key = trace_identity(query_hash, token)
        common.require(key not in keys, "ambiguous query and publication")
        keys[key] = request["key"]

    traces = {}
    outside = 0
    for trace in log_rows(raw_log, MARKER):
        key = keys.get(trace_identity(trace["query_sha256"], trace["publication"]))
        if key is None:
            outside += 1
            continue
        common.require(key not in traces, "duplicate retained Focus input trace")
        traces[key] = trace
    common.require(set(traces) == set(keys.values()), "missing frozen Focus input trace")

    inputs, bindings = [], []
    for request, measured in zip(cohort["rows"], off["rows"], strict=True):
        key = request["key"]
        trace = traces[key]
        common.require(measured["ok"] and measured["served_mode"] == "hybrid"
                       and request["request_sha256"] == measured["request_sha256"]
                       and trace["pair_input_tokens"] == 1024,
                       f"{key}: incompatible Search or Focus pair input")
        candidates, selected = trace["candidates"], trace["selected"]
        common.require(len(candidates) == 40 and [item["ordinal"] for item in candidates] == list(range(40)),
                       f"{key}: expected exactly 40 ordered candidates")
        common.require(len(set(selected)) == len(selected)
                       and all(type(i) is int and 0 <= i < 40 for i in selected)
                       and measured["hit_citations"] == [candidates[i]["citation"] for i in selected],
                       f"{key}: captured selection differs from native reply")
        model_input = {"id": key, "query": request["arguments"]["query"], "candidates": []}
        binding = {"id": key, "repo": request["repo"], "slot": request["slot"],
                   "publication": trace["publication"], "candidates": []}
        for item in candidates:
            text = item["window_text"]
            first, last = item["utf8_start"], item["utf8_end"]
            common.require(type(first) is int and type(last) is int and 0 <= first < last
                           and isinstance(text, str) and len(text.encode("utf-8")) == last - first
                           and search.SHA.fullmatch(item["hit_text_sha256"]) is not None,
                           f"{key}: invalid native scoring window")
            material = {"ordinal": item["ordinal"] + 1, "citation": item["citation"],
                        "hit_text_sha256": item["hit_text_sha256"],
                        "window_text_sha256": common.sha256_bytes(text.encode("utf-8"))}
            item_id = search.digest([key, material])
            model_input["candidates"].append({"id": item_id, "text": text})
            binding["candidates"].append({"id": item_id, **material})
        inputs.append(model_input)
        bindings.append(binding)
    return ({"schema": WINDOWS, "rows": inputs}, {"schema": BINDING, "rows": bindings,
             "input_sha256": pins, "unmatched_trace_events": outside})


def compare(cohort, binding, scores, pins):
    common.require(binding["schema"] == BINDING and scores["schema"] == SCORE
                   and scores["complete"] is True and scores["device_settled"] is True
                   and scores["process_cuda_poisoned"] is False
                   and binding["input_sha256"]["cohort"] == pins["cohort"]
                   and binding["scorer_input_sha256"] == scores["input_sha256"]
                   and scores["model_manifest"] == MANIFEST and scores["graph_sha256"] == GRAPH,
                   "CUDA result and captured exact source windows differ")
    rows = []
    for request, captured, scored in zip(cohort["rows"], binding["rows"], scores["rows"], strict=True):
        key = request["key"]
        common.require(captured["id"] == key == scored["id"], "wide Focus cohort order changed")
        common.require([arm["width"] for arm in scored["arms"]] == [16, 40], "wide Focus widths changed")
        spans = [[span] if (span := unit_span(item["citation"])) else [] for item in captured["candidates"]]

        def covered(ordinals):
            return [note["id"] for note in request["essential"]
                    if depth.ranks([spans[i - 1] for i in ordinals], note)["rule2"] is not None]

        row = {"repo": request["repo"], "slot": request["slot"],
               "unranked_covered": covered(range(1, 11)), "notes": len(request["essential"]), "arms": []}
        for arm in scored["arms"]:
            candidates = arm["candidates"]
            width = arm["width"]
            expected = captured["candidates"][:width]
            common.require(len(candidates) == width and all(
                item["id"] == candidate["id"] and item["ordinal"] == candidate["ordinal"] and
                all(math.isfinite(item[field]) for field in ("relevance", "no_logit", "yes_logit"))
                for item, candidate in zip(candidates, expected, strict=True)), "CUDA score lost a window")
            ranked = sorted(candidates, key=lambda item: (-item["relevance"], item["ordinal"]))
            order = [item["ordinal"] for item in ranked]
            common.require(arm["ranked_ordinals"] == order and arm["wall_us"] > 0,
                           "CUDA rank order or timing is invalid")
            row["arms"].append({"width": width, "covered": covered(order[:10]), "wall_us": arm["wall_us"],
                                "pair_wall_us": arm["pair_wall_us"],
                                "sampled_adapter_peak_used_bytes": arm["sampled_adapter_peak_used_bytes"]})
        short, wide = [arm["candidates"] for arm in scored["arms"]]
        common.require(all(a["input_digest"] == b["input_digest"] for a, b in zip(short, wide[:16], strict=True)),
                       "the same native windows compiled differently across widths")
        row["max_paired_logit_delta"] = max(abs(a[key] - b[key])
                                            for a, b in zip(short, wide[:16], strict=True)
                                            for key in ("no_logit", "yes_logit"))
        common.require(row["max_paired_logit_delta"] <= 1e-3, "shared CUDA pair scores diverged across widths")
        rows.append(row)
    totals = {}
    for repo in common.REPOS:
        group = [row for row in rows if row["repo"] == repo]
        totals[repo] = {"notes": sum(row["notes"] for row in group),
                        "unranked": sum(len(row["unranked_covered"]) for row in group)}
        for index, width in enumerate((16, 40)):
            arms = [row["arms"][index] for row in group]
            totals[repo][str(width)] = {
                "covered": sum(len(arm["covered"]) for arm in arms),
                "wall_us_sum": sum(arm["wall_us"] for arm in arms),
                "adapter_peak_used_bytes": max(arm["sampled_adapter_peak_used_bytes"] for arm in arms),
            }
    return {"schema": "airk.private.focus-shortlist-coverage/1", "input_sha256": pins,
            "scorer_input_sha256": scores["input_sha256"], "model_manifest": MANIFEST,
            "gpu_process_memory": scores["gpu_process_memory"], "repositories": totals, "rows": rows,
            "scope": "Ranked shortlist citation coverage before normal response packing."}


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    commands = parser.add_subparsers(dest="mode", required=True)
    preparation = commands.add_parser("prepare")
    comparison = commands.add_parser("compare")
    for command, names in ((preparation, ("cohort", "off", "trace")), (comparison, ("cohort", "binding", "scores"))):
        for name in names:
            command.add_argument(f"--{name}", type=Path, required=True)
            command.add_argument(f"--{name}-sha256", required=True)
        command.add_argument("--output", type=Path, required=True)
    preparation.add_argument("--binding-output", type=Path, required=True)
    args = parser.parse_args()
    names = ("cohort", "off", "trace") if args.mode == "prepare" else ("cohort", "binding", "scores")
    pins = {name: getattr(args, f"{name}_sha256") for name in names}
    raw = {name: search.pinned(getattr(args, name), pin) for name, pin in pins.items()}
    cohort = json.loads(raw["cohort"])
    if args.mode == "prepare":
        common.require(args.output != args.binding_output and not args.binding_output.exists(),
                       "score input and binding need separate fresh output paths")
        model_input, binding = prepare(cohort, json.loads(raw["off"]), raw["trace"], pins)
        search.write_new(args.output, model_input)
        binding["scorer_input_sha256"] = common.sha256_file(args.output)
        search.write_new(args.binding_output, binding)
    else:
        search.write_new(args.output, compare(cohort, json.loads(raw["binding"]), json.loads(raw["scores"]), pins))


if __name__ == "__main__":
    try:
        main()
    except (common.HarnessError, OSError, ValueError, KeyError, TypeError) as error:
        print(str(error), file=sys.stderr)
        sys.exit(1)
