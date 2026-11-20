# SPDX-License-Identifier: Apache-2.0
"""Authenticate the four independent CPU reference rows used for Compact GNU CUTLASS comparisons."""
import hashlib
import json
import math
from pathlib import Path, PureWindowsPath


ROOT = Path(__file__).resolve().parents[4]
CAMPAIGN = ROOT / "artifacts/v190/final-plan/exec-20261116T052942Z"
ORIGINAL = PureWindowsPath("D:/code/Github/airk")
RECIPE = "7efd4fc72f11de95131c532467803a6a2c82c71cf84ac0bd23cabbcf3662e394"
NUMERICAL_SOURCE = "9df92e6393079f61b6c2ccd13792afe243d4b3078fc2a6effc04c6651039e46f"
GRAPH = "147be075a73ff81ca17b6c2fb965ec8f887e10908fbd33cf47a919d1694aa3d9"
STAGE = "4465463b434f6bb5e9798307c31e622dc3da4d67b6a5f6bf5793daf5c94beabf"
RECEIPTS = {
    32: "6a6bdd91a2a730a93d2cd92e7d111efa3f6247cc829daf8f7f7922935aa7ea8e",
    128: "aa193d37e85bbd763ef3a4b54b44a6c66bd9fb81b9915b92dd031ed9958a4ed7",
    1024: "a3e113d8354109590423b30870423efba963af464121adebd67f89157241546d",
    8192: "a54cfb9f9d9e725e66724a7e85fdaa60ff14487d6aaa7c3b1854a20001f10f5f",
}


def document(path, digest, size=None):
    if path.resolve(strict=True) != path or not path.is_file() or path.stat().st_size > 4 << 20:
        raise ValueError(f"unexpected original reference file {path}")
    raw = path.read_bytes()
    if hashlib.sha256(raw).hexdigest() != digest or (size is not None and len(raw) != size):
        raise ValueError(f"original reference bytes changed {path}")
    return json.loads(raw)


def original(asset):
    relative = PureWindowsPath(asset["path"]).relative_to(ORIGINAL)
    if relative.parts[0] != "artifacts" or ".." in relative.parts:
        raise ValueError("original asset outside the campaign")
    return document(ROOT.joinpath(*relative.parts), asset["sha256"], asset["bytes"])


def row_for(width, digest):
    receipt = document(CAMPAIGN / f"compact-cutlass-bound-control-t{width}-reference-r1/execution.json", digest)
    owned = receipt["owned"]
    if not (receipt["complete"] and owned["owned_descendants_settled"] and owned["exit_code"] == 0
            and owned["owned_job_final_active_processes"] == 0 and not owned["owned_job_forced_termination"]
            and not owned["timed_out"] and not owned["unexpected_third_process"]):
        raise ValueError("original CPU execution did not settle cleanly")
    result = receipt["result"]
    if (result["schema"] != "airk.private.compact-cutlass-publisher-reference/1"
            or result["recipe_sha256"] != RECIPE or result["numerical_source_sha256"] != NUMERICAL_SOURCE
            or result["candidate_graph_sha256"] != GRAPH or result["stage"]["sha256"] != STAGE):
        raise ValueError("matching recipe, graph, or source differs")
    binding = result["original_binding"]
    fixture = original(binding["fixture"])
    producer = original(binding["producer"])
    compiled = result["compiled"]
    if (fixture["reference_producer"]["sha256"] != binding["producer"]["sha256"]
            or fixture["compiler"] != producer["compiler"] or fixture["compiler"] != binding["compiler"]
            or fixture["resource_policy"] != producer["resource_policy_digest"]
            or fixture["resource_policy"] != binding["resource_policy"]):
        raise ValueError("original fixture and producer binding disagree")
    for source in (fixture, producer):
        row, = [entry for entry in source["rows"] if entry["name"] == binding["name"]]
        if row["input"] != compiled or row["reference_original_fp32"] != binding["original_fp32"]:
            raise ValueError("original compiled input or FP32 scores disagree")
    if (len(compiled["input_ids"]) != width or compiled["attention_mask"] != [1] * width
            or compiled["position_ids"] != list(range(width)) or compiled["decision_position"] != width - 1
            or compiled["candidate_positions"] != list(range(32, 40))
            or compiled["option_mask"] != [True] * 8):
        raise ValueError("all-live original input dimensions differ")
    scores = result["matching_fp16"]
    if len(scores) != 8 or not all(math.isfinite(score) for score in scores):
        raise ValueError("invalid matching CPU scores")
    return {"width": width, "receipt_sha256": digest, "original_name": binding["name"],
            "compiler": binding["compiler"], "resource_policy": binding["resource_policy"],
            "fixture_sha256": binding["fixture"]["sha256"], "producer_sha256": binding["producer"]["sha256"],
            "input_sha256": compiled["input_digest"], "matching_scores": scores,
            "matching_argmax": scores.index(max(scores)), "original_fp32": binding["original_fp32"]}


if __name__ == "__main__":
    print(json.dumps({"recipe_sha256": RECIPE, "graph_sha256": GRAPH, "stage_sha256": STAGE,
                      "numerical_source_sha256": NUMERICAL_SOURCE, "gnu_gpu_forwards": 0,
                      "rows": [row_for(width, digest) for width, digest in RECEIPTS.items()]}, indent=2))
