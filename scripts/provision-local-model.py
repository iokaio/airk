# SPDX-License-Identifier: Apache-2.0
"""Deliberately provision pinned Apache-2.0 MiniLM assets; native reads never download."""
import argparse
import hashlib
import json
import os
from pathlib import Path
import tempfile
import urllib.request

CATALOG_PATH = Path(__file__).resolve().parents[1] / "crates/airk-retrieval/src/model_catalog.json"
CATALOG = json.loads(CATALOG_PATH.read_text(encoding="utf-8"))
PROFILE, = CATALOG["profiles"]
if CATALOG["schema"] != 1 or PROFILE["id"] != CATALOG["default_profile"] or PROFILE["precision"] != "fp32":
    raise ValueError("No approved FP32 provisioning profile")
REVISION = PROFILE["manifest"]["revision"]
SOURCE = PROFILE["manifest"]["source"]
ASSETS = {asset["name"]: (asset["url"], asset["bytes"], asset["sha256"]) for asset in PROFILE["assets"]}


def validate_asset_bindings():
    if set(ASSETS) != {"model.onnx", "tokenizer.json", "LICENSE"} or len(PROFILE["assets"]) != 3:
        raise ValueError("Model catalog asset inventory differs")
    for name, field in [("model.onnx", "model_sha256"), ("tokenizer.json", "tokenizer_sha256"), ("LICENSE", "license_sha256")]:
        if PROFILE["manifest"][field] != ASSETS[name][2]:
            raise ValueError("Model catalog manifest and asset hashes differ")


def verify(path, expected_size, expected_digest):
    if path.is_symlink() or not path.is_file() or path.stat().st_size != expected_size:
        raise ValueError(f"Invalid local asset size/type: {path.name}")
    digest = hashlib.sha256()
    count = 0
    with path.open("rb") as source:
        for block in iter(lambda: source.read(1024 * 1024), b""):
            count += len(block)
            if count > expected_size:
                raise ValueError(f"Asset grew while verifying: {path.name}")
            digest.update(block)
    if count != expected_size or digest.hexdigest() != expected_digest:
        raise ValueError(f"Asset digest mismatch: {path.name}")


def provision(destination, download=False):
    validate_asset_bindings()
    destination = Path(destination)
    destination.mkdir(parents=True, exist_ok=True)
    if destination.is_symlink():
        raise ValueError("Destination must be an unlinked directory")
    for name, (url, size, digest) in ASSETS.items():
        path = destination / name
        if path.exists() or path.is_symlink():
            verify(path, size, digest)
            continue
        if not download:
            raise ValueError(f"Missing {name}; use --download to explicitly fetch pinned public assets")
        temporary = None
        try:
            with tempfile.NamedTemporaryFile(dir=destination, prefix="model-download-", delete=False) as output:
                temporary = Path(output.name)
                with urllib.request.urlopen(url, timeout=120) as source:
                    count = 0
                    for block in iter(lambda: source.read(1024 * 1024), b""):
                        count += len(block)
                        if count > size:
                            raise ValueError(f"Download exceeds pinned size: {name}")
                        output.write(block)
                output.flush()
                os.fsync(output.fileno())
            verify(temporary, size, digest)
            # Refuse to overwrite assets created by another provisioner.
            with path.open("xb") as output, temporary.open("rb") as source:
                for block in iter(lambda: source.read(1024 * 1024), b""):
                    output.write(block)
                output.flush()
                os.fsync(output.fileno())
        finally:
            if temporary is not None:
                temporary.unlink(missing_ok=True)
    manifest = dict(PROFILE["manifest"])
    path = destination / "model.json"
    payload = (json.dumps(manifest, indent=2) + "\n").encode()
    if path.exists() or path.is_symlink():
        if path.is_symlink() or path.read_bytes() != payload:
            raise ValueError("Existing model.json differs; select a new destination")
    else:
        with path.open("xb") as output:
            output.write(payload)
            output.flush()
            os.fsync(output.fileno())
    return manifest


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("--destination", type=Path, required=True)
    parser.add_argument("--download", action="store_true", help="explicitly fetch the pinned public model/tokenizer/license")
    args = parser.parse_args()
    manifest = provision(args.destination, args.download)
    print(json.dumps({"verified": True, "directory": str(args.destination.resolve()), "manifest": manifest}, indent=2))


if __name__ == "__main__":
    main()
