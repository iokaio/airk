# SPDX-License-Identifier: Apache-2.0
"""Assemble the original CUDA vectors linked to one complete R8 Compact population."""

import argparse
import json
import math
from pathlib import Path
import sys

sys.path.insert(0, str(Path(__file__).resolve().parent))
from r7_compare import native_vectors
from r7_inputs import CAMPAIGN, GRAPH, IDENTITY, canonical, read, reference, require
from r8_reuse import R8, R8_NATIVE

REUSE = (
    CAMPAIGN / "r8-compact-first56-reuse-r1.json",
    "ef82202dbff07a1f2d8e20b638f343f766e90a8566322a9384feade0a7472811",
)


def panel(shas):
    require(len(shas) == 6 and len(set(shas)) == 6, "six distinct R8 phase receipts required")
    reuse_pin = reference(*REUSE)
    reuse = read(reuse_pin)
    projection = canonical(reuse["r8_projection"], R8)
    compiler = canonical(reuse["r8_compiler"], R8_NATIVE)
    graph_pin = canonical(reuse["graph"], GRAPH)
    current = read(projection)
    graph = read(graph_pin)
    current_model = current["models"]["compact"]
    target = read(current_model)["rows"]
    require([row["ordinal"] for row in target] == list(range(100))
            and [item["first_ordinal"] for item in reuse["phases"]] == list(range(0, 56, 8))
            and all(item["matched_rows"] == 8 for item in reuse["phases"]), "incomplete Compact chronology")
    old_model = read(reuse["r7_projection"])["models"]["compact"]
    original = read(old_model)["rows"]
    provenance = IDENTITY | {"launch_audit_sha256": current["launch_audit"]["sha256"]}
    groups, rows = [], []
    for phase in range(13):
        start = phase * 8
        chosen = target[start:start + 8]
        if phase < 7:
            source = reuse["phases"][phase]["source_receipt"]
            input_model = old_model
            producer_rows = original[start:start + 8]
        else:
            source = reference(CAMPAIGN / f"compact-cutlass-cal-r8-full100-phase{phase:02}-r1/execution.json",
                               shas[phase - 7])
            input_model = current_model
            producer_rows = chosen
        vectors = native_vectors(read(source), producer_rows, phase, provenance, graph, input_model,
                                 phase_size=len(chosen))
        require(all(all(type(value) in (int, float) and math.isfinite(value) for value in scores)
                    for scores in vectors), "incomplete or nonfinite native Compact vector")
        for current_row, producer_row, scores in zip(chosen, producer_rows, vectors, strict=True):
            require(current_row["ordinal"] == producer_row["ordinal"] and current_row["id"] == producer_row["id"],
                    "original CUDA score order differs from R8")
            rows.append({"ordinal": current_row["ordinal"], "id": current_row["id"],
                         "source_phase": phase, "input_digest": current_row["compiled"]["input_digest"],
                         "scores": scores})
        groups.append({"source": source, "phase": phase, "raw_vectors": len(vectors),
                       "reused_original_forward": phase < 7})
    return {
        "schema": "airk.private.r8-compact-scientific-raw-panel/1", "projection": projection, "compiler": compiler,
        "graph": graph_pin, "reuse": reuse_pin, "artifact": IDENTITY, "phases": groups, "rows": rows,
        "calibration_fitted": False,
    }


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("--phase-sha256", nargs=6, required=True, metavar="SHA256")
    args = parser.parse_args()
    require(sys.platform == "win32" and sys.flags.isolated and sys.flags.no_site and sys.dont_write_bytecode,
            "use checked Windows Python -I -B -S")
    print(json.dumps(panel(args.phase_sha256), allow_nan=False, separators=(",", ":")))


if __name__ == "__main__":
    main()
