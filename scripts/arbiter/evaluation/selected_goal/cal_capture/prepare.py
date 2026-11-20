# SPDX-License-Identifier: Apache-2.0
"""Authenticate CAL launch inputs and check the actual native image's fixture selector."""

import hashlib
import json
from pathlib import Path
import subprocess
import sys

ROOT = Path(__file__).resolve().parents[5]
CAMPAIGN = ROOT / "artifacts/v190/final-plan/exec-20261116T052942Z"
PAIR = CAMPAIGN / "fp32-paired-answer-r1"
SELECTOR = "service_runtime::evaluation_fixture_tests::actual_private_arbiter_fixture_software_v2"
PLAN = "e85a33d6322a2a38814bbb282afd339f23ae3461b43be8a9461bcc2f7b68a608"
REPLAY = "3a15febdd68f7ab4f2a3572981a039e21a30f218a2a4e8b482641153d488773f"
CENSUS = "84ba7c6834a95e2ea4683d13a7a1ec0d6aec4754982850041851fb180f0a85d8"
PIPELINE_RECEIPT = "d8bd0de007519be5e1216867e4d8ced60694d24a8ec4894f513584b6b75a0f2b"
SDK = {"session.js": "e19ee7e88f52b4b5b9f578868a302aea26212c2d32b7c072b04930462f73e60b",
       "runner.js": "327a8a5595b11142c68c25562e61e7af3120f915788a91c28d834f37a314ce2b",
       "adminChunks.js": "066bb352cb2a800d891000636592756f6618711367b72f0ce003adfaa6ef8427"}


def pin(path, expected):
    path = Path(path)
    if any(p.is_symlink() or p.is_junction() for p in (path, *path.parents)) or path.stat().st_nlink != 1:
        raise ValueError("linked launch input")
    with path.open("rb") as stream:
        if hashlib.file_digest(stream, "sha256").hexdigest() != expected:
            raise ValueError("launch input changed: " + path.name)
    return {"path": str(path), "sha256": expected, "bytes": path.stat().st_size}


def read(path, expected):
    # Parse only the bytes checked; these role files are bounded JSON.
    p = pin(path, expected)
    if p["bytes"] > 4 << 20:
        raise ValueError("unexpected metadata size")
    raw = Path(p["path"]).read_bytes()
    if hashlib.sha256(raw).hexdigest() != expected:
        raise ValueError("metadata changed during preparation")
    return json.loads(raw)


def prepare():
    previous = read(PAIR / "cpu-launch-current-roster.json", PLAN)
    replay = read(CAMPAIGN / "cal100-retained-population-replay-r1.json", REPLAY)
    census_path = CAMPAIGN / "cal100-retained-census-r1.json"
    census = read(census_path, CENSUS)
    if replay["census"] != census or replay["phase_sizes"] != [8] * 12 + [4]:
        raise ValueError("CAL-only census differs")
    inputs = {role: pin(previous[role]["path"], previous[role]["sha256"]) for role in
              ("service", "cli", "inputs", "minilm")}
    for role in ("manifest", "scopes", "rubric", "corpus"):
        ref = replay["inputs"][role]
        inputs[role] = pin(ref["path"], ref["sha256"])
    profiles = {}
    for entry in read(inputs["inputs"]["path"], inputs["inputs"]["sha256"]):
        profile = read(entry["profile"], entry["profile_sha256"])
        profiles[profile["model"]] = profile["manifest"]
    if set(profiles) != {"arbiter-focus-qwen3-reranker-06b", "arbiter-context-qwen3-4b", "arbiter-context-qwen3-17b"}:
        raise ValueError("both Context compiler profiles and Focus required")
    pipeline = read(PAIR / "cpu-timed-r2/measurement.json", PIPELINE_RECEIPT)["search_pipeline"]
    sdk = {name: pin(ROOT / "packages/vscode/out/airkctl" / name, sha) for name, sha in SDK.items()}
    argv = [inputs["service"]["path"], "--list", "--exact", SELECTOR]
    result = subprocess.run(argv, capture_output=True, text=True, timeout=30, check=True)
    if result.stdout.splitlines() != [SELECTOR + ": test", "", "1 test, 0 benchmarks"]:
        raise ValueError("actual fixture image does not select exactly one software-v2 test")
    return {"schema": "airk.private.cal100-launch-input-audit/1", "inputs": inputs,
            "census": pin(census_path, CENSUS), "profiles": profiles, "compiled_sdk": sdk,
            "selector_check": argv, "selected_tests": 1, "pipeline": pipeline,
            "native_template_source": "fresh fixture context-request.json",
            "fixture_environment": {"AIRK_SELECTED_CONTEXT_CAL_OBSERVER": "true",
                                    "AIRK_SELECTED_CONTEXT_MINILM_PIPELINE": pipeline,
                                    "AIRK_ARBITER_EVALUATION_RESOURCE_CONSENT": "throughput64gib4threads",
                                    "AIRK_ARBITER_EVALUATION_SECONDS": "10800",
                                    "AIRK_ARBITER_EVALUATION_INPUTS": inputs["inputs"]["path"],
                                    "AIRK_ARBITER_EVALUATION_INPUTS_SHA256": inputs["inputs"]["sha256"]},
            "phase_counts": replay["phase_sizes"], "fixture_started": False, "calibration_fitted": False}


if __name__ == "__main__":
    if not sys.flags.isolated or not sys.dont_write_bytecode or not sys.flags.no_site:
        raise ValueError("use isolated Python -I -B -S")
    print(json.dumps(prepare(), sort_keys=True, allow_nan=False))
