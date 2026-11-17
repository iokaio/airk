# SPDX-License-Identifier: Apache-2.0
"""Private exact native file projection: fresh offline ZIPs and release parts."""
import argparse
import hashlib
import json
from pathlib import Path
import stat
import time
import zipfile

import stage_offline_bundles as owner
import stage_release_parts as release


def import_text(identity):
    return (
        "Use the matching qualified AIRK native pair. Read `airkctl arbiter status`\n"
        "and use its current configuration revision for N. Extract this complete ZIP\n"
        "to a fresh local directory; keep all files in the exact models child.\n"
        f"Profile: {identity}\n"
        f"airkctl arbiter import --expected-revision N --profile {identity} "
        f"--directory ABSOLUTE_PATH_TO_EXTRACTED_MODELS_CHILD\n"
        f"The directory argument is the original absolute path to models/{identity}.\n"
        "Pass a quoted argument when the path contains spaces. Native import verifies\n"
        "every asset read-only against the embedded qualified profile. No private\n"
        "Profile file is required. Import does not initialize configuration, enable\n"
        "a stage, grant indexing consent, or enroll any index. Those are separate\n"
        "explicit operations. CPU/platform/memory admission still applies.\n"
        "This archive construction alone grants no runtime or release qualification.\n"
    ).encode("utf-8")


def notices(path, pin, plan_pin, identities, anchor):
    _, value = owner.pinned_json(path, pin, anchor)
    return validate_notices(value, plan_pin, identities, anchor)


def validate_notices(value, plan_pin, identities, anchor):
    if (value.get("schema") != "airk.private-offline-notice-mapping/1"
            or value.get("plan_sha256") != plan_pin
            or set(value.get("files", {})) != set(identities)):
        raise ValueError("exact pinned external notice mapping required")
    prepared = {}
    for identity, rows in value["files"].items():
        if not isinstance(rows, list) or not 2 <= len(rows) <= 64:
            raise ValueError("bounded full license/readme notice files required")
        names, roles, documents = set(), set(), []
        for row in rows:
            name = row.get("name")
            size, expected = row.get("bytes"), row.get("sha256", "")
            if (not owner.safe_name(name) or name in names
                    or row.get("role") not in ("license", "readme", "notice")
                    or type(size) is not int or not 0 < size <= 32 << 20
                    or not owner.HEX.fullmatch(expected)):
                raise ValueError("unsafe/duplicate/unbounded notice pin")
            source = owner.no_alias(row["path"], anchor, regular=True)
            if source.stat().st_size != size or owner.digest(source) != expected:
                raise ValueError("full pinned notice differs")
            names.add(name)
            roles.add(row["role"])
            documents.append(("notices/" + name, source, size, expected))
        if not {"license", "readme"} <= roles:
            raise ValueError("separate full license and publisher readme required")
        prepared[identity] = documents
    return prepared


def add_file(archive, name, source, size, pin, anchor):
    source = owner.no_alias(source, anchor, regular=True)
    before = source.stat()
    hashed, total = hashlib.sha256(), 0
    info = zipfile.ZipInfo(name, date_time=(1980, 1, 1, 0, 0, 0))
    info.create_system = 3
    info.external_attr = (stat.S_IFREG | 0o644) << 16
    info.compress_type = zipfile.ZIP_STORED
    with source.open("rb") as reader, archive.open(info, "w", force_zip64=True) as writer:
        while block := reader.read(min(owner.CHUNK, size + 1 - total)):
            total += len(block)
            if total > size:
                raise ValueError("archive source grew")
            hashed.update(block)
            writer.write(block)
    after = owner.no_alias(source, anchor, regular=True).stat()
    if (total != size or hashed.hexdigest() != pin
            or (before.st_dev, before.st_ino, before.st_size, before.st_mtime_ns)
            != (after.st_dev, after.st_ino, after.st_size, after.st_mtime_ns)):
        raise ValueError("archive source continuity/hash differs")


def archive_bundle(path, identity, files, documents, offline, anchor):
    entries = [(f"models/{identity}/{row['name']}",
                offline / "models" / identity / row["name"], row["bytes"], row["sha256"])
               for row in files] + documents
    instruction = import_text(identity)
    expected = {name: (size, pin) for name, _, size, pin in entries}
    expected["IMPORT.txt"] = (len(instruction), hashlib.sha256(instruction).hexdigest())
    with zipfile.ZipFile(path, "x", compression=zipfile.ZIP_STORED, allowZip64=True) as archive:
        archive.writestr("IMPORT.txt", instruction)
        for name, source, size, pin in entries:
            add_file(archive, name, source, size, pin, anchor)
    with zipfile.ZipFile(path, "r") as archive:
        if len(archive.infolist()) != len(expected) or set(archive.namelist()) != set(expected):
            raise ValueError("archive exact entry census differs")
        for info in archive.infolist():
            size, pin = expected[info.filename]
            if info.file_size != size or info.compress_type != zipfile.ZIP_STORED:
                raise ValueError("archive entry bounds differ")
            hashed = hashlib.sha256()
            with archive.open(info) as entry:
                while block := entry.read(owner.CHUNK):
                    hashed.update(block)
            if hashed.hexdigest() != pin:
                raise ValueError("archive read-back digest differs")
    owner.no_alias(path, anchor, regular=True)
    return {"name": path.name, "profile": identity, "bytes": path.stat().st_size,
            "sha256": owner.digest(path), "entries": len(expected)}


def package(plan, plan_pin, sources, sources_pin, publication, publication_pin,
            notice_map, notice_pin, output, anchor):
    anchor, output = Path(anchor).absolute(), Path(output).absolute()
    if output.exists() or ".." in output.parts or not output.is_relative_to(anchor):
        raise ValueError("fresh output under named artifact anchor required")
    owner.no_alias(output.parent, anchor)
    _, projection = owner.pinned_json(plan, plan_pin, anchor)
    bundles = projection.get("bundles", [])
    identities = [row.get("profile", "") for row in bundles]
    if len(identities) != 3 or len(set(identities)) != 3 or any(not owner.HEX.fullmatch(x) for x in identities):
        raise ValueError("exact three final native bundles required")
    docs = notices(notice_map, notice_pin, plan_pin, identities, anchor)
    started = time.monotonic()
    output.mkdir()
    staged = owner.stage(plan, plan_pin, sources, sources_pin, output / "offline", anchor)
    parts = release.stage(plan, plan_pin, sources, sources_pin, publication, publication_pin,
                          output / "release-parts", anchor)
    archive_root = output / "archives"
    archive_root.mkdir()
    rows = []
    for bundle in bundles:
        identity = bundle["profile"]
        rows.append(archive_bundle(archive_root / f"airk-arbiter-{identity}.zip", identity,
                                   bundle["files"], docs[identity], output / "offline", anchor))
    owner.write_new(archive_root / "SHA256SUMS", "".join(
        f"{row['sha256']}  {row['name']}\n" for row in rows).encode("ascii"))
    receipt = {"schema": "airk.private-offline-package-construction/1", "complete": True,
               "plan_sha256": plan_pin, "source_map_sha256": sources_pin,
               "publication_sha256": publication_pin, "notice_map_sha256": notice_pin,
               "source_script_sha256": owner.digest(__file__),
               "stager_sha256": owner.digest(owner.__file__),
               "parts_script_sha256": owner.digest(release.__file__),
               "archives": rows, "release_parts": len(parts["parts"]),
               "logical_bundles": len(staged["bundles"]),
               "elapsed_seconds": time.monotonic() - started,
               "qualified": False, "published": False,
               "scope": "Exact file construction only; native import and qualification remain separate gates."}
    owner.write_new(output / "packaging-receipt.json", (json.dumps(receipt, indent=2) + "\n").encode("utf-8"))
    return receipt


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    for name in ("plan", "sources", "publication", "notices", "output", "anchor"):
        parser.add_argument("--" + name, type=Path, required=True)
    for name in ("plan", "sources", "publication", "notices"):
        parser.add_argument("--" + name + "-sha256", required=True)
    args = parser.parse_args()
    print(json.dumps(package(args.plan, args.plan_sha256, args.sources, args.sources_sha256,
                             args.publication, args.publication_sha256, args.notices, args.notices_sha256,
                             args.output, args.anchor)))


if __name__ == "__main__":
    main()
