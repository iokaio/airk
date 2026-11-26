# SPDX-License-Identifier: Apache-2.0
"""Reconcile closed development publication paths and source hashes to the pinned export."""

from __future__ import annotations

import argparse
import json
from pathlib import Path
import sqlite3
import sys

HERE = Path(__file__).resolve().parent
if str(HERE) not in sys.path:
    sys.path.insert(0, str(HERE))

import common
import search_focus_ab as search


def check_sidecars(database):
    for suffix in ("-wal", "-journal"):
        sidecar = database.with_name(database.name + suffix)
        common.require(not sidecar.exists() or sidecar.stat().st_size == 0, "publication has an uncommitted sidecar")


def source_rows(database):
    check_sidecars(database)
    before = common.sha256_file(database)
    with sqlite3.connect(database.resolve().as_uri() + "?mode=ro&immutable=1", uri=True) as read:
        read.execute("PRAGMA trusted_schema=OFF")
        read.execute("PRAGMA temp_store=MEMORY")
        read.execute("PRAGMA query_only=ON")
        rows = dict(read.execute("SELECT path, lower(hex(source_hash)) FROM files ORDER BY path"))
    check_sidecars(database)
    common.require(common.sha256_file(database) == before, "publication changed during source audit")
    return rows, before


def publication_database(repo, record):
    published = Path(record["data_dir"]).resolve(strict=True)
    _, incarnation, _, _ = record["publication_token"].split(":")
    candidates = [path for path in published.rglob("index.sqlite3") if path.parent.name == incarnation]
    common.require(len(candidates) == 1 and not candidates[0].is_symlink(),
                   f"{repo}: no unique published database for the pinned incarnation")
    return candidates[0]


def audit_repository(repo, record):
    source = Path(record["root"]).resolve(strict=True)
    rows, db_hash = source_rows(publication_database(repo, record))
    files = {path.relative_to(source).as_posix(): path for path in source.rglob("*") if path.is_file()}
    common.require(len(files) == record["snapshot"]["files"] and len(rows) == record["files"],
                   f"{repo}: path counts differ from the closed fixture inventory")
    common.require(all(not path.is_symlink() and path.resolve().is_relative_to(source) for path in files.values()),
                   f"{repo}: export contains a link outside the pinned root")
    outside = set(rows) - set(files)
    atoms = record["denies"]
    common.require(all(len(atom) == 2 and atom[0] in {"exact", "subtree"} for atom in atoms),
                   f"{repo}: unsupported source policy atom")
    denies = {path for path in set(rows) | set(files)
              if any(path == rule or (kind == "subtree" and path.startswith(rule + "/")) for kind, rule in atoms)}
    unexpected_denies = denies & set(rows)
    missing = set(files) - set(rows)
    matches = 0
    mismatches = []
    for path in sorted(set(rows) & set(files)):
        common.require(search.SHA.fullmatch(rows[path]) is not None, "indexed source hash is invalid")
        if common.sha256_file(files[path]) == rows[path]:
            matches += 1
        else:
            mismatches.append(path)
    return {"repository": repo, "source_commit": record["source_commit"],
            "publication_token": record["publication_token"], "database_sha256": db_hash,
            "exported_paths": len(files), "indexed_paths": len(rows), "outside_export": sorted(outside),
            "intentionally_denied": sorted(denies & missing), "indexed_denies": sorted(unexpected_denies),
            "other_absent": sorted(missing - denies), "matching_source_hashes": matches,
            "source_hash_mismatches": mismatches}


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("--handle", type=Path, required=True)
    parser.add_argument("--handle-sha256", required=True)
    parser.add_argument("--output", type=Path, required=True)
    args = parser.parse_args()
    handle = json.loads(search.pinned(args.handle, args.handle_sha256))
    common.require(handle["state"] == "closed" and handle["final_stage_idle"] is True
                   and handle["service_exit_code"] == 0, "source audit requires a clean closed fixture")
    indexes = handle["indexes"]
    common.require(set(indexes) == {"bookbuilder-v2", "munarium"}, "wrong development repository roster")
    records = [audit_repository(repo, record) for repo, record in sorted(indexes.items())]
    search.write_new(args.output, {"schema": "airk.private.census-source-fidelity/1",
                                  "handle_sha256": args.handle_sha256, "repositories": records})


if __name__ == "__main__":
    try:
        main()
    except (common.HarnessError, OSError, ValueError, KeyError, TypeError, sqlite3.Error) as error:
        print(str(error), file=sys.stderr)
        sys.exit(1)
