# SPDX-License-Identifier: Apache-2.0
"""Join the thirteen Large R8 CAL phase receipts into one raw panel and screen its top-two margins.

Every row is checked against the original Large compiler projection. The rows
carry the same fields as the Compact raw panel. No model runs and nothing is
fitted; the margin screen only counts requests whose two highest legal raw
scores lie within the 0.014 and 0.020 near-tie limits.
"""
import argparse
import ctypes
import json
import math
import os
from pathlib import Path
import re
import runpy
import sys

HERE = Path(__file__).resolve().parent
ROOT = HERE.parents[1]
PHASE = runpy.run_path(str(HERE / "probe_large_cal.py"))
CAMPAIGN = PHASE["CAMPAIGN"]
require = PHASE["require"]
SCHEMA = "airk.private.r8-large-scientific-raw-panel/1"
OUTPUT_NAME = r"r8-large-raw-panel-r[1-9][0-9]*\.json"
NEAR_TIE_LIMITS = (0.014, 0.020)
ROW_FIELDS = ("ordinal", "id", "compiled", "captures", "roster", "remaining_ms", "remaining_input_tokens")


def phase_vectors(report, phase, chosen, model, graph, graph_pin, settled):
    """Raw score vectors of one settled phase receipt, bound to the same image, graph and original rows."""
    require(report["schema"] == PHASE["SCHEMA"] and report["complete"] is True and report["mode"] == "cal"
            and report["direct"] is True and report["phase"] == phase and settled(report["owned"])
            and report["qualification"] is False and report["calibration_fitted"] is False
            and report["graph_receipt"] == graph_pin and report["image"] == graph["image"]
            and report["build"] == graph["build"] and report["runtime_manifest"] == graph["runtime_manifest"]
            and report["host_bytes"] == PHASE["HOST"] and report["seconds"] == PHASE["SECONDS"],
            "Large phase receipt is incomplete or used another image, graph receipt or limit")
    result = report["result"]["result"]
    cal = result["cal"]
    seen = cal["observation"]
    first, last = PHASE["phase_span"](phase)
    require(result["typed_graph_sha256"] == PHASE["TYPED"] and result["model_sha256"] == PHASE["GRAPH"]
            and result["weight_sha256"] == PHASE["WEIGHTS"]
            and cal["native_forwards"] == len(seen["rows"]) == len(chosen) == last - first
            and len(report["kernel_records"]) == PHASE["LAYERS"] * len(chosen) and seen["phase"] == phase
            and seen["original_projection"] == {"path": model["path"], "sha256": model["sha256"]}
            and seen["launch_audit"]["sha256"] == PHASE["AUDIT"]
            and seen["calibration_fitted"] is False and seen["qualification"] is False
            and [item["ordinal"] for item in seen["rows"]] == list(range(first, last)),
            "Large GPU phase does not bind the exact graph, phase or original inputs")
    vectors = []
    for row, candidate in zip(chosen, seen["rows"], strict=True):
        require(row["status"] == "eligible" and candidate["status"] == "raw_scores_only"
                and all(candidate[name] == row[name] for name in ROW_FIELDS)
                and candidate["input_digest"] == row["compiled"]["input_digest"]
                and len(candidate["scores"]) == len(row["roster"]["cards"]) == len(row["compiled"]["option_mask"])
                and all(type(value) in (int, float) and math.isfinite(value) for value in candidate["scores"]),
                "Large GPU row changed the exact original input, card roster or score vector")
        vectors.append(candidate["scores"])
    return vectors


def top_two_margins(rows, masks):
    """Difference of the two highest legal raw scores of every row, with the near-tie counts."""
    entries = []
    for row, mask in zip(rows, masks, strict=True):
        scores = row["scores"]
        legal = [index for index, allowed in enumerate(mask) if allowed is True]
        require(len(mask) == len(scores) and len(legal) >= 2, "top-two margin needs two legal options")
        first, second = sorted(legal, key=lambda index: (-scores[index], index))[:2]
        entries.append({"ordinal": row["ordinal"], "id": row["id"], "winner": first, "runner_up": second,
                        "margin": scores[first] - scores[second]})
    require(entries, "margin screen needs at least one row")
    ordered = sorted(entries, key=lambda entry: (entry["margin"], entry["ordinal"]))
    low, high = NEAR_TIE_LIMITS
    return {"rule": "difference of the two highest legal raw scores under the native option mask",
            "rows": len(entries),
            "at_or_below_0_014": sum(entry["margin"] <= low for entry in entries),
            "at_or_below_0_020": sum(entry["margin"] <= high for entry in entries),
            "near_tie_rows": [entry for entry in ordered if entry["margin"] <= high],
            "minimum": ordered[0]}


def panel(receipts, graph_pin, campaign, read, exact, settled):
    """Assemble the panel from thirteen `(execution.json path, sha256)` pairs and the same-image graph receipt."""
    receipts = [(Path(path), sha) for path, sha in receipts]
    require(len(receipts) == PHASE["PHASES"] and len({path for path, _ in receipts}) == PHASE["PHASES"]
            and len({sha for _, sha in receipts}) == PHASE["PHASES"], "thirteen distinct Large phase receipts required")
    root = campaign / "r8-chronological-full100-r2"
    projection = exact(root / "projection-receipt.json", PHASE["PROJECTION_SHA"])
    compiler = exact(campaign / "r8-chronological-native-full100-r1/native.json", PHASE["COMPILER_SHA"])
    saved, native = read(projection["path"], 16 << 20), read(compiler["path"], 16 << 20)
    model = saved["models"]["large"]
    require(model["sha256"] == PHASE["BATCH"] and Path(model["path"]) == root / "large.json"
            and native["projection_sha256"] == PHASE["PROJECTION_SHA"] and native["passed"] is True
            and native["NN_invocations"] == 0
            and native["inputs"]["large"] == {key: model[key] for key in ("path", "sha256")},
            "foreign R8 projection, Large batch or compiler receipt")
    batch_pin = exact(root / "large.json", PHASE["BATCH"])
    batch = read(batch_pin["path"], 128 << 20)
    target = batch["rows"]
    require(batch["model"] == PHASE["MODEL"] and batch["original_manifest"] == PHASE["ORIGINAL"]
            and batch["launch_audit_sha256"] == PHASE["AUDIT"] and batch["NN_invocations"] == 0
            and [row["ordinal"] for row in target] == list(range(PHASE["REQUESTS"]))
            and all(row["status"] == "eligible" for row in target), "incomplete Large chronology")
    graph = read(graph_pin["path"], 64 << 20)
    measured = graph["result"]["result"]
    require(graph["mode"] == "graph" and graph["complete"] is True and graph.get("direct", False) is True
            and settled(graph["owned"]) and measured["typed_graph_sha256"] == PHASE["TYPED"]
            and measured["model_sha256"] == PHASE["GRAPH"] and measured["weight_sha256"] == PHASE["WEIGHTS"]
            and graph["runtime_manifest"]["sha256"] == PHASE["MANIFEST"], "wrong same-image Large direct graph receipt")
    sources = {}
    for path, sha in receipts:
        name = re.fullmatch(PHASE["OUTPUT_NAME"], path.parent.name)
        require(path.name == "execution.json" and path.parent.parent == campaign and name is not None,
                "Large phase receipt is outside the campaign or misnamed")
        phase = int(name.group(1))
        require(phase < PHASE["PHASES"] and phase not in sources, "duplicate or unknown Large phase receipt")
        sources[phase] = exact(path, sha)
    require(sorted(sources) == list(range(PHASE["PHASES"])), "every original phase 0-12 is required exactly once")
    groups, rows = [], []
    for phase in range(PHASE["PHASES"]):
        first, last = PHASE["phase_span"](phase)
        chosen = target[first:last]
        vectors = phase_vectors(read(sources[phase]["path"], 64 << 20), phase, chosen, model, graph, graph_pin, settled)
        for row, scores in zip(chosen, vectors, strict=True):
            rows.append({"ordinal": row["ordinal"], "id": row["id"], "source_phase": phase,
                         "input_digest": row["compiled"]["input_digest"], "scores": scores})
        groups.append({"source": sources[phase], "phase": phase, "raw_vectors": len(vectors),
                       "reused_original_forward": False})
    require(len(rows) == PHASE["REQUESTS"], "Large panel is not the complete original population")
    return {
        "schema": SCHEMA, "projection": projection, "compiler": compiler, "graph": graph_pin, "input": batch_pin,
        "artifact": {"model_sha256": PHASE["GRAPH"], "weight_sha256": PHASE["WEIGHTS"],
                     "typed_graph_sha256": PHASE["TYPED"], "runtime_manifest_sha256": PHASE["MANIFEST"]},
        "image": graph["image"], "build": graph["build"], "phases": groups, "rows": rows,
        "top_two_margins": top_two_margins(rows, [row["compiled"]["option_mask"] for row in target]),
        "calibration_fitted": False, "qualification": False,
    }


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("--phase-receipt", nargs=2, action="append", required=True, metavar=("EXECUTION_JSON", "SHA256"))
    parser.add_argument("--graph-receipt", type=Path, required=True)
    parser.add_argument("--graph-sha256", required=True)
    parser.add_argument("--powershell", type=Path, required=True)
    parser.add_argument("--powershell-sha256", required=True)
    parser.add_argument("--output", type=Path, required=True)
    args = parser.parse_args()
    shared = runpy.run_path(str(HERE / "probe_compact.py"))
    components = runpy.run_path(str(ROOT / "scripts/cuda-component-execution.py"))
    base = components["base"]()
    parent, _ = base.helpers({"ownership_helper_root": str(base.MAINTAINED_HELPER_ROOT)})
    exact = lambda path, sha: shared["exact_pin"](path, sha, base)
    require(os.name == "nt" and sys.flags.isolated and sys.flags.no_site and sys.dont_write_bytecode
            and parent.worker.windows_sid() == base.SID == components["token_owner"](ctypes.c_void_p(-1)),
            "verified isolated human required")
    guard = {"checked_guard": base.pin(ROOT / "scripts/dev/assert-human-execution.ps1"),
             "powershell": exact(args.powershell, args.powershell_sha256)}
    out = args.output
    require(out.is_absolute() and out.parent == CAMPAIGN and out.parent.resolve(strict=True) == out.parent
            and re.fullmatch(OUTPUT_NAME, out.name) is not None and not out.exists(), "fresh private panel path required")
    value = panel(args.phase_receipt, exact(args.graph_receipt, args.graph_sha256), CAMPAIGN, base.read, exact,
                  parent.settled)
    for pin in (value["projection"], value["compiler"], value["graph"], value["input"],
                *[group["source"] for group in value["phases"]]):
        base.pinned(pin)
    components["guard_many"](guard, [out], False)
    with out.open("x", encoding="utf-8", newline="\n") as stream:
        json.dump(value, stream, allow_nan=False, separators=(",", ":"))
    components["guard_many"](guard, [out], True)
    margins = value["top_two_margins"]
    print(json.dumps({"panel": base.pin(out), "rows": len(value["rows"]),
                      "at_or_below_0_014": margins["at_or_below_0_014"],
                      "at_or_below_0_020": margins["at_or_below_0_020"], "minimum": margins["minimum"]}))


if __name__ == "__main__":
    main()
