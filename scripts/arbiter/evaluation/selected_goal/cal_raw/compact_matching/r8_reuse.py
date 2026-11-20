# SPDX-License-Identifier: Apache-2.0
"""Link identical R7 scientific CUDA outputs to R8 inputs without rerunning forwards."""

import argparse
import json
import math
from pathlib import Path
import re
import sys

sys.path.insert(0, str(Path(__file__).resolve().parent))
from r7_compare import native_vectors
from r7_inputs import CAMPAIGN, COMPILER, GRAPH, PROJECTION, canonical, read, reference, require, selected

R8 = (
    CAMPAIGN / "r8-chronological-full100-r2/projection-receipt.json",
    "e1fd8a2b0f2f064a0c32ef361f8bd085c58120f7eaf5ab1758a80f9bea2ea845",
)
R8_NATIVE = (
    CAMPAIGN / "r8-chronological-native-full100-r1/native.json",
    "e990a1b8346b5cd0fcd68ffd97b0c1eee776094641bd41ef864af547971690cb",
)
GPU_SHAS = (
    "3f13709ece160ae0df1f02356ce15c427693c3e29c5deaaecd44c322b48020c4",
    "d202778a912d0fb63a2f89990131731c6e7d910d60daa1aa03452eb215f24416",
    "44f4ffe70a8df0e871f0473bac42e2dbd8fc694a74fc2ab106f19f2e4b3ebfa6",
    "a22c4d3bff1ea7cf8e5409a774859245f195d98e6ec711d8354de19ca715304d",
    "f5e5b4c28bffbe8f078ad799a4f6fc5d19c48f507aa56c690aa3ff9c554f82e2",
    "eabebbdecd2b08d0df39c3aaeb81266d58830b72399c4b5d5dc2ca89ff07e148",
    "dc8be457da2f91e1ed52466e9ec3be4a27c2acf5d7fa01ce9caabe1c8d9da58a",
)


def resolved_roster(request):
    """Resolve authority-scoped record references back to their actual source units."""
    roster = request["roster"]
    records = {record["id"]: record for record in request["records"]}
    require(len(records) == len(request["records"]), "ambiguous native roster records")

    def evidence(unit):
        pointer = re.fullmatch(r"/hits/(0|[1-9][0-9]*)", unit["result_pointer"])
        require(pointer is not None, "invalid native card evidence pointer")
        record = records[unit["record"]]
        hit = record["canonical_result"]["hits"][int(pointer[1])]
        return {
            **{key: value for key, value in unit.items() if key != "record"},
            "operation": record["operation"], "role": record["role"],
            "arguments": record["arguments"], "hit": hit,
        }

    basis = roster["basis"]
    cards = [
        {**{key: value for key, value in card.items() if key not in ("id", "assessments")},
         "assessments": [item | {"evidence": evidence(item["evidence"])} for item in card["assessments"]]}
        for card in roster["cards"]
    ]
    return {
        "schema": roster["schema"], "policy": roster["policy"], "cards": cards,
        "basis": basis | {"source_units_presented": [evidence(unit) for unit in basis["source_units_presented"]]},
    }


def verify_inputs(old, target):
    require(old["ordinal"] == target["ordinal"] and old["id"] == target["id"]
            and old["status"] == target["status"] == "eligible" and old["compiled"] == target["compiled"],
            "R8 original scoring tensors, slot positions or ordinal differ")
    requests = []
    for row in (old, target):
        pin, = row["captures"]
        record = read(pin)
        request = record["request"]
        require(request["roster"] == row["roster"] and
                record["Context_NN_dispatch"] is False, "projection differs from original native admission")
        requests.append(request)
    a, b = requests
    require(a["question"] == b["question"] and resolved_roster(a) == resolved_roster(b),
            "R8 ordered card semantics or source evidence differs")


def verify_reuse(current, native):
    r8_projection = canonical(current, R8)
    r8_compiler = canonical(native, R8_NATIVE)
    current, native = read(r8_projection), read(r8_compiler)
    require(native["passed"] is True and native["NN_invocations"] == 0
            and native["projection_sha256"] == r8_projection["sha256"]
            and native["inputs"]["compact"]["sha256"] == current["models"]["compact"]["sha256"],
            "R8 native compiler did not verify these inputs")
    read(current["reference_preflight"])
    original_projection, original_compiler, graph = [reference(*item) for item in (PROJECTION, COMPILER, GRAPH)]
    original_compact = read(original_projection)["models"]["compact"]
    gpu_graph = read(graph)
    old_batch = read(original_compact)
    target_batch = read(current["models"]["compact"])
    require(len(target_batch["rows"]) == 100 and len(old_batch["rows"]) == 56
            and old_batch["model"] == target_batch["model"]
            and old_batch["original_manifest"] == target_batch["original_manifest"],
            "original compiler/model or chronological population changed")
    groups = []
    for phase, sha in enumerate(GPU_SHAS):
        path = CAMPAIGN / f"compact-cutlass-cal-r7-first56-phase{phase:02}-r1/execution.json"
        source = reference(path, sha)
        first = phase * 8
        rows, identity = selected(original_projection, original_compiler, graph, first, 8)
        vectors = native_vectors(read(source), rows, phase, identity, gpu_graph, original_compact)
        for original, target, vector in zip(rows, target_batch["rows"][first:first + 8], vectors, strict=True):
            verify_inputs(original, target)
            require(all(type(value) in (int, float) and math.isfinite(value) for value in vector),
                    "nonfinite source score vector")
        groups.append({"source_receipt": source, "first_ordinal": first, "matched_rows": len(rows)})
    return {
        "schema": "airk.private.r8-compact-scientific-score-reuse/1", "r8_projection": r8_projection,
        "r8_compiler": r8_compiler, "r7_projection": original_projection, "r7_compiler": original_compiler,
        "graph": graph, "phases": groups, "reusable_raw_score_rows": 56,
        "fresh_GPU_rows_needed": 44, "new_model_forwards": 0, "calibration_fitted": False,
    }


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.parse_args()
    require(sys.platform == "win32" and sys.flags.isolated and sys.flags.no_site and sys.dont_write_bytecode,
            "use checked Windows Python -I -B -S")
    result = verify_reuse(reference(*R8), reference(*R8_NATIVE))
    print(json.dumps(result, allow_nan=False, separators=(",", ":")))


if __name__ == "__main__":
    main()
