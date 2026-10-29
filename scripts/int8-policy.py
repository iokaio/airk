# SPDX-License-Identifier: Apache-2.0
"""Release boundary: exactly two approved offline INT8 models, never FP32."""
import json
from pathlib import Path

ROOT = Path(__file__).resolve().parents[1]
APPROVED = {
    "minilm-l6-v2-u8-v1": ("uint8", 23046789, "b941bf19f1f1283680f449fa6a7336bb5600bdcd5f84d10ddc5cd72218a0fd21"),
    "minilm-l6-v2-s8-v1": ("int8", 23026053, "4278337fd0ff3c68bfb6291042cad8ab363e1d9fbc43dcb499fe91c871902474"),
}


def catalog():
    value = json.loads((ROOT / "crates/airk-retrieval/src/model_catalog.json").read_text(encoding="utf-8"))
    validate_catalog(value)
    return value


def validate_catalog(value):
    profiles = value.get("profiles", [])
    if (value.get("schema") != 1 or len(profiles) != 2
            or {p.get("id") for p in profiles} != set(APPROVED)
            or value.get("default_profile") != "minilm-l6-v2-u8-v1"):
        raise ValueError("Installer requires exactly the two approved INT8 models")
    for profile in profiles:
        precision, size, digest = APPROVED[profile["id"]]
        assets = profile.get("assets", [])
        weights = [a for a in assets if a.get("name") == "model.onnx"]
        if (profile.get("precision") != precision or len(weights) != 1
                or weights[0].get("bytes") != size or weights[0].get("sha256") != digest
                or profile.get("manifest", {}).get("model_sha256") != digest):
            raise ValueError("FP32 or unapproved model is forbidden in build and installer")


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
