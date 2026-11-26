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
import followup_search_replay as followup
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


def audit_read_replies(records, indexes, cohort, pass_index):
    rows = []
    for scenario, calls in followup.recorded_rows(cohort, pass_index):
        repo = scenario["repo"]
        physical = "bookbuilder-v2" if repo == "bookbuilder" else repo
        record = indexes[physical]
        verified = records[physical]
        source = Path(record["root"]).resolve(strict=True)
        omitted = set(verified["intentionally_denied"]) | set(verified["other_absent"])
        for call in calls:
            if call["tool"] != "airk_read_file":
                continue
            report = {"repo": repo, "slot": scenario["slot"], "ordinal": call["ordinal"],
                      "result_sha256": call["recorded_result_sha256"], "ranges": []}
            rows.append(report)
            if not call["recorded_ok"]:
                report["status"] = "recorded_error"
                continue
            raw = search.pinned(Path(call["recorded_result_path"]), call["recorded_result_sha256"])
            body = common.envelope_body(json.loads(raw))
            for hit in body.get("hits", []):
                citation = hit["citation"]
                relative = citation["path"]
                first, last = citation["start_line"], citation["end_line"]
                span = {"path": relative, "first": first, "last": last}
                report["ranges"].append(span)
                path = (source / relative).resolve(strict=True)
                requested = call["arguments"]
                common.require(path.is_relative_to(source) and relative not in omitted,
                               "archived native read refers outside the verified indexed export")
                common.require(requested["path"] == relative
                               and requested["start_line"] <= first <= last <= requested["end_line"],
                               "archived native read returned a range outside the request")
                try:
                    data = path.read_bytes()
                    text = data.decode("utf-8")
                except UnicodeError:
                    span["status"] = "extracted_or_non_utf8"
                    continue
                if b"\0" in data:
                    span["status"] = "extracted_or_non_utf8"
                    continue
                lines = text.split("\n")
                fragments = [line + "\n" for line in lines[:-1]]
                if lines[-1]:
                    fragments.append(lines[-1])
                expected = "".join(fragments[first - 1:last])
                actual = hit["text"]
                truncated = bool(hit.get("truncated") or body.get("truncated"))
                matched = last <= len(fragments) and (expected.startswith(actual) if truncated else expected == actual)
                span.update(status=("prefix" if truncated else "exact") if matched else "mismatch",
                            expected_sha256=common.sha256_bytes(expected.encode("utf-8")),
                            actual_sha256=common.sha256_bytes(actual.encode("utf-8")))
            report["status"] = "read" if report["ranges"] else "no_text_hit"
    return rows


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("--handle", type=Path, required=True)
    parser.add_argument("--handle-sha256", required=True)
    parser.add_argument("--output", type=Path, required=True)
    for name in ("cohort", "pass-index"):
        parser.add_argument(f"--{name}", type=Path)
        parser.add_argument(f"--{name}-sha256")
    args = parser.parse_args()
    archive = (args.cohort, args.cohort_sha256, args.pass_index, args.pass_index_sha256)
    common.require(not any(archive) or all(archive), "archived reads require a cohort and pass index with both pins")
    handle = json.loads(search.pinned(args.handle, args.handle_sha256))
    common.require(handle["state"] == "closed" and handle["final_stage_idle"] is True
                   and handle["service_exit_code"] == 0, "source audit requires a clean closed fixture")
    indexes = handle["indexes"]
    common.require(set(indexes) == {"bookbuilder-v2", "munarium"}, "wrong development repository roster")
    records = [audit_repository(repo, record) for repo, record in sorted(indexes.items())]
    report = {"schema": "airk.private.census-source-fidelity/1",
              "handle_sha256": args.handle_sha256, "repositories": records}
    if args.cohort:
        common.require(all(not record["source_hash_mismatches"] for record in records),
                       "archived read audit requires matching pinned source bytes")
        cohort = json.loads(search.pinned(args.cohort, args.cohort_sha256))
        pass_index = json.loads(search.pinned(args.pass_index, args.pass_index_sha256))
        report["cohort_sha256"] = args.cohort_sha256
        report["pass_index_sha256"] = args.pass_index_sha256
        report["archived_reads"] = audit_read_replies({r["repository"]: r for r in records}, indexes, cohort, pass_index)
    search.write_new(args.output, report)


if __name__ == "__main__":
    try:
        main()
    except (common.HarnessError, OSError, ValueError, KeyError, TypeError, sqlite3.Error) as error:
        print(str(error), file=sys.stderr)
        sys.exit(1)
