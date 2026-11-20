# SPDX-License-Identifier: Apache-2.0
"""Compare paired independent CPU vectors with one closed Compact R7 GPU phase."""
import argparse
import json
from pathlib import Path
import runpy
import sys

sys.path.insert(0, str(Path(__file__).resolve().parent))
from inputs import CAMPAIGN, SCIENCE, digest, require
from r7_inputs import COMPILER, GRAPH, PROJECTION, canonical, checked, parent, read, reference, selected

NUMERICAL = SCIENCE / "score_comparison.py"
LIVE_SOURCE_PINS = {
    "r7_reference.py": "c540fc1257f5baea2e13001cf3c6897e60defec12c0bc1b5a05990298cbe377b",
    "r7_affinity.py": "66211bb41ef81605f6935557fa6233de52370c4c93ed90fe9de7778e4c0d46c0",
}


def compatible_sources(observed, current):
    if observed == current:
        return True
    # The original live workers used these exact pre-recovery runner revisions.
    # Every scoring dependency must still equal the current reviewed source seal.
    directory = Path(__file__).resolve().parent
    original = current | {str(directory / name): sha for name, sha in LIVE_SOURCE_PINS.items()}
    return observed == original


def native_vectors(report, rows, phase, provenance, graph, model_input):
    result = report["result"]
    cal = result["cal"]
    require(report["complete"] is True and report["mode"] == "cal" and parent.settled(report["owned"])
            and report["image"] == graph["image"] and report["build"] == graph["build"]
            and report["runtime_manifest"] == graph["runtime_manifest"]
            and all(result[key] == provenance[key] for key in ("model_sha256", "weight_sha256", "typed_graph_sha256"))
            and result["cal_native_forwards"] == len(cal["rows"]) == 8 and cal["phase"] == phase
            and cal["original_projection"] == {"path": model_input["path"], "sha256": model_input["sha256"]}
            and cal["launch_audit"]["sha256"] == provenance["launch_audit_sha256"]
            and cal["calibration_fitted"] is False and cal["qualification"] is False,
            "R7 GPU phase does not bind exact graph, phase or original inputs")
    require([item["ordinal"] for item in cal["rows"]] == list(range(phase * 8, phase * 8 + 8)),
            "R7 GPU phase differs from the original ordinal interval")
    vectors = []
    for row in rows:
        candidate = cal["rows"][row["ordinal"] % 8]
        require(row["status"] == "eligible" and candidate["status"] == "raw_scores_only"
                and all(candidate[name] == row[name] for name in ("ordinal", "id", "compiled", "captures", "roster", "remaining_ms"))
                and candidate["input_digest"] == row["compiled"]["input_digest"]
                and len(candidate["scores"]) == len(row["roster"]["cards"]),
                "R7 GPU row changed the exact original input or card roster")
        vectors.append(candidate["scores"])
    return vectors


def verify_cpu(receipt, rows, provenance, role, sources):
    from r7_reference import SCHEMA, verify_identity, verify_result

    path = Path(receipt["path"])
    require(path.parent.parent == CAMPAIGN and path.name == "execution.json", "R7 CPU Job path is outside the campaign")
    observed = read(receipt)
    require(observed["schema"] == SCHEMA and observed["complete"] is True and observed["role"] == role
            and observed["input"] == provenance and compatible_sources(observed["sources"], sources)
            and parent.settled(observed["owned"])
            and observed["first"] == rows[0]["ordinal"] and observed["count"] == len(rows),
            "R7 independent CPU Job does not match original rows or owner")
    checked(reference(path.with_name("worker.log"), observed["log_sha256"]), limit=16 << 20)
    measured = observed["result"]
    verify_result(measured, rows, provenance["mode"], role)
    verify_identity(measured, provenance)
    return [row["scores"] for row in measured["rows"]]


def compare(first, count, protected, publisher, native):
    require(type(first) is int and type(count) is int and 0 <= first < 56 and 1 <= count <= 8
            and (first + count - 1) // 8 == first // 8, "one original R7 GPU phase interval is required")
    phase = first // 8
    expected_native = CAMPAIGN / f"compact-cutlass-cal-r7-first56-phase{phase:02}-r1/execution.json"
    require(Path(native["path"]) == expected_native, "wrong original Compact GPU phase path")
    projection = canonical({"path": str(PROJECTION[0]), "sha256": PROJECTION[1]}, PROJECTION)
    compiler = canonical({"path": str(COMPILER[0]), "sha256": COMPILER[1]}, COMPILER)
    graph = canonical({"path": str(GRAPH[0]), "sha256": GRAPH[1]}, GRAPH)
    rows, proof = selected(projection, compiler, graph, first, count)
    scores = native_vectors(read(native), rows, phase, proof, read(graph), read(projection)["models"]["compact"])
    from r7_reference import source_set

    sources = source_set()
    matching = verify_cpu(protected, rows, proof, "protected_matching", sources)
    original = verify_cpu(publisher, rows, proof, "publisher_fp32", sources)
    numerical = runpy.run_path(str(NUMERICAL))["numerical"]
    return {
        "schema": "airk.private.compact-protected-cal-r7-reference-comparison/1",
        "first": first, "count": count, "input": proof, "native": native, "protected": protected, "publisher": publisher,
        "comparison_source_sha256": digest(NUMERICAL), "calibration_fitted": False, "qualification": False,
        "rows": [{"ordinal": row["ordinal"], "id": row["id"], "numerical": numerical(gpu, fp16, fp32)}
                 for row, gpu, fp16, fp32 in zip(rows, scores, matching, original, strict=True)],
    }


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    for key in ("protected", "publisher", "native"):
        parser.add_argument("--" + key, type=Path, required=True)
        parser.add_argument("--" + key + "-sha256", required=True)
    parser.add_argument("--first", type=int, required=True)
    parser.add_argument("--count", type=int, required=True)
    args = parser.parse_args()
    require(sys.platform == "win32" and sys.flags.isolated and sys.flags.no_site and sys.dont_write_bytecode,
            "use the original checked Windows Python -I -B -S")
    receipts = [reference(getattr(args, key), getattr(args, key + "_sha256")) for key in ("protected", "publisher", "native")]
    print(json.dumps(compare(args.first, args.count, *receipts), allow_nan=False, separators=(",", ":")))


if __name__ == "__main__":
    main()
