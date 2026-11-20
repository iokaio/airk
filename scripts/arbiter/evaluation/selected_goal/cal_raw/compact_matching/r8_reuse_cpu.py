# SPDX-License-Identifier: Apache-2.0
"""Reuse the two exact prior Compact CPU reference phases absent from R7."""

import argparse
import hashlib
import json
from pathlib import Path
import runpy
import subprocess
import sys

HERE = Path(__file__).resolve().parent
sys.path.insert(0, str(HERE))
from r7_compare import NUMERICAL, native_vectors
from r7_inputs import CAMPAIGN, COMPILER, GRAPH, IDENTITY, PROJECTION, checked, parent, read, reference, require, selected
from r8_reuse import GPU_SHAS, R8, verify_inputs
from run import MARKER, source_set, verify_result

R5 = (
    CAMPAIGN / "r5-chronological-native-first56-r1/projection-receipt.json",
    "a8be658b0e414115dba40ffa9460054b47d8d68886d943c8858ce56f7c7181e6",
)
REFERENCES = (
    ("d0cf1079addf422fd1809ddbbb774d73448b59b306c315159765d57b4230c824",
     "1a99c887a8268ba98be54637814f9963a61105455c8c9cff00c19803d4fd3abb"),
    ("a4bc44c68b500ab405134e5639ed7c21b028da2a874d9cd24867d9072a4980a8",
     "8f80bd2b6064857c4624eb3e92d30af2ca80659bbd4d06a19c00e93940f28cef"),
)


LOG_SHAS = (
    ("f0bd2bc61975c0fec34b4c26578b21084c6e40378851f2dc0203924555998254",
     "4e9e8a9c19e2d0dd93e818c952cdf0f1dc2c6d3e1f68edb8836e0fa5958e318c"),
    ("609ddf7521608281349e437f994c66a6f8406d8a970e6e646d7d098c1d75fb81",
     "3d70df876fa6ded9791f184246af4c1c48a433100e92653185fec622357f3266"),
)


def historical_sources():
    current = source_set()
    path = HERE / "inputs.py"
    source = "6398e8b6e2b6ff14578fe299c3f6b4b39172cd60:scripts/arbiter/evaluation/selected_goal/cal_raw/compact_matching/inputs.py"
    # The intervening change only added the distinct failed-terminal input mode.
    original = subprocess.check_output(["git", "--no-optional-locks", "show", source])
    sha = hashlib.sha256(original).hexdigest()
    require(sha == "b498c4165dc45eff65feb4b89b964d520860f34b542e3d2dc6c39197691168fa"
            and current[str(path)] == "3ddacad0a8ff4ef0a90be221bd84a29fab3875cfd14b10c0ec861177183a58ce",
            "R5 historical reader or its independently reviewed successor changed")
    return current | {str(path): sha}


def vectors(pin, log_pin, projection, rows, role, sources):
    receipt = read(pin)
    require(receipt["schema"] == "airk.private.compact-protected-cal-cpu-batch/1"
            and receipt["complete"] is True and receipt["input"] == {
                key: projection[key] for key in ("path", "sha256")}
            and receipt["first"] == rows[0]["ordinal"] and receipt["count"] == len(rows)
            and receipt["role"] == role and parent.settled(receipt["owned"])
            and receipt["sources"] == sources, "original R5 CPU worker custody differs")
    require(Path(log_pin["path"]) == Path(pin["path"]).with_name("worker.log"),
            "original worker log path differs")
    raw = checked(log_pin, limit=16 << 20)
    lines = [line[len(MARKER):] for line in raw.decode("utf-8").splitlines() if line.startswith(MARKER)]
    measured = receipt["result"]
    require(len(lines) == 1 and json.loads(lines[0]) == measured
            and measured["candidate_graph"]["sha256"] == IDENTITY["model_sha256"]
            and measured["candidate_weights"]["sha256"] == IDENTITY["weight_sha256"]
            and measured["matching_recipe_sha256"] == IDENTITY["matching_recipe_sha256"],
            "original CPU scoring files, arithmetic or native output differ")
    verify_result(measured, rows, "r5_original", role)
    return [row["scores"] for row in measured["rows"]]


def compare():
    r5_pin, r8_pin = reference(*R5), reference(*R8)
    original = read(read(r5_pin)["models"]["compact"])["rows"]
    current = read(read(r8_pin)["models"]["compact"])["rows"]
    projection, compiler, graph = (reference(*item) for item in (PROJECTION, COMPILER, GRAPH))
    gpu_model = read(projection)["models"]["compact"]
    gpu_graph = read(graph)
    numerical = runpy.run_path(str(NUMERICAL))["numerical"]
    sources = historical_sources()
    groups = []
    for phase, (matching_sha, publisher_sha) in enumerate(REFERENCES, start=3):
        first = phase * 8
        interval = f"{first:03}-{first + 7:03}"
        r5_rows = original[first:first + 8]
        r8_rows = current[first:first + 8]
        for old, target in zip(r5_rows, r8_rows, strict=True):
            verify_inputs(old, target)
        r7_rows, proof = selected(projection, compiler, graph, first, 8)
        cuda = reference(CAMPAIGN / f"compact-cutlass-cal-r7-first56-phase{phase:02}-r1/execution.json", GPU_SHAS[phase])
        gpu = native_vectors(read(cuda), r7_rows, phase, proof, gpu_graph, gpu_model)
        refs = [reference(CAMPAIGN / f"cal100-compact-protected-r5-{kind}-{interval}-r1/execution.json", sha)
                for kind, sha in (("matching", matching_sha), ("publisher", publisher_sha))]
        logs = [reference(Path(pin["path"]).with_name("worker.log"), sha)
                for pin, sha in zip(refs, LOG_SHAS[phase - 3], strict=True)]
        fp16 = vectors(refs[0], logs[0], r5_pin, r5_rows, "protected_matching", sources)
        fp32 = vectors(refs[1], logs[1], r5_pin, r5_rows, "publisher_fp32", sources)
        rows = [{"ordinal": row["ordinal"], "id": row["id"], "numerical": numerical(a, b, c)}
                for row, a, b, c in zip(r8_rows, gpu, fp16, fp32, strict=True)]
        require(all(row["numerical"]["release_numerical_case_pass"] for row in rows),
                "original R5 references fail the cross-capture numerical comparison")
        groups.append({"original_GPU": cuda, "original_CPU": refs, "original_CPU_logs": logs, "rows": rows})
    return {
        "schema": "airk.private.r8-compact-scientific-cpu-reuse/1", "r8_projection": r8_pin,
        "original_r5_projection": r5_pin, "r7_projection": projection, "graph": graph,
        "groups": groups, "reused_comparisons": sum(len(group["rows"]) for group in groups),
        "new_model_forwards": 0, "calibration_fitted": False,
    }


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.parse_args()
    require(sys.platform == "win32" and sys.flags.isolated and sys.flags.no_site and sys.dont_write_bytecode,
            "use checked Windows Python -I -B -S")
    print(json.dumps(compare(), allow_nan=False, separators=(",", ":")))


if __name__ == "__main__":
    main()
