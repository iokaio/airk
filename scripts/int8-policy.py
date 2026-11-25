# SPDX-License-Identifier: Apache-2.0
"""Consume the native-owned bundled model policy; never invent model pins."""
import json
from pathlib import Path
import re

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


def approved_profile(identifier):
    profiles = [profile for profile in catalog()["profiles"] if profile["id"] == identifier]
    if len(profiles) != 1:
        raise ValueError("Unknown native-approved model profile")
    return profiles[0]


def approved_engine(identifier):
    # Rust's approved runtime is engine/version/backend; consume its engine
    # prefix rather than keeping another version pin in Python callers.
    return approved_profile(identifier)["runtime"].rsplit("/", 1)[0]


def verify_model_smoke(report):
    profile = approved_profile(report.get("profile"))
    if (type(report.get("schema")) is not int or report["schema"] != 1
            or report.get("source") != "embedded"
            or report.get("engine") != profile["runtime"].rsplit("/", 1)[0]
            or type(report.get("dimensions")) is not int or report["dimensions"] != profile["manifest"]["dimensions"]
            or report.get("repeat_equal") is not True
            or any(not isinstance(report.get(field), str) or not re.fullmatch("[0-9a-f]{64}", report[field])
                   for field in ("legacy_identity", "pipeline_digest"))):
        raise ValueError("Approved embedded MiniLM pipeline receipt missing")
    return report


def verify_embedded(report):
    expected = catalog()
    if report.get("schema") != 1 or report.get("fp32_included") is not False:
        raise ValueError("Helper did not verify its complete approved model inventory")
    profiles = report.get("profiles", [])
    if len(profiles) != len(APPROVED) or {p.get("id") for p in profiles} != set(APPROVED):
        raise ValueError("Helper contains an unexpected model inventory")
    for profile in profiles:
        reference = next(p for p in expected["profiles"] if p["id"] == profile["id"])
        if (profile.get("manifest") != reference["manifest"] or profile.get("assets") != reference["assets"]
                or profile.get("precision") != reference["precision"]):
            raise ValueError("Embedded model differs from approved manifest")
    return report
