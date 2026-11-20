# SPDX-License-Identifier: Apache-2.0
"""Reconstruct protected Large T32 readout from independently authenticated decision-token operands."""
import argparse
import hashlib
import json
from pathlib import Path
import runpy

ROOT = Path(__file__).resolve().parents[2]
CAMPAIGN = ROOT / "artifacts/v190/final-plan/exec-20261116T052942Z"
OBSERVER = ROOT / "scripts/arbiter-fp16-reference/large_hidden_reference.py"
OBSERVER_SHA = "be9af50c94d7fb63f420a0d04d031ae6c4097abd920baf6c8d9e5aef5b385d10"
SCORE_SHA = "6eb43a120a45669915d99cdf57f71143228a6feeaaa1d0c444816ccb0f280790"
BASE_SHA = "bc91621b62faeec0963caec75321c34b76e36f4bd66bbb7df6746d2872ab6191"


def read(path, expected):
    if not path.is_absolute() or not path.is_relative_to(CAMPAIGN) or path.resolve(strict=True) != path:
        raise ValueError("original private campaign input required")
    data = path.read_bytes()
    if len(data) > 8 << 20 or hashlib.sha256(data).hexdigest() != expected:
        raise ValueError(f"input differs: {path.name}")
    return json.loads(data)


def compare(native_digest, reference_digest):
    native_path = CAMPAIGN / "windows-large-protected-qkv-hidden-t32-r1/native.json"
    ref_path = CAMPAIGN / "large-fp32-qkv-hidden-independent-t32-r1/reference.json"
    native, ref = read(native_path, native_digest), read(ref_path, reference_digest)
    original = read(CAMPAIGN / "windows-large-protected-qkv-t32-r1/native.json", SCORE_SHA)
    expected = read(CAMPAIGN / "large-fp32-qkv-independent-t32-r1/reference.json", BASE_SHA)
    if (hashlib.sha256(OBSERVER.read_bytes()).hexdigest() != OBSERVER_SHA
            or ref["hidden_source"]["sha256"] != OBSERVER_SHA or ref["matching_fp16"] != expected["matching_fp16"]
            or ref["candidate"] != expected["candidate"] or ref["weights"] != expected["weights"]
            or ref["input_digest"] != expected["input_digest"] or ref["hidden"]["candidate_ids"] != list(range(32, 40))
            or native["independent_proof"]["sha256"] != BASE_SHA or native["profile"] != original["profile"]
            or native["reviewed_graph"] != original["reviewed_graph"]):
        raise ValueError("capture source, native input, candidate graph or original matching vector differs")
    result = runpy.run_path(str(OBSERVER))["compare"](native, ref["hidden"],original["rows"][0]["raw_scores"])
    return {"schema":"airk.private.large-qkv-hidden-attribution/1","native":native_digest,
            "independent":reference_digest,"source_score":SCORE_SHA,"source_reference":BASE_SHA,**result}


if __name__ == "__main__":
    p = argparse.ArgumentParser(description=__doc__)
    p.add_argument("--native-sha256", required=True)
    p.add_argument("--reference-sha256", required=True)
    args = p.parse_args()
    print(json.dumps(compare(args.native_sha256, args.reference_sha256), allow_nan=False))
