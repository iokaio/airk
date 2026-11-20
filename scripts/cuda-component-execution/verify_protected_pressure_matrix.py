# SPDX-License-Identifier: Apache-2.0
"""Authenticate the five Windows pressure fits and report the original dispositions."""
import hashlib
import json
import struct
from pathlib import Path

BASE = Path(__file__).resolve().parents[2] / "artifacts/v190/final-plan/exec-20261116T052942Z"
CONTROL = "d9e4abed37173d97ce98aef07615be70dc1fa3d735ee666e5bff467883ac8986"
REVIEW = "cda1dfea8e03310c1286ccd78ede74928388e59e6ac63fd39c457ba27c8d22d7"
# Outer receipt and original native receipt, in acquisition order.
RUNS = [
    (24, 1, "dd82bcac500fb4734dfeaa170dd31a2bcb4f6f710b3acbe53d0cb6ecc98dc0f1", "e058c1c590bd12f2f40d9daae8c89b452f6ff050eb791afa1a390d446a01bd07"),
    (16, 2, "140ec1cc6a99838c2af21a3940c2dfa5f671fb62fff9c28009833a26189b42a6", "a8931ef78662c4ac00e3d97ffe59e68242e4053a5345dd10826f05b017cbdaae"),
    (12, 1, "494b1f23bd03f1d898951a4d5e331a7f231d186f4928f8adedf7093fd92ec802", "384d6cc98c7b7f8ea88ba616f725ad97273d7a56cf9bc5da90bbdc7e05bcb119"),
    (8, 2, "c3b28198ff35c95960c916939ef6aa9db2221e04d656a8b58346db209af19ef1", "d46ff5dcc8a6d848c04703f4917cc8cfa013a63ac6230e6faf5e1558ae7dcd53"),
    (6, 1, "a1584277d7bfce1823d4eac6a0a2d25ac1bc26a44b5b02d0721f2883a412e5a1", "14404415f2592725e8b1031869fb6c8b756fdbcaf12ef93c1314ab837d42c67b"),
]


def require(value, reason):
    if not value:
        raise ValueError(reason)


def reopen(directory, sha):
    data = (BASE / directory / "execution.json").read_bytes()
    require(hashlib.sha256(data).hexdigest() == sha, directory)
    return json.loads(data)


def matrix():
    control = reopen("compact-cutlass-width-score-8192-r1", CONTROL)["result"]["score"]
    review = reopen("compact-capacity-protected-16-reconciliation-r1", REVIEW)
    rows = []
    for tier, attempt, outer_sha, native_sha in RUNS:
        folder = f"compact-capacity-protected-{tier}-gib-pressure-fit-r{attempt}"
        outer = reopen(folder, outer_sha)
        native = reopen(folder + "-native", native_sha)
        owned = native["owned"]
        require(owned["exit_code"] == 0 and owned["owned_descendants_settled"]
                and not owned["owned_job_forced_termination"]
                and owned["owned_job_final_active_processes"] == 0, folder)
        require(outer["tier_gib"] == tier and outer["trial"]["capacity_admitted"] is False, folder)
        if tier == 16:
            require(review["original_outer_sha256"] == outer_sha
                    and review["original_native_sha256"] == native_sha
                    and review["numerical_words_exact"] and not native["complete"], folder)
            elapsed = review["native_call_us"]
            counts = review["counts"]
        else:
            sample, = native["result"]["timing"]["rows"]
            require(native["complete"] and sample["forward_synchronized"] and sample["tokens"] == 8192
                    and sample["input_digest"] == control["compiled_input_digest"]
                    and struct.pack("<8f", *sample["raw_scores"]) == struct.pack("<8f", *control["raw_scores"]), folder)
            elapsed = sample["native_call_us"]
            counts = outer["trial"]["candidate_wddm"]
        require(counts["peak_shared_bytes"] > 0 and not counts["verified"], folder)
        rows.append(dict(tier_gib=tier, directory=folder, outer_sha256=outer_sha, native_sha256=native_sha,
                         outer_complete=outer["complete"], native_wrapper_complete=native["complete"],
                         native_job_exit=0, native_job_final_members=0, native_call_us=elapsed,
                         peak_host_committed_bytes=owned["peak_host_committed_bytes"],
                         exact_score_words=8, candidate_wddm=counts,
                         historical_outcome=outer["trial"]["observed_outcome"],
                         ballast_continuity_failure=tier == 6, capacity_admitted=False))
    return dict(control_sha256=CONTROL, offline_16_gib_review_sha256=REVIEW, rows=rows,
                model_allocation_physical_residency="unmeasured", eligible_pressure_fixed20_tiers=[])


if __name__ == "__main__":
    print(json.dumps(matrix(), indent=2))
