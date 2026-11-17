# SPDX-License-Identifier: Apache-2.0
"""Copy only pinned native publication slices; no Profile parsing or authority."""
import argparse
import hashlib
import json
import os
from pathlib import Path
import stage_offline_bundles as owner


def copy_slice(source, destination, offset, size, pin, anchor):
    before = owner.no_alias(source, anchor, regular=True).stat()
    hashed, total = hashlib.sha256(), 0
    with source.open("rb") as reader, destination.open("xb") as writer:
        reader.seek(offset)
        while total < size:
            block = reader.read(min(4 << 20, size - total))
            if not block:
                raise ValueError("native part source shortened")
            writer.write(block)
            hashed.update(block)
            total += len(block)
        writer.flush()
        os.fsync(writer.fileno())
    after = owner.no_alias(source, anchor, regular=True).stat()
    if ((before.st_dev, before.st_ino, before.st_size) != (after.st_dev, after.st_ino, after.st_size)
            or total != size or hashed.hexdigest() != pin or owner.digest(destination) != pin):
        raise ValueError("native source slice/destination differs")


def stage(plan_path, plan_pin, sources_path, sources_pin, publication_path, publication_pin, output, anchor):
    anchor, output = Path(anchor).absolute(), Path(output).absolute()
    if output.exists() or ".." in output.parts or not output.is_relative_to(anchor):
        raise ValueError("fresh output under named artifact anchor required")
    owner.no_alias(output.parent, anchor)
    _, plan = owner.pinned_json(plan_path, plan_pin, anchor)
    _, sources = owner.pinned_json(sources_path, sources_pin, anchor)
    publication_raw, publication = owner.pinned_json(publication_path, publication_pin, anchor)
    if (plan.get("schema") != "airk.arbiter.offline-bundle-plan/1"
            or sources.get("schema") != "airk.private-offline-bundle-sources/1"
            or publication.get("schema") != "airk.arbiter.publication/1"
            or sources.get("plan_sha256") != plan_pin
            or sources.get("publication_sha256") != publication_pin
            or not owner.HEX.fullmatch(plan.get("catalog_sha256", ""))
            or publication.get("compiled_catalog_sha256") != plan.get("catalog_sha256")
            or publication.get("tag") != plan.get("tag")
            or not owner.HEX.fullmatch(plan.get("promotion_decision_sha256", ""))):
        raise ValueError("exact native plan/publication/private-map closure required")
    bundles = plan.get("bundles", [])
    identities = [row.get("profile", "") for row in bundles]
    if (len(bundles) != 3 or len(set(identities)) != 3
            or any(not owner.HEX.fullmatch(identity) for identity in identities)
            or len(publication.get("profiles", [])) != 3
            or set(publication.get("profiles", [])) != set(identities)
            or set(sources.get("files", {})) != set(identities)):
        raise ValueError("exact three actual native bundle identities required")
    logical = {}
    for bundle in bundles:
        identity = bundle["profile"]
        if not owner.HEX.fullmatch(bundle.get("qualification", "")):
            raise ValueError("actual final qualification pin missing")
        assets = bundle.get("files", [])
        names = [row.get("name") for row in assets]
        mapping = sources["files"][identity]
        if (not 1 <= len(assets) <= 128 or any(not owner.safe_name(name) for name in names)
                or len(set(names)) != len(names) or set(names) != set(mapping)):
            raise ValueError("native logical closure differs")
        for asset in assets:
            if (type(asset.get("bytes")) is not int or not 0 < asset["bytes"] <= 16 << 30
                    or not owner.HEX.fullmatch(asset.get("sha256", ""))):
                raise ValueError("native logical file pin invalid")
            source = owner.no_alias(mapping[asset["name"]], anchor, regular=True)
            if source.stat().st_size != asset["bytes"]:
                raise ValueError("native full source length differs")
            logical[(identity, asset["name"])] = (source, asset)
    parts = sources.get("parts", {})
    pins = publication.get("assets", {})
    if not 1 <= len(parts) <= 16384 or set(parts) != set(pins):
        raise ValueError("native publication/slice census differs")
    grouped, prepared = {}, []
    for name, row in parts.items():
        if not owner.safe_name(name):
            raise ValueError("unsafe native part filename")
        key = (row.get("profile"), row.get("asset_name"))
        if key not in logical:
            raise ValueError("native slice names foreign logical asset")
        offset, size = row.get("offset"), row.get("bytes")
        pin = row.get("sha256", "")
        if (type(offset) is not int or offset < 0 or type(size) is not int or not 0 < size <= 1 << 30
                or not owner.HEX.fullmatch(pin) or type(pins[name].get("bytes")) is not int
                or pins[name] != {"bytes": size, "sha256": pin}
                or offset + size > logical[key][1]["bytes"]):
            raise ValueError("native slice/part pins or bounds differ")
        grouped.setdefault(key, []).append((offset, size))
        prepared.append((name, logical[key][0], offset, size, pin))
    # Every referenced full logical source is covered once by its native slices.
    # Duplicate logical assets sharing the same native part filename need no
    # second source; native canonical publication already verified that identity.
    for key, spans in grouped.items():
        cursor = 0
        for offset, size in sorted(spans):
            if offset != cursor:
                raise ValueError("native slice gap/overlap")
            cursor += size
        source, asset = logical[key]
        if cursor != asset["bytes"] or owner.digest(source) != asset["sha256"]:
            raise ValueError("native full source coverage/hash differs")
    covered_assets = {(logical[key][1]["bytes"], logical[key][1]["sha256"]) for key in grouped}
    if any((asset["bytes"], asset["sha256"]) not in covered_assets for _, asset in logical.values()):
        raise ValueError("native publication omits a distinct logical asset")
    output.mkdir()
    part_root = output / "parts"
    part_root.mkdir()
    rows = []
    for name, source, offset, size, pin in sorted(prepared):
        copy_slice(source, part_root / name, offset, size, pin, anchor)
        rows.append({"name": name, "bytes": size, "sha256": pin})
    owner.write_new(output / "publication.json", publication_raw)
    receipt = {"schema": "airk.native-release-part-staging/1", "complete": True,
               "plan_sha256": plan_pin, "publication_sha256": publication_pin,
               "promotion_decision_sha256": plan["promotion_decision_sha256"],
               "source_script_sha256": owner.digest(__file__), "source_owner_sha256":owner.digest(owner.__file__), "parts": rows,
               "scope": "Exact native byte projection only; no model qualification or publication performed."}
    owner.write_new(output / "staging-receipt.json", (json.dumps(receipt, indent=2) + "\n").encode())
    return receipt


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    for name in ("plan", "sources", "publication", "output", "anchor"):
        parser.add_argument("--" + name, type=Path, required=True)
    for name in ("plan", "sources", "publication"):
        parser.add_argument("--" + name + "-sha256", required=True)
    args = parser.parse_args()
    stage(args.plan, args.plan_sha256, args.sources, args.sources_sha256,
          args.publication, args.publication_sha256, args.output, args.anchor)


if __name__ == "__main__":
    main()
