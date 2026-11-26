# SPDX-License-Identifier: Apache-2.0
"""Compare closed Search controls by canonical FTS postings and exact vector bytes."""

from __future__ import annotations

import argparse
import hashlib
from itertools import groupby
import json
from pathlib import Path
import sqlite3
import sys

HERE = Path(__file__).resolve().parent
if str(HERE) not in sys.path:
    sys.path.insert(0, str(HERE))

import common
import search_focus_ab as search
import source_fidelity_audit as fidelity


def fingerprint(database):
    fidelity.check_sidecars(database)
    before = common.sha256_file(database)
    with sqlite3.connect(database.resolve().as_uri() + "?mode=ro&immutable=1", uri=True) as read:
        read.execute("PRAGMA temp_store=MEMORY")
        read.execute("CREATE VIRTUAL TABLE temp.vocabulary USING fts5vocab(main, fts, instance)")
        read.execute("PRAGMA trusted_schema=OFF")
        read.execute("PRAGMA query_only=ON")
        stable_ids = dict(read.execute("SELECT id, lower(hex(stable_id)) FROM chunks"))
        postings = read.execute("SELECT term, doc, col, offset FROM temp.vocabulary ORDER BY term")
        digest = hashlib.sha256()
        count = 0
        for term, group in groupby(postings, key=lambda row: row[0]):
            canonical = sorted((stable_ids[doc], column, offset) for _, doc, column, offset in group)
            digest.update(json.dumps([term, canonical], separators=(",", ":")).encode("utf-8"))
            digest.update(b"\n")
            count += len(canonical)
    fidelity.check_sidecars(database)
    common.require(common.sha256_file(database) == before, "closed control database changed during comparison")
    vector_files = {file.name: common.sha256_file(file) for file in database.parent.glob("vectors-*.f32")}
    common.require(len(vector_files) == 1, "control requires one committed matrix generation")
    return {"db_sha256": before, "fts_postings": count, "fts_sha256": digest.hexdigest(),
            "chunks": len(stable_ids), "vectors_sha256": next(iter(vector_files.values()))}


def compare(first, second):
    expected = {"bookbuilder-v2", "munarium"}
    common.require(set(first["indexes"]) == set(second["indexes"]) == expected,
                   "control fixture repository roster differs")
    result = {}
    for repo in sorted(expected):
        left, right = first["indexes"][repo], second["indexes"][repo]
        for field in ("source_commit", "pipeline_digest", "files", "denies", "served_mode"):
            common.require(left[field] == right[field], f"{repo}: control {field} differs")
        control = fingerprint(fidelity.publication_database(repo, left))
        alternate = fingerprint(fidelity.publication_database(repo, right))
        keys = ("fts_postings", "fts_sha256", "chunks", "vectors_sha256")
        result[repo] = {"primary_publication": left["publication_token"],
                        "alternate_publication": right["publication_token"],
                        "primary": control, "alternate": alternate,
                        "equal_rank_inputs": all(control[key] == alternate[key] for key in keys)}
    return result


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    for name in ("primary", "alternate"):
        parser.add_argument(f"--{name}", type=Path, required=True)
        parser.add_argument(f"--{name}-sha256", required=True)
    parser.add_argument("--output", type=Path, required=True)
    args = parser.parse_args()
    primary = json.loads(search.pinned(args.primary, args.primary_sha256))
    alternate = json.loads(search.pinned(args.alternate, args.alternate_sha256))
    common.require(all(item["state"] == "closed" and item["final_stage_idle"] is True
                       and item["service_exit_code"] == 0 for item in (primary, alternate)),
                   "comparison requires both closed, idle fixtures")
    search.write_new(args.output, {"schema": "airk.private.census-posting-comparison/1",
                                  "primary_sha256": args.primary_sha256,
                                  "alternate_sha256": args.alternate_sha256,
                                  "results": compare(primary, alternate)})


if __name__ == "__main__":
    try:
        main()
    except (common.HarnessError, OSError, ValueError, KeyError, TypeError, sqlite3.Error) as error:
        print(str(error), file=sys.stderr)
        sys.exit(1)
