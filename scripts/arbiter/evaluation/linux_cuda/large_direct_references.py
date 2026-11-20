# SPDX-License-Identifier: Apache-2.0
"""Read the independent Large direct FP32-activation CPU references for nominated original receipts."""
import argparse
import json
import math

from cutlass_references import CAMPAIGN, document, original


STAGE = "c2ac1951a3c0b5817380c1c6164b25e4e2f3b679ba569b727a7ebbbbf1ea9a35"
GRAPH = "c1b5776144c6d9af07d08552cae40e491d8f866e89c53bab567adeeb32ca75e4"
WEIGHTS = "5b053f02795e72ff88123b28f89d3f2c89da055e9ed2228bc60809dd0a2b4270"


def row(width, digest):
    if width not in (32, 128, 1024, 8192):
        raise ValueError("expected an original numeric control width")
    directory = CAMPAIGN / f"large-direct-fp32-independent-t{width}-r1"
    packet = document(directory / "execution.json", digest)
    owner = packet["owned"]
    if not (packet["complete"] and not packet["whole_phase_expired"] and owner["exit_code"] == 0
            and owner["owned_descendants_settled"] and not owner["owned_job_forced_termination"]
            and owner["owned_job_final_active_processes"] == 0):
        raise ValueError("original independent CPU Job did not settle")
    pin = packet["proof"]
    result = document(directory / "reference.json", pin["sha256"], pin["bytes"])
    if (result["recipe"] != "large-weight-only-fp32-sm86-compensated-causal-mha"
            or result["model"] != "large" or result["tokens"] != width or result["device"] != "cpu"
            or result["forward_count"] != 1 or result["qualification"] is not False):
        raise ValueError("original publisher CPU model, recipe, or forward count changed")
    publisher = result["publisher"]
    if (publisher["revision"] != "1cfa9a7208912126459214e8b04321603b3df60c"
            or publisher["logical_source_dtype"] != "float32" or publisher["publisher_storage_dtype"] != "bfloat16"
            or publisher["candidate_ONNX_used_for_expected_values"] is not False):
        raise ValueError("publisher revision, dtypes, or reference independence changed")
    candidate = result["candidate"]
    stage = original(candidate["stage"])
    if (candidate["stage"]["sha256"] != STAGE or candidate["graph"] != result["graph"]
            or result["graph"]["sha256"] != GRAPH or result["graph"]["bytes"] != 1_100_471
            or result["weights"]["sha256"] != WEIGHTS or result["weights"]["bytes"] != 8_045_328_384
            or stage["weights"] != result["weights"]):
        raise ValueError("original direct graph and weights differ")
    for key in ("matching_fp16", "original_fp32"):
        scores = result[key]
        if len(scores) != 8 or not all(math.isfinite(v) for v in scores):
            raise ValueError("expected exactly eight finite CPU scores")
    return {"width": width, "input_sha256": result["input_digest"], "execution_sha256": digest,
            "reference_sha256": pin["sha256"], "matching": result["matching_fp16"],
            "publisher": result["original_fp32"], "matching_argmax": result["matching_fp16"].index(max(result["matching_fp16"]))}


if __name__ == "__main__":
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("--row", nargs=2, metavar=("WIDTH", "EXECUTION_SHA256"), required=True, action="append")
    args = parser.parse_args()
    widths = [int(n) for n, _ in args.row]
    if len(set(widths)) != len(widths):
        parser.error("each original width may appear only once")
    print(json.dumps({"stage_sha256": STAGE, "graph_sha256": GRAPH, "weights_sha256": WEIGHTS,
                      "gnu_gpu_forwards": 0, "rows": [row(int(n), sha) for n, sha in args.row]}, indent=2))
