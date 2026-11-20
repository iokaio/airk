# SPDX-License-Identifier: Apache-2.0
"""Bind a frozen development request to the native-emitted static-1024 Focus roster."""
import copy
import hashlib
import json
from pathlib import Path


CAMPAIGN = Path(__file__).resolve().parents[4] / "artifacts/v190/final-plan/exec-20261116T052942Z"
AUDIT = CAMPAIGN / "cal100-focus1024-launch-r1/cal100-launch-audit.json"
AUDIT_SHA = "87731282ecd121eaf08d5074d1bc421c5753bc65f7126f9578b31c6f9303d661"
ROSTER_SHA = "8fbde10f3fc8a42d1d05ec0f362a1e0527840ad49fa6686224f1feee4b08373d"
FOCUS = "94c65d0c598b0b110cd25ddeb1c167ae730430197c0d5e6a59d79ee226c17c64"
OLD_FOCUS = "c8c5918be0f9e6a7d74e797250d0f8e7a32fd63e226d1deb662f44240651f632"
LARGE = "d0e3c74813347b9bb4d3e17ffa4e337bbd16574ca80a81e4b1504c7e1546b307"


def load(path, digest):
    raw = path.read_bytes()
    if hashlib.sha256(raw).hexdigest() != digest:
        raise ValueError("native roster input differs: " + path.name)
    return json.loads(raw)


def bind(original, original_sha):
    audit = load(AUDIT, AUDIT_SHA)
    if (audit["profiles"] != {"arbiter-context-qwen3-17b": original["cpu_context_bootstrap_manifest"],
                              "arbiter-context-qwen3-4b": LARGE, "arbiter-focus-qwen3-reranker-06b": FOCUS}
            or original["cpu_focus_manifest"] != OLD_FOCUS or audit["pipeline"] != original["required_minilm_pipeline"]
            or audit["selected_tests"] != 1 or audit["selector_check"][-1] != original["native_selector"]):
        raise ValueError("wrong source profile roster or native pipeline")
    for key in ("service", "cli", "minilm"):
        if any(original["runtime_inputs"][key][field] != audit["inputs"][key][field] for field in ("path", "sha256")):
            raise ValueError("unexpected shared runtime change: " + key)
    seal = audit["inputs"]["inputs"]
    if seal["sha256"] != ROSTER_SHA:
        raise ValueError("unexpected new source roster")
    roster = load(Path(seal["path"]), ROSTER_SHA)
    descriptors = [load(Path(entry["profile"]), entry["profile_sha256"]) for entry in roster]
    if {profile["model"]: profile["manifest"] for profile in descriptors} != audit["profiles"] or len(roster) != 3:
        raise ValueError("native descriptor identities differ")
    focus, = (profile for profile in descriptors if profile["stage"] == "focus")
    if (focus["max_tokens"], focus["max_candidates"]) != (1024, 4):
        raise ValueError("wrong fixed Focus dimensions")
    rebound = copy.deepcopy(original)
    rebound["cpu_focus_manifest"] = FOCUS
    rebound["runtime_inputs"]["inputs"] = {k: seal[k] for k in ("path", "sha256")}
    rebound["original_focus512_descriptor_sha256"] = original_sha
    rebound["focus1024_launch_audit_sha256"] = AUDIT_SHA
    rebound["focus1024_resource_policy_digest"] = audit["focus_static1024"]["resource_policy_digest"]
    return rebound
