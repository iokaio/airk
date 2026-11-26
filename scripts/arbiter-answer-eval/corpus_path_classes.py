# SPDX-License-Identifier: Apache-2.0
"""Count corpus path classes in closed indexes and retained top-40 Search candidates."""

from __future__ import annotations

import argparse
from collections import Counter
import json
from pathlib import Path, PurePosixPath
import sqlite3
import sys

HERE = Path(__file__).resolve().parent
if str(HERE) not in sys.path:
    sys.path.insert(0, str(HERE))

import common
import search_focus_ab as search
import search_shortlist_audit as shortlist
import source_fidelity_audit as fidelity


PAYLOADS = {".bmp", ".dat", ".epub", ".gif", ".ico", ".jar", ".jpeg", ".jpg", ".mp3", ".mp4",
            ".pdf", ".png", ".svg", ".tgz", ".ttf", ".wasm", ".webp", ".woff", ".zip"}
DOCUMENTS = {".adoc", ".md", ".mdx", ".rst", ".txt"}


def path_class(path):
    entry = PurePosixPath(path.lower())
    folders = set(entry.parts[:-1])
    leaf = entry.name
    if folders & {"vendor", "vendors", "third_party", "third-party", "node_modules"}:
        return "vendored"
    if folders & {"generated", "__generated__", "dist", "obj"} or ".generated." in leaf:
        return "generated"
    if folders & {"test", "tests", "__tests__", "spec", "specs"} or any(
        marker in leaf for marker in (".test.", ".spec.", "_test.", "_spec.")
    ):
        return "tests"
    if entry.suffix in PAYLOADS:
        return "bundled_payload"
    if folders & {"doc", "docs", "documentation"} or entry.suffix in DOCUMENTS:
        return "documentation"
    return "source"


def counts_for_database(database):
    fidelity.check_sidecars(database)
    before = common.sha256_file(database)
    with sqlite3.connect(database.resolve().as_uri() + "?mode=ro&immutable=1", uri=True) as read:
        read.execute("PRAGMA trusted_schema=OFF")
        read.execute("PRAGMA temp_store=MEMORY")
        read.execute("PRAGMA query_only=ON")
        files = Counter()
        units = Counter()
        query = "SELECT f.path, count(u.id) FROM files f LEFT JOIN units u ON u.file_id=f.id GROUP BY f.id"
        for path, count in read.execute(query):
            category = path_class(path)
            files[category] += 1
            units[category] += count
    fidelity.check_sidecars(database)
    common.require(common.sha256_file(database) == before, "corpus database changed during the read")
    return dict(files), dict(units), before


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    for name in ("handle", "cohort", "off", "trace"):
        parser.add_argument(f"--{name}", type=Path, required=True)
        parser.add_argument(f"--{name}-sha256", required=True)
    parser.add_argument("--output", type=Path, required=True)
    args = parser.parse_args()
    handle = json.loads(search.pinned(args.handle, args.handle_sha256))
    cohort = json.loads(search.pinned(args.cohort, args.cohort_sha256))
    off = json.loads(search.pinned(args.off, args.off_sha256))
    common.require(handle["state"] == "closed" and handle["final_stage_idle"] is True
                   and handle["service_exit_code"] == 0, "corpus audit requires a clean closed fixture")
    common.require(search.roster(cohort["rows"]) == search.roster(off["rows"])
                   and args.cohort_sha256 == off["cohort_sha256"]
                   and off["identity"]["search_profile"] == "wide_shortlist", "wrong frozen shortlist cohort")
    indexes = off["identity"]["indexes"]
    traces = shortlist.trace_pairs(search.pinned(args.trace, args.trace_sha256), cohort["rows"], indexes)
    results = {}
    for repo, index in indexes.items():
        record = handle["indexes"][index["physical"]]
        common.require(record["publication_token"] == index["publication_token"]
                       and record["pipeline_digest"] == index["pipeline_digest"]
                       and record["source_commit"] == index["source_commit"], "shortlist and corpus inputs differ")
        files, units, db_hash = counts_for_database(fidelity.publication_database(repo, record))
        common.require(sum(files.values()) == record["files"], "closed corpus file counts differ")
        ranks = Counter()
        for (traced_repo, _), trace in traces.items():
            if traced_repo != repo:
                continue
            candidates = trace["candidates"]
            common.require(0 < len(candidates) <= 40, "invalid wide shortlist size")
            ranks.update(path_class(citation["path"]) for citation, _ in candidates)
        results[repo] = {"files": files, "committed_units": units, "top40_candidates": dict(ranks),
                         "publication": record["publication_token"], "database_sha256": db_hash}
    search.write_new(args.output, {
        "schema": "airk.private.corpus-path-classes/1",
        "inputs": {name: getattr(args, f"{name}_sha256") for name in ("handle", "cohort", "off", "trace")},
        "results": results,
        "scope": "Heuristic path classes. Candidate counts use one copy of each retained matched Step 7 shortlist.",
    })


if __name__ == "__main__":
    try:
        main()
    except (common.HarnessError, OSError, ValueError, KeyError, TypeError, sqlite3.Error) as error:
        print(str(error), file=sys.stderr)
        sys.exit(1)
