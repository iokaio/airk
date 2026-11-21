# SPDX-License-Identifier: Apache-2.0
"""Score a panel of compiled rows on an ONNX Runtime provider and measure the drift from named reference scores.

This is a development measurement. It qualifies nothing. See README.md in this folder.
"""
import argparse
from datetime import datetime, timezone
import json
import math
from pathlib import Path
import statistics
import sys
import time

sys.path.insert(0, str(Path(__file__).resolve().parent))
from smoke import Sampler, checked, digest, feeds, gpu_used_mib, require

SCHEMA = "airk.private.webgpu-panel/1"
RAW_PANEL = "airk.private.r8-compact-scientific-raw-panel/1"
PAIRED = ("airk.private.compact-protected-cal-r7-reference-comparison/1",
          "airk.private.r8-compact-scientific-cpu-reuse/1")
# Reference set name -> the key that holds its vector in a paired CPU comparison row.
PAIRED_SETS = {"matching_cpu": "matching_scores", "publisher_cpu": "original_scores"}
BAND_EDGES = (32, 128, 1024, 2048, 8192)
MARGINS = (0.002, 0.014)


def pin(path):
    path = Path(path)
    return {"path": str(path), "bytes": path.stat().st_size, "sha256": digest(path)}


def named(text):
    """Split a NAME=PATH argument."""
    name, separator, path = text.partition("=")
    require(bool(name) and bool(separator) and bool(path), "expected NAME=PATH: " + text)
    return name, Path(path)


def usable(vector):
    return (type(vector) is list and len(vector) >= 2
            and all(type(value) in (int, float) and math.isfinite(value) for value in vector))


def plain(vector):
    """Scores as JSON values. A non-finite score is kept as text so that the receipt can still be written."""
    return [value if math.isfinite(value) else str(value) for value in vector]


def panel_rows(doc, source):
    """Rows of a compiled panel: ordinal, id, token count and the six compiled inputs."""
    rows = []
    for row in doc["rows"]:
        compiled = row["compiled"]
        tokens = checked(compiled, None, source)
        options = len(compiled["option_mask"])
        require(row.get("status", "eligible") == "eligible" and tokens >= 1 and options >= 2
                and all(value is True for value in compiled["option_mask"])
                and 0 <= compiled["decision_position"] < tokens,
                "panel row is not eligible, masks an option or decides outside its tokens: " + str(row["ordinal"]))
        rows.append({"ordinal": row["ordinal"], "id": row["id"], "tokens": tokens, "options": options,
                     "input_digest": compiled.get("input_digest"), "compiled": compiled})
    require(bool(rows) and len({row["ordinal"] for row in rows}) == len(rows),
            "panel has no rows or repeats an ordinal: " + str(source))
    return rows


def vector_rows(doc, source):
    """Score vectors by ordinal, from a retained raw panel or an earlier receipt of this script."""
    require(doc.get("schema") in (RAW_PANEL, SCHEMA), "unexpected reference schema: " + str(source))
    out = {}
    for row in doc["rows"]:
        require(row["ordinal"] not in out and usable(row["scores"]),
                "reference repeats an ordinal or holds unusable scores: " + str(source))
        out[row["ordinal"]] = {"id": row["id"], "input_digest": row.get("input_digest"), "scores": row["scores"]}
    return out


def paired_rows(doc, source):
    """Candidate, matching and publisher vectors by ordinal, from a retained paired CPU comparison."""
    require(doc.get("schema") in PAIRED, "unexpected comparison schema: " + str(source))
    rows = doc["rows"] if "rows" in doc else [row for group in doc["groups"] for row in group["rows"]]
    out = {}
    for row in rows:
        vectors = {key: row["numerical"][key] for key in ("candidate_scores", *PAIRED_SETS.values())}
        require(row["ordinal"] not in out and all(usable(vector) for vector in vectors.values()),
                "comparison repeats an ordinal or holds unusable scores: " + str(source))
        out[row["ordinal"]] = {"id": row["id"], **vectors}
    return out


def bound(rows, vectors, source):
    """Reference scores by ordinal, each checked against the panel row it belongs to.

    The id and the option count must match, and so must the input digest where the reference records one.
    """
    known = {row["ordinal"]: row for row in rows}
    out = {}
    for ordinal, item in vectors.items():
        row = known.get(ordinal)
        require(row is not None and row["id"] == item["id"] and len(item["scores"]) == row["options"]
                and item.get("input_digest") in (None, row["input_digest"]),
                "reference row does not belong to the panel row: " + str(ordinal) + " in " + str(source))
        out[ordinal] = item["scores"]
    return out


def paired_sets(rows, primary, paired, source):
    """The matching and publisher CPU sets of one paired comparison.

    A comparison row records no input digest. It is tied to the panel through its candidate scores, which
    must equal the primary reference's scores for the same ordinal.
    """
    require(all(primary.get(ordinal) == item["candidate_scores"] for ordinal, item in paired.items()),
            "comparison candidate scores differ from the primary reference: " + str(source))
    return {name: bound(rows, {ordinal: {"id": item["id"], "scores": item[key]} for ordinal, item in paired.items()},
                        source) for name, key in PAIRED_SETS.items()}


def top_two(scores):
    """Index of the top score, the lowest index on a tie, and its margin over the second score."""
    order = sorted(range(len(scores)), key=lambda index: (-scores[index], index))
    return order[0], scores[order[0]] - scores[order[1]]


def compared(scores, reference):
    """Drift of one scored vector from one reference vector."""
    require(len(scores) == len(reference), "scored and reference vectors differ in length")
    winner, _ = top_two(scores)
    reference_winner, reference_margin = top_two(reference)
    worst = max(abs(a - b) for a, b in zip(scores, reference))
    return {"reference": reference, "max_abs_difference": worst, "reference_winner": reference_winner,
            "same_winner": winner == reference_winner, "reference_margin": reference_margin,
            "margin_below_twice_difference": reference_margin < 2 * worst}


def nearest_rank(values, percent):
    """Nearest-rank percentile: the smallest value with at least that share of the values at or below it."""
    ordered = sorted(values)
    require(bool(ordered) and type(percent) is int and 0 < percent <= 100,
            "a nearest rank needs values and a whole percent")
    return ordered[-(-percent * len(ordered) // 100) - 1]


def spread(values):
    return {"rows": len(values), "max": max(values), "median": statistics.median(values),
            "p95_nearest_rank": nearest_rank(values, 95)}


def band(tokens, edges):
    """Label of a token band. Each edge is the inclusive upper bound of one band, in increasing order."""
    low = 1
    for edge in edges:
        if tokens <= edge:
            return str(low) + "-" + str(edge)
        low = edge + 1
    return str(low) + "+"


def summary(rows, name, edges, margins):
    """Drift statistics for one reference set, over the scored rows that have it."""
    have = [row for row in rows if name in row["comparisons"]]
    require(bool(have), "no scored row has the reference " + name)

    def block(group):
        items = [row["comparisons"][name] for row in group]
        return {"difference": spread([item["max_abs_difference"] for item in items]),
                "winner_changes": sum(not item["same_winner"] for item in items),
                "margin_below_twice_difference": sum(item["margin_below_twice_difference"] for item in items)}

    def detail(row):
        item = row["comparisons"][name]
        return {"ordinal": row["ordinal"], "id": row["id"], "tokens": row["tokens"], "scores": row["scores"],
                "reference": item["reference"], "max_abs_difference": item["max_abs_difference"],
                "margin": row["margin"], "reference_margin": item["reference_margin"]}

    bands = {}
    for row in sorted(have, key=lambda row: row["tokens"]):
        bands.setdefault(band(row["tokens"], edges), []).append(row)
    out = block(have)
    out["winner_change_rows"] = [detail(row) for row in have if not row["comparisons"][name]["same_winner"]]
    out["margin_below_twice_difference_rows"] = [
        detail(row) for row in have if row["comparisons"][name]["margin_below_twice_difference"]]
    out["bands"] = {label: block(group) for label, group in bands.items()}
    # A proxy only: no fitted threshold exists, so this counts raw winner margins against a possible guard band.
    out["margin_at_least"] = {
        str(margin): {"reference": sum(row["comparisons"][name]["reference_margin"] >= margin for row in have),
                      "scored": sum(row["margin"] >= margin for row in have)} for margin in margins}
    return out


def open_session(ort, args, options, session_config, receipt):
    """Create the session the way smoke.py does: the CPU provider, or the WebGPU plugin on its one device."""
    session_options = ort.SessionOptions()
    for key, value in session_config.items():
        session_options.add_session_config_entry(key, value)
    if args.intra_threads:
        session_options.intra_op_num_threads = args.intra_threads
        receipt["intra_threads"] = args.intra_threads
    if args.provider == "cpu":
        require(not options, "provider options apply to WebGPU only")
        return ort.InferenceSession(str(args.model), sess_options=session_options, providers=["CPUExecutionProvider"])
    import onnxruntime_ep_webgpu as plugin
    ort.register_execution_provider_library("webgpu", plugin.get_library_path())
    devices = [d for d in ort.get_ep_devices() if d.ep_name == plugin.get_ep_name()]
    require(len(devices) == 1, "expected exactly one WebGPU device, found " + str(len(devices)))
    hardware = devices[0].device
    receipt["plugin"] = {"version": dict(devices[0].ep_metadata).get("version"),
                         "library": pin(plugin.get_library_path()),
                         "adapter": {"vendor": hardware.vendor, "vendor_id": hardware.vendor_id,
                                     "device_id": hardware.device_id, **dict(hardware.metadata)}}
    session_options.add_provider_for_devices(devices, options)
    return ort.InferenceSession(str(args.model), sess_options=session_options)


def main():
    parser = argparse.ArgumentParser(description=__doc__.splitlines()[0])
    parser.add_argument("--model", required=True, type=Path)
    parser.add_argument("--weights", action="append", default=[], type=Path,
                        help="a weight file the graph reads, recorded with its digest")
    parser.add_argument("--panel", required=True, type=Path,
                        help="compiled rows: each has an ordinal, an id and the six compiled inputs")
    parser.add_argument("--reference", action="append", required=True,
                        help="NAME=PATH of a raw panel or an earlier receipt of this script; the first is primary")
    parser.add_argument("--comparison", action="append", default=[], type=Path,
                        help="a retained paired CPU comparison: adds the matching_cpu and publisher_cpu sets")
    parser.add_argument("--first", type=int, default=0, help="position of the first panel row to score")
    parser.add_argument("--count", type=int, help="number of panel rows to score; all remaining by default")
    parser.add_argument("--band-edge", action="append", default=[], type=int,
                        help="inclusive upper token bound of a band, in increasing order")
    parser.add_argument("--margin", action="append", default=[], type=float,
                        help="a winner margin to count rows against, as a guard-band proxy")
    parser.add_argument("--provider", choices=("webgpu", "cpu"), default="webgpu")
    parser.add_argument("--option", action="append", default=[], help="provider option as key=value")
    parser.add_argument("--session-config", action="append", default=[], help="session config entry as key=value")
    parser.add_argument("--intra-threads", type=int, help="intra-op thread count, for CPU runs")
    parser.add_argument("--warm", type=int, default=0, help="warm forwards per row after the first one")
    parser.add_argument("--output", required=True, type=Path)
    args = parser.parse_args()
    begun = time.perf_counter()
    started_utc = datetime.now(timezone.utc).isoformat()
    require(args.output.is_dir() and not any(args.output.iterdir()), "a fresh empty output directory is required")
    edges = args.band_edge or list(BAND_EDGES)
    margins = args.margin or list(MARGINS)
    require(all(a < b for a, b in zip(edges, edges[1:])) and edges[0] >= 1 and args.warm >= 0 and args.first >= 0,
            "band edges must increase from at least 1, and --warm and --first cannot be negative")

    rows = panel_rows(json.loads(args.panel.read_bytes()), args.panel)
    references = {}
    for name, path in map(named, args.reference):
        require(name not in references and name not in PAIRED_SETS, "repeated or reserved reference name: " + name)
        references[name] = {"sources": [pin(path)],
                            "vectors": bound(rows, vector_rows(json.loads(path.read_bytes()), path), path)}
    primary = next(iter(references))
    for path in args.comparison:
        sets = paired_sets(rows, references[primary]["vectors"], paired_rows(json.loads(path.read_bytes()), path), path)
        for name, vectors in sets.items():
            entry = references.setdefault(name, {"sources": [], "vectors": {}})
            require(not set(entry["vectors"]) & set(vectors), "two comparisons cover the same ordinal: " + str(path))
            entry["sources"].append(pin(path))
            entry["vectors"].update(vectors)
    chosen = rows[args.first:None if args.count is None else args.first + args.count]
    require(bool(chosen), "no panel row selected")

    import numpy as np
    import onnxruntime as ort

    options = dict(item.split("=", 1) for item in args.option)
    session_config = dict(item.split("=", 1) for item in args.session_config)
    receipt = {"schema": SCHEMA, "qualification": False, "provider": args.provider, "provider_options": options,
               "session_config": session_config, "onnxruntime": ort.__version__, "model": pin(args.model),
               "weights": [pin(path) for path in args.weights],
               "panel": pin(args.panel) | {"rows": len(rows)},
               "selected": {"first": args.first, "rows": len(chosen)}, "primary_reference": primary,
               "references": {name: {"sources": item["sources"], "rows": len(item["vectors"])}
                              for name, item in references.items()},
               "band_edges": edges, "margins": margins, "warm": args.warm, "started_utc": started_utc, "rows": []}
    gpu = args.provider == "webgpu"
    baseline = gpu_used_mib() if gpu else None
    sampler = Sampler() if gpu else None
    if sampler:
        sampler.start()
    opened = time.perf_counter()
    session = open_session(ort, args, options, session_config, receipt)
    load_seconds = round(time.perf_counter() - opened, 3)
    receipt["session_providers"] = session.get_providers()
    after_load = gpu_used_mib() if gpu else None
    for row in chosen:
        inputs = feeds(session, row["compiled"], np)
        times, outputs = [], []
        for _ in range(1 + args.warm):
            start = time.perf_counter()
            output = session.run(["scores"], inputs)[0]
            times.append(round(time.perf_counter() - start, 4))
            outputs.append([float(value) for value in np.asarray(output, dtype=np.float64).reshape(-1)])
        scores = outputs[-1]
        require(len(scores) == row["options"], "the graph returned an unexpected number of scores")
        finite = all(math.isfinite(value) for vector in outputs for value in vector)
        entry = {"ordinal": row["ordinal"], "id": row["id"], "tokens": row["tokens"], "options": row["options"],
                 "input_digest": row["input_digest"], "first_seconds": times[0], "warm_seconds": times[1:],
                 "scores": plain(scores), "finite": finite, "repeat_max_abs_difference": None,
                 "winner": None, "margin": None, "comparisons": {}}
        if finite:
            entry["winner"], entry["margin"] = top_two(scores)
            if args.warm:
                entry["repeat_max_abs_difference"] = max(abs(a - b) for vector in outputs[1:]
                                                         for a, b in zip(vector, outputs[0]))
            entry["comparisons"] = {name: compared(scores, item["vectors"][row["ordinal"]])
                                    for name, item in references.items() if row["ordinal"] in item["vectors"]}
        if gpu:
            entry["adapter_memory_mib_after"] = gpu_used_mib()
        receipt["rows"].append(entry)
        print(json.dumps({"ordinal": entry["ordinal"], "tokens": entry["tokens"], "seconds": times, "finite": finite,
                          "difference": {name: item["max_abs_difference"] for name, item in entry["comparisons"].items()},
                          "same_winner": {name: item["same_winner"] for name, item in entry["comparisons"].items()}}),
              file=sys.stderr, flush=True)
    session_seconds = round(time.perf_counter() - opened, 3)
    receipt["adapter_memory_mib"] = None
    if sampler:
        receipt["adapter_memory_mib"] = {"baseline": baseline, "after_load": after_load, "peak": sampler.stop(),
                                         "samples": sampler.samples}
    scored = receipt["rows"]
    receipt["nonfinite_ordinals"] = [row["ordinal"] for row in scored if not row["finite"]]
    receipt["summary"] = {name: summary(scored, name, edges, margins) for name in references
                          if any(name in row["comparisons"] for row in scored)}
    receipt["timing"] = {
        "load_seconds": load_seconds, "forwards": len(scored) * (1 + args.warm),
        "forward_seconds": round(sum(row["first_seconds"] + sum(row["warm_seconds"]) for row in scored), 3),
        "first_seconds": spread([row["first_seconds"] for row in scored]),
        # From just before the session is created to the last row: load, forwards and per-row overhead.
        "session_seconds": session_seconds,
        # The whole run up to this point: also reading and checking the inputs and hashing the files.
        "total_seconds": round(time.perf_counter() - begun, 3)}
    receipt["finished_utc"] = datetime.now(timezone.utc).isoformat()
    receipt["complete"] = True
    receipt["finite"] = not receipt["nonfinite_ordinals"]
    with (args.output / "panel.json").open("x", encoding="utf-8") as file:
        json.dump(receipt, file, indent=1, allow_nan=False)
    print(json.dumps({
        "provider": args.provider, "rows": len(scored), "finite": receipt["finite"], "timing": receipt["timing"],
        "memory_mib": receipt["adapter_memory_mib"],
        "summary": {name: {key: item[key] for key in ("difference", "winner_changes", "margin_below_twice_difference",
                                                      "margin_at_least")}
                    for name, item in receipt["summary"].items()}}))


if __name__ == "__main__":
    main()
