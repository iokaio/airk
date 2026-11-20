# SPDX-License-Identifier: Apache-2.0
"""Apply the provisional Compact 1.8.207 development decision to sealed score vectors."""
import hashlib
import json
from pathlib import Path
import runpy

HERE = Path(__file__).resolve().parent
CAMPAIGN = HERE.parents[1] / "artifacts/v190/final-plan/exec-20261116T052942Z"
PROBE = HERE / "probe_compact.py"
HISTORICAL = (
    ("compact-fp16-qkv-fp32-native-t32-r2", "a479f953b5c13076d7c54388a89e257f6a18dc3cc5031a3bae9fa89e2b99e3ee", False),
    ("compact-fp16-qkv-fp32-native-development-r1", "ebf0f2e1eadd7ff543479a254ed81240af4830a1cc9c1c78b1db69e5f1841ade", True),
)


def read(path, digest):
    data = path.read_bytes()
    if hashlib.sha256(data).hexdigest() != digest:
        raise ValueError("source receipt differs")
    return json.loads(data)


def main():
    owner = runpy.run_path(str(PROBE))
    raw_limit = owner["DEVELOPMENT_RAW_LIMIT"]
    near_tie = owner["DEVELOPMENT_NEAR_TIE"]
    if (raw_limit, near_tie) != (0.070, 0.020):
        raise ValueError("expected the Compact 1.8.207 development policy")
    evaluated = []
    for name, digest, complete in HISTORICAL:
        folder = CAMPAIGN / name
        owned = read(folder / "execution.json", digest)
        if (owned["complete"] is not complete or not owned["owned"]["owned_descendants_settled"]
                or owned["owned"]["owned_job_final_active_processes"] != 0):
            raise ValueError("unexpected historical run status")
        if complete:
            pin = owned["outputs"]["native"]
        else:
            if owned["failure"]["message"] != "Compact FP16 T32 matching parity, original mass or time bound refused":
                raise ValueError("original T32 refusal changed")
            pin, = (p for p in owned["failure_outputs"] if Path(p["path"]).name == "native-result.json")
        native = read(folder / "native-result.json", pin["sha256"])
        ref_pin = owned["projection_reference"]
        proof = read(Path(ref_pin["path"]), ref_pin["sha256"])
        refs = proof["references"] if complete else [proof["reference"]]
        for row, ref in zip(native["rows"], refs, strict=True):
            if row["input_digest"] != ref["input"]["input_digest"] or len(ref["input"]["input_ids"]) != row["tokens"]:
                raise ValueError("reference input differs")
            match = owner["development_metrics"](row["raw_scores"], ref["matching_qkv_fp32"])
            fp32 = owner["development_metrics"](row["raw_scores"], ref["original_fp32"])
            ambiguous = match["reference_margin"] <= near_tie
            passed = owner["provisional_development_pass"](match, fp32)
            evaluated.append({"tokens": row["tokens"], "native_input": row["input_digest"], "owned": digest,
                              "native": pin["sha256"], "reference": ref_pin["sha256"], "historical_complete": complete,
                              "matching": match, "original_fp32": fp32, "near_tie_ambiguous": ambiguous,
                              "raw_ceiling_winner_at_risk": match["reference_margin"] <= 2 * raw_limit,
                              "provisional_development_pass": passed})
    if [r["tokens"] for r in evaluated] != [32, 128, 1024, 8192]:
        raise ValueError("unexpected width roster")
    print(json.dumps({"schema": "airk.private.compact-1.8.207-development-rescreen/1",
                      "source": hashlib.sha256(Path(__file__).read_bytes()).hexdigest(),
                      "comparison_owner": hashlib.sha256(PROBE.read_bytes()).hexdigest(), "rows": evaluated,
                      "matching_raw_limit": raw_limit, "near_tie_margin": near_tie, "mass_limit": .05,
                      "qualification_raw_limit": .001, "near_tie_rows": sum(r["near_tie_ambiguous"] for r in evaluated),
                      "qualification": False}, allow_nan=False))


if __name__ == "__main__":
    main()
