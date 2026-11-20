# SPDX-License-Identifier: Apache-2.0
"""Prepare and verify the exact public Arbiter release payload for the AIRK Hub repository."""
import argparse
import hashlib
import json
import os
from pathlib import Path
import re
import stat
import sys

REPO = "iokaio/airk-arbiter-models"
SCHEMA = "airk.private.arbiter-mirror-inventory/1"
MAX_PART = 1 << 30
MAX_CARD = 1 << 20
MAX_INVENTORY = 1 << 20
MAX_FILES = 256
COMPONENT = re.compile(r"[A-Za-z0-9._-]{1,192}\Z")
LABEL = re.compile(r"[a-z0-9]+(?:[.-][a-z0-9]+)*\Z")
SHA256 = re.compile(r"[a-f0-9]{64}\Z")
SLOTS = ("focus", "context-compact", "context-large")
MODEL_SUFFIXES = {"model.onnx", "model.onnx.data"}
COMPANION_SUFFIX = re.compile(r"[a-z0-9]+(?:-[a-z0-9]+)*\.(?:json|txt|md|pdf)\Z")


def basename(text):
    if not isinstance(text, str) or text in (".", "..") or not COMPONENT.fullmatch(text):
        raise ValueError("expected a flat safe release filename")
    return text


def public_name(slot, upstream, variant, revision, suffix):
    if (slot not in SLOTS or not LABEL.fullmatch(upstream) or not LABEL.fullmatch(variant)
            or type(revision) is not int or not 1 <= revision <= 999):
        raise ValueError("invalid release naming input")
    part = re.fullmatch(r"model\.onnx\.data\.part([0-9]{3})", suffix)
    if suffix not in MODEL_SUFFIXES and not COMPANION_SUFFIX.fullmatch(suffix) and not (part and 1 <= int(part.group(1)) <= 16):
        raise ValueError("unknown release asset role or part ordinal")
    return basename(f"airk-arbiter-{slot}-{upstream}-{variant}-r{revision:03}.{suffix}")


def checked_info(path, *, directory=False):
    if not path.is_absolute() or path.resolve(strict=True) != path:
        raise ValueError("release inventory and payload paths must be absolute and resolved")
    info = path.lstat()
    reparse = getattr(info, "st_file_attributes", 0) & getattr(stat, "FILE_ATTRIBUTE_REPARSE_POINT", 0)
    if reparse or not (stat.S_ISDIR(info.st_mode) if directory else stat.S_ISREG(info.st_mode) and info.st_nlink == 1):
        raise ValueError("expected a regular release file or directory without links")
    return info


def fingerprint(info):
    return info.st_dev, info.st_ino, info.st_size, info.st_mtime_ns


def verified_file(path, size_limit, expected_size=None, *, contents=False):
    original = checked_info(path)
    if original.st_size <= 0 or original.st_size > size_limit or expected_size not in (None, original.st_size):
        raise ValueError(f"release file length differs: {path.name}")
    with path.open("rb") as source:
        if fingerprint(os.fstat(source.fileno())) != fingerprint(original):
            raise ValueError(f"release file identity changed: {path.name}")
        if contents:
            data = source.read(size_limit + 1)
            length, sha = len(data), hashlib.sha256(data).hexdigest()
        else:
            digest = hashlib.sha256()
            length = 0
            while block := source.read(1 << 20):
                length += len(block)
                if length > original.st_size:
                    raise ValueError(f"release file grew: {path.name}")
                digest.update(block)
            data, sha = None, digest.hexdigest()
        if length != original.st_size or fingerprint(os.fstat(source.fileno())) != fingerprint(original):
            raise ValueError(f"release file changed while reading: {path.name}")
    if fingerprint(checked_info(path)) != fingerprint(original):
        raise ValueError(f"release file path changed: {path.name}")
    return sha, data


def unique_object(items):
    result = {}
    for name, value in items:
        if name in result:
            raise ValueError(f"duplicate inventory field: {name}")
        result[name] = value
    return result


def inventory(path, inventory_sha, directory):
    checked_info(directory, directory=True)
    if path.parent == directory:
        raise ValueError("keep the private inventory outside the public payload directory")
    sha, raw = verified_file(path, MAX_INVENTORY, contents=True)
    if sha != inventory_sha:
        raise ValueError("inventory SHA-256 differs")
    value = json.loads(raw, object_pairs_hook=unique_object)
    if (not isinstance(value, dict) or set(value) != {"schema", "repo", "files", "card"}
            or value["schema"] != SCHEMA or value["repo"] != REPO
            or not isinstance(value["files"], list) or not 1 <= len(value["files"]) <= MAX_FILES):
        raise ValueError("invalid AIRK Hub release inventory")
    names = set()

    def pin(row, *, card=False):
        if (not isinstance(row, dict) or set(row) != {"path", "bytes", "sha256"}
                or type(row["bytes"]) is not int or not isinstance(row["sha256"], str)
                or not SHA256.fullmatch(row["sha256"])):
            raise ValueError("invalid release file pin")
        name = basename(row["path"])
        if (name.casefold() == "readme.md") != card or (card and name != "README.md") or name.casefold() in names:
            raise ValueError(f"duplicate or misplaced release file: {name}")
        if not card and (not name[0].isalnum()
                         or name != name.lower() or ".." in name):
            raise ValueError(f"expected a lowercase public asset filename: {name}")
        names.add(name.casefold())
        local = directory / name
        actual, _ = verified_file(local, MAX_CARD if card else MAX_PART, row["bytes"])
        if actual != row["sha256"]:
            raise ValueError(f"release file SHA-256 differs: {name}")
        return row | {"local_path": local}

    files = [pin(row) for row in value["files"]]
    card = pin(value["card"], card=True) if value["card"] is not None else None
    expected = {row["path"] for row in files}
    if card:
        expected.add(card["path"])
    if {entry.name for entry in directory.iterdir()} != expected:
        raise ValueError("staging directory must contain exactly the reviewed release files")
    return files, card


def remote_only(row):
    return {key: row[key] for key in ("path", "bytes", "sha256")}


def release_report(result, pins, card):
    expected = pins + ([card] if card else [])
    if (result["repo"] != REPO or not re.fullmatch(r"[0-9a-f]{40}", result["commit"])
            or result["files"] != expected):
        raise ValueError("Hub readback does not cover the exact release inventory and optional card")
    return result | {"files": pins}


def local_token():
    token = os.environ.get("AIRK_HF_TOKEN")
    if token:
        return token
    from huggingface_hub import get_token

    token = get_token()
    if not token:
        raise ValueError("no Hub credential in AIRK_HF_TOKEN or the active hf CLI login")
    return token


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    operations = parser.add_subparsers(dest="operation", required=True)
    naming = operations.add_parser("name")
    naming.add_argument("--slot", choices=SLOTS, required=True)
    for key in ("upstream", "variant", "suffix"):
        naming.add_argument("--" + key, required=True)
    naming.add_argument("--revision", type=int, required=True)
    for command in ("check", "verify-hub", "publish-hub"):
        operation = operations.add_parser(command)
        operation.add_argument("--inventory", type=Path, required=True)
        operation.add_argument("--inventory-sha256", required=True)
        operation.add_argument("--directory", type=Path, required=True)
        if command == "verify-hub":
            operation.add_argument("--commit", required=True)
        elif command == "publish-hub":
            operation.add_argument("--expected-head", required=True)
    args = parser.parse_args()
    try:
        if args.operation == "name":
            print(public_name(args.slot, args.upstream, args.variant, args.revision, args.suffix))
            return 0
        files, card = inventory(args.inventory, args.inventory_sha256, args.directory)
        pins = [remote_only(row) for row in files]
        if args.operation == "check":
            result = {"repo": REPO, "inventory_sha256": args.inventory_sha256, "files": pins,
                      "card": remote_only(card) if card else None}
        elif args.operation == "verify-hub":
            from hf_transport import verify_hub

            result = verify_hub(REPO, args.commit, pins + ([remote_only(card)] if card else []))
        else:
            from hf_transport import publish_hub

            result = publish_hub(REPO, files, expected_head=args.expected_head, credential=local_token(), card=card)
        if args.operation != 'check':
            result = release_report(result, pins, remote_only(card) if card else None)
        print(json.dumps(result, sort_keys=True, ensure_ascii=True, allow_nan=False))
        return 0
    except (ImportError, OSError, RuntimeError, ValueError) as error:
        print(f"Arbiter Hub mirror: {error}", file=sys.stderr)
        return 1


if __name__ == "__main__":
    sys.path.insert(0, str(Path(__file__).resolve().parent))
    sys.exit(main())
