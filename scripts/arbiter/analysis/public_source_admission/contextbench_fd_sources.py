# SPDX-License-Identifier: Apache-2.0
"""Verify both complete fd Git trees and classify every blob before native staging."""
import argparse
import hashlib
import io
import json
from pathlib import Path, PurePosixPath
import re
import runpy
import tarfile
import time
import tomllib
from urllib.request import Request, urlopen


HERE = Path(__file__).resolve().parent
CAMPAIGN = HERE.parents[3] / "artifacts/v190/final-plan/exec-20261116T052942Z"
ROSTER = CAMPAIGN / "contextbench-full-revision-trees-r1.private.json"
ROSTER_SHA = "1e500bf49108bc64e3616459603dbc23b56f0ca478490ce00d6728177156531a"
REPO = "sharkdp/fd"
BASES = {"93cdb2628e89dd5831eee22b8df697aea00eca3b", "cbd11d8a45dc80392c5f1be9679051085e6a3376"}
BLOB_CEILING = 2 << 20


def require(ok, message):
    if not ok:
        raise ValueError(message)


def sha(raw):
    return hashlib.sha256(raw).hexdigest()


def revisions():
    raw = ROSTER.read_bytes()
    require(sha(raw) == ROSTER_SHA, "source-revision census differs")
    selected = [r for r in json.loads(raw)["revisions"] if r["repo"] == REPO]
    require({r["base"] for r in selected} == BASES and len(selected) == 2, "expected the two fd bases")
    verify = runpy.run_path(str(HERE / "contextbench_full_tree_inventory.py"))["verify_tree"]
    for revision in selected:
        require(revision["counts_by_mode"] == {"040000": 11, "100644": 48} and len(revision["entries"]) == 59,
                "unexpected complete tree roster")
        verify(revision["entries"], revision["tree"])
    return selected


def identifier(raw):
    return hashlib.sha1(b"blob " + str(len(raw)).encode() + b"\0" + raw).hexdigest()


def fetch(revision):
    base = revision["base"]
    origin = f"https://codeload.github.com/{REPO}/tar.gz/{base}"
    with urlopen(Request(origin, headers={"User-Agent": "AIRK-private-source-audit"}), timeout=25) as response:
        require(response.status == 200 and response.geturl() == origin, "unexpected source endpoint")
        archive = response.read(BLOB_CEILING + 1)
    require(0 < len(archive) <= BLOB_CEILING, "unexpected source archive size")
    expected = {p: (mode, oid, size) for p, mode, oid, size in revision["entries"]}
    prefix = f"fd-{base}"
    visited = set()
    bodies = {}
    with tarfile.open(fileobj=io.BytesIO(archive), mode="r|gz") as bundle:
        for entry in bundle:
            if entry.name == prefix and entry.isdir():
                continue
            require(entry.name.startswith(prefix + "/"), "unexpected tar prefix")
            path = entry.name[len(prefix) + 1:]
            require(path in expected and path not in visited and str(PurePosixPath(path)) == path
                    and all(part not in {".", ".."} and not re.search(r'[<>:"\\|?*]', part)
                            for part in PurePosixPath(path).parts), "unexpected archive path")
            visited.add(path)
            mode, oid, size = expected[path]
            if mode == "040000":
                require(entry.isdir(), "expected a Git directory")
                continue
            require(mode == "100644" and entry.isfile() and entry.size == size and size <= BLOB_CEILING,
                    "unexpected file type or size")
            with bundle.extractfile(entry) as src:
                raw = src.read(size + 1)
            require(len(raw) == size and identifier(raw) == oid, "Git blob identity differs")
            bodies[path] = raw
    require(visited == expected.keys() and len(bodies) == 48, "Git archive misses a source entry")
    return bodies


def inspect(revision, bodies):
    rights = runpy.run_path(str(HERE / "audit_contextbench_confirmation_rights.py"))
    license_expression = tomllib.loads(bodies["Cargo.toml"].decode())["package"]["license"]
    require(license_expression in rights["DUAL"] and b"Apache License" in bodies["LICENSE-APACHE"]
            and b"Permission is hereby granted" in bodies["LICENSE-MIT"], "base dual grant differs")
    grants = {"LICENSE-MIT", "LICENSE-APACHE"}
    files = []
    for path in sorted(bodies):
        raw = bodies[path]
        if path in grants:
            disposition = "native_original_grant"
        elif path == "doc/logo.png":
            disposition = "withhold_binary_artwork_provenance"
        else:
            signals = rights["file_signals"](raw)
            disposition = rights["private_source_basis"](path, None, license_expression, grants, signals, [])
        references = []
        if not path.endswith(".png") and path not in grants:
            for line in raw.decode().splitlines():
                if re.search(r'(?i)(copyright|ported from|copied from|adapted from|license|attribution|artwork|logo by|screencast by)', line):
                    references.append(line.strip()[:180])
        files.append({"path": path, "oid": identifier(raw), "bytes": len(raw), "sha256": sha(raw),
                      "disposition": disposition, "rights_lines": references[:10]})
    return {"repo": REPO, "base": revision["base"], "tree": revision["tree"], "expression": license_expression,
            "files": files}


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.parse_args()
    started = time.monotonic()
    rows = [inspect(revision, fetch(revision)) for revision in revisions()]
    print(json.dumps({"source_roster_sha256": ROSTER_SHA, "elapsed_seconds": time.monotonic() - started,
                      "revisions": rows}, sort_keys=True, ensure_ascii=True))


if __name__ == "__main__":
    main()
