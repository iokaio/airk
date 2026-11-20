# SPDX-License-Identifier: Apache-2.0
"""Stage complete fixed fd source trees and preserve the copied dependency grants."""
import argparse
import hashlib
import io
import json
from pathlib import Path
import runpy
import struct
import tarfile
import time
import tomllib
from urllib.request import urlopen


HERE = Path(__file__).resolve().parent
audit = runpy.run_path(str(HERE / "contextbench_fd_sources.py"))
OUTPUT = audit["CAMPAIGN"] / "contextbench-fd-whole-r1"
RECEIPT = "6a2fa427df2d0680a7f27990575d272504d8c7f1d3509d22cca06c95ed952c49"
GRANTS = {"LICENSE-APACHE": "c71d239df91726fc519c6eb72d318ec65820627232b2f796219e87dcf35d0ab4",
          "LICENSE-MIT": "f9c4c77baa3828004ee54b8a4f2db2e88ed44a6237a493965bf551fac0fcb62d"}
require, sha = audit["require"], audit["sha"]


def layout(revisions):
    outputs = []
    for revision in revisions:
        base = revision["base"]
        outputs.extend(f"sources/{base}/{path}" for path, mode, *_ in revision["entries"] if mode == "100644")
        outputs.extend(f"notices/{base}/lscolors/{name}" for name in GRANTS)
    outputs.append("manifest.private.json")
    directories = sorted({p.as_posix() for name in outputs for p in Path(name).parents if str(p) != "."},
                         key=lambda p: (len(Path(p).parts), p))
    return directories, outputs


def png_origin(raw):
    require(raw.startswith(b"\x89PNG\r\n\x1a\n"), "invalid logo header")
    pos = 8
    chunks, texts = [], []
    while pos < len(raw):
        require(pos + 12 <= len(raw), "incomplete logo chunk")
        size = struct.unpack_from(">I", raw, pos)[0]
        tag = raw[pos + 4:pos + 8]
        require(pos + size + 12 <= len(raw), "invalid logo chunk extent")
        if tag == b"tEXt":
            texts.append(raw[pos + 8:pos + 8 + size])
        chunks.append(tag)
        pos += size + 12
    require(chunks == [b"IHDR", b"pHYs", b"tEXt", b"IDAT", b"IDAT", b"IEND"]
            and texts == [b"Software\0www.inkscape.org"], "PNG ancillary provenance changed")
    return "upstream_project_logo_with_inkscape_generator_and_dual_root_grants"


def dependency(raw, source):
    lock = tomllib.loads(raw.decode())
    require(set(lock) == {"version", "package"} and all(set(p) <= {"name", "version", "source", "checksum", "dependencies"}
                                                        for p in lock["package"]), "lock contains more than package metadata")
    package, = [p for p in lock["package"] if p["name"] == "lscolors"]
    version = package["version"]
    require(version in {"0.12.0", "0.15.0"} and b"Copied from LsColors::style_for_path_with_metadata()" in source,
            "unexpected dependency source attribution")
    url = f"https://static.crates.io/crates/lscolors/lscolors-{version}.crate"
    with urlopen(url, timeout=25) as response:
        require(response.geturl() == url and response.status == 200, "unexpected dependency endpoint")
        tar = response.read(500001)
    require(len(tar) <= 500000 and sha(tar) == package["checksum"], "published crate checksum differs")
    prefix = f"lscolors-{version}/"
    with tarfile.open(fileobj=io.BytesIO(tar), mode="r:gz") as bundle:
        def member(name):
            entry = bundle.getmember(prefix + name)
            require(entry.isfile() and entry.size <= 100000, "unexpected crate member")
            with bundle.extractfile(entry) as stream:
                return stream.read()
        require(tomllib.loads(member("Cargo.toml").decode())["package"]["license"] == "MIT/Apache-2.0",
                "different dependency license declaration")
        require(b"fn style_for_path_with_metadata" in member("src/lib.rs"), "declared upstream symbol missing")
        grants = {name: member(name) for name in GRANTS}
        require({n: sha(data) for n, data in grants.items()} == GRANTS, "published dependency grants differ")
        legal_names = [name for name in bundle.getnames() if name.startswith(prefix)
                       and Path(name).name.casefold().startswith(("notice", "copying", "license"))]
        require(set(legal_names) == {prefix + name for name in GRANTS}, "additional governing notices require review")
    return {"package": "lscolors", "version": version, "registry_archive_sha256": package["checksum"],
            "source": url, "declared_license": "MIT/Apache-2.0", "grants": GRANTS}, grants


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("--layout", action="store_true")
    args = parser.parse_args()
    revisions = audit["revisions"]()
    directories, outputs = layout(revisions)
    if args.layout:
        print(json.dumps({"directories": directories, "files": outputs}))
        return
    raw = (OUTPUT / "source-audit.private.json").read_bytes()
    require(sha(raw) == RECEIPT, "source-rights census changed")
    original = {r["base"]: r for r in json.loads(raw)["revisions"]}
    started = time.monotonic()
    manifest = {"schema": "airk.private.contextbench-fd-whole-source-roots/1", "audit": RECEIPT,
                "source_tree_roster": audit["ROSTER_SHA"], "revisions": []}
    require(all((OUTPUT / d).resolve(strict=True) == OUTPUT / d for d in directories)
            and not any((OUTPUT / p).exists() for p in outputs), "fresh preflighted directories required")
    for revision in revisions:
        base = revision["base"]
        bodies = audit["fetch"](revision)
        receipt = original[base]
        require(audit["inspect"](revision, bodies) == receipt, "frozen whole-tree observation differs")
        dep, grants = dependency(bodies["Cargo.lock"], bodies["src/dir_entry.rs"])
        for name, body in grants.items():
            with (OUTPUT / "notices" / base / "lscolors" / name).open("xb") as dst:
                dst.write(body)
        files = []
        for row in receipt["files"]:
            path = row["path"]
            body = bodies[path]
            disposition = row["disposition"]
            if path == "Cargo.lock":
                disposition = "original_published_lock_metadata_no_dependency_implementation"
            elif path == "src/dir_entry.rs":
                disposition = "attributed_lscolors_routine_with_registry_pinned_dual_grants"
            elif path == "doc/logo.png":
                disposition = png_origin(body)
            require(not disposition.startswith("withhold"), "unresolved source right: " + path)
            with (OUTPUT / "sources" / base / path).open("xb") as dst:
                dst.write(body)
            files.append({"path": path, "oid": row["oid"], "sha256": row["sha256"], "bytes": row["bytes"],
                          "disposition": disposition})
        manifest["revisions"].append({"repo": revision["repo"], "base": base, "tree": revision["tree"],
                                      "source_root": "sources/" + base, "root_grants": ["LICENSE-APACHE", "LICENSE-MIT"],
                                      "dependency_origin": dep, "files": files})
    result = (json.dumps(manifest, sort_keys=True, indent=2) + "\n").encode()
    with (OUTPUT / "manifest.private.json").open("xb") as stream:
        stream.write(result)
    print(json.dumps({"staged_files": sum(len(r["files"]) for r in manifest["revisions"]),
                      "dependency_grants": 4, "manifest_sha256": sha(result), "elapsed_seconds": time.monotonic() - started}))


if __name__ == "__main__":
    main()
