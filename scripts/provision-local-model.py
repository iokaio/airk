# SPDX-License-Identifier: Apache-2.0
"""Deliberately provision pinned Apache-2.0 MiniLM assets; native reads never download."""
import argparse
import hashlib
import importlib.util
import json
import os
from pathlib import Path
import tempfile
import urllib.request

CATALOG_PATH = Path(__file__).resolve().parents[1] / "crates/airk-retrieval/src/model_catalog.json"
CATALOG = json.loads(CATALOG_PATH.read_text(encoding="utf-8"))
_spec = importlib.util.spec_from_file_location("int8_policy", Path(__file__).with_name("int8-policy.py"))
_policy = importlib.util.module_from_spec(_spec)
_spec.loader.exec_module(_policy)
_policy.validate_catalog(CATALOG)
HISTORICAL_INT8_POLICY_SHA256 = "9da184de7f3ae4ef93d9b4fabd6987677ffec0261c6c472f3e49387cd2dcb31d"
default_profiles = [profile for profile in CATALOG["profiles"] if profile["id"] == CATALOG["default_profile"]]
if CATALOG["schema"] != 2 or len(default_profiles) != 1:
    raise ValueError("No unique native-approved default provisioning profile")
PROFILE = default_profiles[0]
REVISION = PROFILE["manifest"]["revision"]
SOURCE = PROFILE["manifest"]["source"]
ASSETS = {asset["name"]: (asset["url"], asset["bytes"], asset["sha256"]) for asset in PROFILE["assets"]}


def validate_asset_bindings(profile=None, assets=None):
    profile = PROFILE if profile is None else profile
    assets = ASSETS if assets is None else assets
    if set(assets) != {"model.onnx", "tokenizer.json", "LICENSE"} or len(profile["assets"]) != 3:
        raise ValueError("Model catalog asset inventory differs")
    for name, field in [("model.onnx", "model_sha256"), ("tokenizer.json", "tokenizer_sha256"), ("LICENSE", "license_sha256")]:
        if profile["manifest"][field] != assets[name][2]:
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


def provision(destination, download=False, profile=None, source_directory=None):
    selected = PROFILE if profile is None else profile
    assets = ASSETS if profile is None else {a["name"]: (a["url"], a["bytes"], a["sha256"]) for a in profile["assets"]}
    validate_asset_bindings(selected, assets)
    destination = Path(destination)
    destination.mkdir(parents=True, exist_ok=True)
    if destination.is_symlink():
        raise ValueError("Destination must be an unlinked directory")
    for name, (url, size, digest) in assets.items():
        path = destination / name
        if path.exists() or path.is_symlink():
            verify(path, size, digest)
            continue
        if source_directory is None and (not download or url is None):
            raise ValueError(f"Missing {name}; assets require explicit authenticated local inputs")
        temporary = None
        try:
            with tempfile.NamedTemporaryFile(dir=destination, prefix="model-download-", delete=False) as output:
                temporary = Path(output.name)
                local = Path(source_directory) / name if source_directory is not None else None
                if local is not None and (local.is_symlink() or not local.is_file()):
                    raise ValueError("Local model input is not a regular unlinked file: " + name)
                # Hash these actual copied bytes before publishing anything;
                # no verify-then-reopen check stands in for their identity.
                incoming = local.open("rb") if local is not None else urllib.request.urlopen(url, timeout=120)
                with incoming as source:
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
    manifest = dict(selected["manifest"])
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


def historical_int8_profiles(path):
    """Read the exact former native capture for explicit offline reproduction."""
    if path.is_symlink() or not path.is_file() or path.stat().st_size > 16_384:
        raise ValueError("The historical INT8 capture must be a regular bounded file")
    with path.open("rb") as source:
        raw = source.read(16_385)
    if hashlib.sha256(raw).hexdigest() != HISTORICAL_INT8_POLICY_SHA256:
        raise ValueError("Historical INT8 model policy capture differs from the retained pin")
    return json.loads(raw)["catalog"]["profiles"]


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("--destination", type=Path, required=True)
    parser.add_argument("--download", action="store_true", help="fetch only when the native catalog supplies an authenticated URL; current assets use authenticated local inputs")
    parser.add_argument("--profile", help="one exact profile ID from the selected policy")
    parser.add_argument("--source-directory", type=Path, help="explicit local model.onnx/tokenizer.json/LICENSE inputs for --profile")
    parser.add_argument("--historical-policy", type=Path, help="exact retained INT8 policy capture for offline reproduction")
    args = parser.parse_args()
    if args.source_directory is not None and args.profile is None:
        parser.error("--source-directory requires one explicit --profile")
    if args.historical_policy is not None:
        if args.profile is None or args.source_directory is None or args.download:
            parser.error("--historical-policy requires --profile and --source-directory without --download")
        available = historical_int8_profiles(args.historical_policy)
    else:
        available = CATALOG["profiles"]
    profiles = [profile for profile in available if args.profile is None or profile["id"] == args.profile]
    if not profiles:
        parser.error("--profile is absent from the selected policy")
    manifest = {profile["id"]: provision(args.destination / profile["id"], args.download, profile, args.source_directory)
                for profile in profiles}
    print(json.dumps({"verified": True, "directory": str(args.destination.resolve()), "manifest": manifest}, indent=2))


if __name__ == "__main__":
    main()
