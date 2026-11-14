# SPDX-License-Identifier: Apache-2.0
"""Consume the native-owned trained model policy; never invent model pins."""
import json
from pathlib import Path

ROOT = Path(__file__).resolve().parents[1]
POLICY_PATH = ROOT / "fixtures/models/model-policy.json"
NATIVE_POLICY = json.loads(POLICY_PATH.read_bytes())
if NATIVE_POLICY.get("schema") != "airk.minilm.native-policy/2":
    raise ValueError("Native model policy capture is unsupported")
REFERENCE = NATIVE_POLICY["catalog"]
APPROVED = {profile["id"]: (profile["precision"], next(asset["bytes"] for asset in profile["assets"] if asset["name"] == "model.onnx"),
                            profile["manifest"]["model_sha256"]) for profile in REFERENCE["profiles"]}


def catalog():
    value = json.loads((ROOT / "crates/airk-retrieval/src/model_catalog.json").read_text(encoding="utf-8"))
    validate_catalog(value)
    return value


def validate_catalog(value):
    # The native capture has passed the owning Rust validator's independent
    # refusal cases. Exact parsed equality preserves every native policy field.
    if value != REFERENCE:
        raise ValueError("Model catalog differs from reviewed native trained policy")


def verify_embedded(report):
    expected = catalog()
    if report.get("schema") != 1 or report.get("fp32_included") is not False:
        raise ValueError("Helper did not verify its complete INT8 inventory")
    profiles = report.get("profiles", [])
    if len(profiles) != 2 or {p.get("id") for p in profiles} != set(APPROVED):
        raise ValueError("Helper contains an unexpected model inventory")
    for profile in profiles:
        reference = next(p for p in expected["profiles"] if p["id"] == profile["id"])
        if (profile.get("manifest") != reference["manifest"] or profile.get("assets") != reference["assets"]
                or profile.get("precision") != reference["precision"]):
            raise ValueError("Embedded model differs from approved INT8 manifest")
    return report
