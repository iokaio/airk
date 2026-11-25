# SPDX-License-Identifier: Apache-2.0
"""Prepare the reviewed FP16 MiniLM export for a checked local Cargo build."""
import argparse
from hashlib import sha256
import json
import os
import stat
from pathlib import Path


PROFILE = "minilm-l6-v2-fp16-v1"
PINNED = {
    "model.onnx": (45469884, "f3c3a4eed373650ba1e2f4d3d53437f1ecf92d4c50e5994e07e163751923ee4a"),
    "tokenizer.json": (466247, "be50c3628f2bf5bb5e3a7f17b1f74611b2561a3a27eeab05e5aa30f411572037"),
    "LICENSE": (11358, "cfc7749b96f63bd31c3c42b5c471bf756814053e847c10f3eb003417bc523d30"),
}
SOURCE_MANIFEST = "c175df24836f8986ac9e041153c3d168b4e18a057889febbd66b4308460e0514"
BUILD_MANIFEST = "f311662cd7cc0f48dbf595b26d648537bd18e67a46d60b3f9f4e2150b2e63779"
PROOF = "7422648ca9e7b9d26e2ae6e131c0ad7e0aa4a0c70eeef64519691727db7b340b"


def unlinked(path):
    attributes = getattr(path.lstat(), "st_file_attributes", 0)
    return not path.is_symlink() and not attributes & stat.FILE_ATTRIBUTE_REPARSE_POINT


def verified(path, size, digest):
    if not unlinked(path) or not path.is_file() or path.stat().st_size != size:
        raise ValueError(f"Pinned asset does not match type or size: {path}")
    with path.open("rb") as source:
        data = source.read(size + 1)
    if len(data) != size or sha256(data).hexdigest() != digest:
        raise ValueError(f"Pinned asset does not match hash: {path}")
    return data


def provision(export_bundle, shipped_license, destination):
    folder = destination / PROFILE
    if not all(path.is_dir() and unlinked(path) for path in (export_bundle, destination, folder)):
        raise ValueError("Use unlinked export and preflighted build-input directories")
    if any(folder.iterdir()):
        raise ValueError("The build-input profile directory must be empty")
    proof_path = export_bundle / "receipt.json"
    if not unlinked(proof_path) or not proof_path.is_file() or proof_path.stat().st_size > 16384:
        raise ValueError("The reviewed export receipt is missing or oversized")
    with proof_path.open("rb") as source:
        proof = source.read(16385)
    if sha256(proof).hexdigest() != PROOF:
        raise ValueError("The export proof identity changed")
    manifest = json.loads(verified(export_bundle / "model.json", 714, SOURCE_MANIFEST))
    manifest["license_sha256"] = PINNED["LICENSE"][1]
    manifest_bytes = (json.dumps(manifest, indent=2) + "\n").encode()
    if sha256(manifest_bytes).hexdigest() != BUILD_MANIFEST:
        raise ValueError("The build-input manifest does not match the reviewed schema-2 ABI")
    assets = {
        name: verified(shipped_license if name == "LICENSE" else export_bundle / name, size, digest)
        for name, (size, digest) in PINNED.items()
    }
    assets["model.json"] = manifest_bytes
    for name, data in assets.items():
        with (folder / name).open("xb") as output:
            output.write(data)
            output.flush()
            os.fsync(output.fileno())
    return {
        "directory": str(destination.resolve()),
        "profile": PROFILE,
        "export_receipt_sha256": PROOF,
        "assets": {
            name: {"bytes": len(data), "sha256": sha256(data).hexdigest()}
            for name, data in assets.items()
        },
    }


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    for name in ("export-bundle", "shipped-license", "destination"):
        parser.add_argument(f"--{name}", type=Path, required=True)
    args = parser.parse_args()
    print(json.dumps(provision(args.export_bundle, args.shipped_license, args.destination), indent=2))


if __name__ == "__main__":
    main()
