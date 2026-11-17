# SPDX-License-Identifier: Apache-2.0
"""Stage only a pinned Rust-owned final file projection; never constructs Profiles."""
import argparse
import hashlib
import json
import os
from pathlib import Path
import re
import stat
import time

CHUNK = 4 * 1024 * 1024
HEX = re.compile(r"[0-9a-f]{64}\Z")


def digest(path):
    result = hashlib.sha256()
    with Path(path).open("rb") as source:
        while block := source.read(CHUNK):
            result.update(block)
    return result.hexdigest()


def no_alias(path, anchor, regular=False):
    path, anchor = Path(path).absolute(), Path(anchor).absolute()
    if ".." in path.parts or not path.is_relative_to(anchor):
        raise ValueError("path leaves the explicitly named artifact anchor")
    for item in (path, *path.parents):
        info = item.lstat()
        if stat.S_ISLNK(info.st_mode) or getattr(info, "st_file_attributes", 0) & 0x400:
            raise ValueError("symlink/reparse path refused")
        if item == path and regular and (not stat.S_ISREG(info.st_mode) or info.st_nlink != 1):
            raise ValueError("nonregular or hardlinked payload refused")
        if item == anchor:
            break
    return path


def pinned_json(path, pin, anchor):
    path = no_alias(path, anchor, regular=True)
    with path.open("rb") as source:
        raw = source.read((2 << 20) + 1)
    if len(raw) > 2 << 20 or not HEX.fullmatch(pin) or hashlib.sha256(raw).hexdigest() != pin:
        raise ValueError("native projection/source-map pin or metadata bound differs")
    return raw, json.loads(raw)


def safe_name(name):
    return (isinstance(name, str) and 0 < len(name) <= 128 and name not in (".", "..")
            and all(c.isascii() and (c.isalnum() or c in "._-") for c in name))


def copy_exact(source, target, size, pin, anchor):
    source = no_alias(source, anchor, regular=True)
    if type(size) is not int or not 0 < size <= 16 << 30 or not HEX.fullmatch(pin):
        raise ValueError("invalid projected payload pin")
    before = source.stat()
    if before.st_size != size or target.exists():
        raise ValueError("source length/fresh target differs")
    hashed, total = hashlib.sha256(), 0
    # Exclusive final-name creation never replaces a raced foreign file. A
    # failed fresh output has no completion receipt and remains incomplete.
    with source.open("rb") as reader, target.open("xb") as writer:
        while block := reader.read(min(CHUNK, size + 1 - total)):
            total += len(block)
            hashed.update(block)
            writer.write(block)
            if total > size:
                break
        writer.flush()
        os.fsync(writer.fileno())
    after = no_alias(source, anchor, regular=True).stat()
    if ((before.st_dev, before.st_ino, before.st_size) != (after.st_dev, after.st_ino, after.st_size)
            or total != size or hashed.hexdigest() != pin or digest(target) != pin):
        raise ValueError("source/copy changed or complete payload digest differs")


def write_new(path, raw):
    with path.open("xb") as output:
        output.write(raw)
        output.flush()
        os.fsync(output.fileno())


def stage(plan_path, plan_pin, sources_path, sources_pin, output, anchor):
    anchor, output = Path(anchor).absolute(), Path(output).absolute()
    if output.exists() or ".." in output.parts or not output.is_relative_to(anchor):
        raise ValueError("fresh output under named artifact anchor required")
    no_alias(output.parent, anchor)
    plan_raw, plan = pinned_json(plan_path, plan_pin, anchor)
    _, sources = pinned_json(sources_path, sources_pin, anchor)
    if (plan.get("schema") != "airk.arbiter.offline-bundle-plan/1"
            or sources.get("schema") != "airk.private-offline-bundle-sources/1"
            or sources.get("plan_sha256") != plan_pin
            or not HEX.fullmatch(plan.get("catalog_sha256", ""))
            or not HEX.fullmatch(plan.get("promotion_decision_sha256", ""))):
        raise ValueError("actual final Rust projection and matching private source map required")
    bundles = plan.get("bundles", [])
    if len(bundles) != 3:
        raise ValueError("exact three native-projected selected bundles required")
    identities, prepared = set(), []
    for bundle in bundles:
        identity = bundle.get("profile", "")
        if (not HEX.fullmatch(identity) or bundle.get("directory") != identity or identity in identities
                or not HEX.fullmatch(bundle.get("qualification", ""))):
            raise ValueError("duplicate/unsafe native bundle directory")
        identities.add(identity)
        files = bundle.get("files", [])
        names = [row.get("name") for row in files]
        mapping = sources.get("files", {}).get(identity, {})
        if (not 1 <= len(files) <= 128 or any(not safe_name(name) for name in names)
                or len(set(names)) != len(names) or set(names) != set(mapping)):
            raise ValueError("projected logical closure/source map differs")
        for row in files:
            source = no_alias(mapping[row["name"]], anchor, regular=True)
            if type(row.get("bytes")) is not int or not 0 < row["bytes"] <= 16 << 30 or not HEX.fullmatch(row.get("sha256", "")):
                raise ValueError("invalid native projected asset")
            if source.stat().st_size != row["bytes"]:
                raise ValueError("source length differs before staging")
        prepared.append((identity, files, mapping))
    if set(sources.get("files", {})) != identities:
        raise ValueError("unknown/missing source bundle")
    started = time.monotonic()
    output.mkdir()
    (output / "models").mkdir()
    rows = []
    for identity, files, mapping in prepared:
        destination = output / "models" / identity
        destination.mkdir()
        for row in files:
            copy_exact(mapping[row["name"]], destination / row["name"], row["bytes"], row["sha256"], anchor)
        if {p.name for p in destination.iterdir()} != {row["name"] for row in files}:
            raise ValueError("final model directory contains an unprojected entry")
        rows.append({"directory": "models/" + identity, "files": files})
    # Only public native projection and payload pins leave this helper. The
    # private source map, proof bodies, corpus, cache and service state do not.
    write_new(output / "offline-bundle-plan.json", plan_raw)
    write_new(output / "IMPORT.txt", (
        "Use the matching tested AIRK native pair and read `airkctl arbiter status`.\n"
        "Import one exact child directory with its native profile SHA:\n"
        "airkctl arbiter import --expected-revision N --profile SHA --directory ABSOLUTE\n"
        "The child models/SHA directory contains only compiled logical assets.\n"
        "Import does not enable/enroll a stage. Native platform/CPU/memory admission still applies.\n"
        "This staging receipt grants no runtime qualification or publication authority.\n"
    ).encode())
    receipt = {"schema": "airk.offline-bundle-staging/1", "complete": True,
               "native_plan_sha256": plan_pin, "private_source_map_sha256": sources_pin,
               "source_script_sha256": digest(__file__), "bundles": rows,
               "elapsed_seconds": time.monotonic() - started,
               "scope": "Exact copied native projection only; no new qualification, native import or publication performed."}
    write_new(output / "staging-receipt.json", (json.dumps(receipt, indent=2) + "\n").encode())
    return receipt


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    for name in ("plan", "sources", "output", "anchor"):
        parser.add_argument("--" + name, type=Path, required=True)
    parser.add_argument("--plan-sha256", required=True)
    parser.add_argument("--sources-sha256", required=True)
    args = parser.parse_args()
    print(json.dumps(stage(args.plan, args.plan_sha256, args.sources, args.sources_sha256, args.output, args.anchor)))


if __name__ == "__main__":
    main()
