# SPDX-License-Identifier: Apache-2.0
"""Compare sealed native and independent Compact decision-token projection inputs."""
import argparse
import hashlib
import json
import math
from pathlib import Path
import struct


def read_pin(path, expected):
    raw = path.read_bytes()
    if hashlib.sha256(raw).hexdigest() != expected:
        raise ValueError("input SHA-256 differs")
    return json.loads(raw)


def compare(native, reference):
    rows = []
    if len(native) != 4 or len(reference) != 56:
        raise ValueError("expected four native samples from 56 independent boundaries")
    for i, layer in enumerate((0, 9, 18, 27)):
        projection = "self_attn.k_proj"
        actual = native[i]
        expected = reference[2 * layer]
        weight = f"layers.{layer}.{projection}.weight"
        name = f"/airk_g5/weight//scorer/layers.{layer}/{projection.replace('.', '/')}/MatMul/input16"
        if actual["name"] != name or expected["weight"] != weight or actual["width"] != 2048:
            raise ValueError("tap name, width or order differs")
        words = [x["bits"] for x in (actual, expected)]
        if any(len(row) != 2048 or any(type(w) is not int or not 0 <= w <= 65535 or (w & 0x7c00) == 0x7c00
                                     for w in row) for row in words):
            raise ValueError("finite complete half rows required")
        a, b = [struct.unpack("<2048e", struct.pack("<2048H", *row)) for row in words]
        diff = [x-y for x, y in zip(a, b, strict=True)]
        norm = math.fsum(x*x for x in b)
        rows.append({"layer": layer, "projection": projection, "changed_words": sum(x != y for x, y in zip(*words)),
                     "max_abs": max(map(abs, diff)), "rms": math.sqrt(math.fsum(x*x for x in diff) / 2048),
                     "relative_l2": math.sqrt(math.fsum(x*x for x in diff) / norm) if norm else None})
    return rows


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    for key in ("native", "reference"):
        parser.add_argument("--" + key, type=Path, required=True)
        parser.add_argument("--" + key + "-sha256", required=True)
    args = parser.parse_args()
    owned = read_pin(args.native, args.native_sha256)
    ref = read_pin(args.reference, args.reference_sha256)
    if (owned["mode"] != "qkv-decision-t1024" or owned["complete"] is not True
            or owned["raw_scores_equal_to_sealed"] is not True or ref["complete"] is not True):
        raise ValueError("completed sealed decision captures required")
    ref = ref["result"]
    if ref["schema"] != "airk.private.compact-t1024-reference-decision-rows/1":
        raise ValueError("reference decision capture schema differs")
    native = read_pin(args.native.with_name("native-result.json"), owned["outputs"]["native"]["sha256"])
    if native["rows"][0]["input_digest"] != ref["input_digest"] or ref["position"] != 1023:
        raise ValueError("decision input differs")
    result = {"native_sha256": args.native_sha256, "reference_sha256": args.reference_sha256,
              "qualification": False, "rows": compare(native["layer0_taps"], ref["rows"])}
    print(json.dumps(result, allow_nan=False))


if __name__ == "__main__":
    main()
