# SPDX-License-Identifier: Apache-2.0
"""Prepare authenticated controller-only evidence for the twenty frozen development cases.

Use --preflight to check the roster and all primary byte ranges without writing
files. In --output mode, a fresh private directory receives the per-case
manifests and complete_evidence_inputs.json for the Codex freeze. Supply the
audited closed database for each export and the separately pinned Answer
publication token. Diagnostics contain only counts and short failure codes.
"""
from __future__ import annotations

import argparse
from contextlib import closing
from dataclasses import dataclass
import io
from itertools import islice
import json
from pathlib import Path, PurePosixPath
import re
import sqlite3
import sys

HERE = Path(__file__).resolve().parent
if str(HERE) not in sys.path:
    sys.path.insert(0, str(HERE))

import common
from complete_evidence import CompleteEvidence, source_passage

PHYSICAL = {"bookbuilder": "bookbuilder-v2", "munarium": "munarium"}
ESSENTIALS = {"bookbuilder": 61, "munarium": 52}
SHA256 = re.compile(r"[0-9a-f]{64}")
COMMIT = re.compile(r"[0-9a-f]{40}")
TOKEN = re.compile(r"airk-r1:([0-9a-f]{32}):(0|[1-9][0-9]{0,19}):[0-9a-f]{64}")
MANIFEST_LIMIT = 4 << 20
SOURCE_LIMIT = 64 << 20
FAILURE_LIMIT = 16


class PreparationError(Exception):
    """A small diagnostic code that can be written without disclosing inputs."""


def require(ok, code):
    if not ok:
        raise PreparationError(code)


def pinned_json(path, digest, label):
    require(SHA256.fullmatch(digest or "") is not None, label + "_pin")
    try:
        with Path(path).open("rb") as stream:
            data = stream.read(MANIFEST_LIMIT + 1)
        require(len(data) <= MANIFEST_LIMIT, label + "_size")
        require(common.sha256_bytes(data) == digest, label + "_digest")
        value = json.loads(data)
    except (OSError, UnicodeError, ValueError) as error:
        raise PreparationError(label + "_unreadable") from error
    require(isinstance(value, dict), label + "_shape")
    return value


def token_parts(value):
    match = TOKEN.fullmatch(value or "")
    require(match is not None and int(match[2]) < (1 << 64), "invalid_publication_token")
    return match[1], int(match[2])


def reopen_exact(root, relative):
    require(isinstance(relative, str) and relative and "\0" not in relative
            and "\\" not in relative and ":" not in relative
            and all(part not in ("", ".", "..") for part in relative.split("/")),
            "invalid_relative_path")
    candidate = root / PurePosixPath(relative)
    try:
        actual = candidate.resolve(strict=True)
    except OSError as error:
        raise PreparationError("missing_input_path") from error
    require(actual == candidate and actual.is_relative_to(root) and actual.is_file(),
            "linked_or_foreign_input_path")
    return actual


def excerpt_bytes(data, first, last):
    require(type(first) is int and type(last) is int and 1 <= first <= last <= len(data),
            "invalid_line_range")
    lines = list(islice(io.BytesIO(data), first - 1, last))
    require(len(lines) == last - first + 1, "line_range_outside_source")
    return b"".join(lines)


def source_span(root, relative, first, last, expected_sha):
    path = reopen_exact(root, relative)
    try:
        with path.open("rb") as stream:
            data = stream.read(SOURCE_LIMIT + 1)
        require(len(data) <= SOURCE_LIMIT, "source_exceeds_limit")
        require(common.sha256_bytes(data) == expected_sha, "exported_source_digest_differs")
        require(b"\0" not in data, "source_needs_extracted_mapping")
        data.decode("utf-8", errors="strict")
        excerpt = excerpt_bytes(data, first, last)
    except (OSError, UnicodeError) as error:
        raise PreparationError("source_unreadable_or_non_utf8") from error
    return {"path": relative, "start_line": first, "end_line": last,
            "source_sha256": expected_sha, "excerpt_sha256": common.sha256_bytes(excerpt)}, len(excerpt)


@dataclass(frozen=True)
class Repository:
    key: str
    root: Path
    commit: str
    archive_sha256: str
    receipt_sha256: str
    database: Path
    database_sha256: str
    indexed_file_count: int
    audited_token: str
    publication: str | None


def repository_pin(values, freeze, audits, publications):
    key, receipt_path, receipt_sha, database_path = values
    require(key in PHYSICAL, "invalid_repository")
    publication = publications.get(key)
    physical = PHYSICAL[key]
    receipt = pinned_json(receipt_path, receipt_sha, "export_receipt")
    audit = audits[physical]
    commit = freeze.get(key + "_commit")
    require(isinstance(commit, str) and COMMIT.fullmatch(commit) is not None
            and receipt.get("repository") == physical and receipt.get("commit") == commit
            and audit.get("source_commit") == commit, "source_commit_or_repository_differs")
    archive = receipt.get("archive", {})
    require(isinstance(archive, dict) and SHA256.fullmatch(archive.get("sha256", "")) is not None
            and type(archive.get("bytes")) is int and archive["bytes"] > 0,
            "invalid_archive_pin")
    require(type(audit.get("indexed_paths")) is int and audit["indexed_paths"] > 0
            and audit.get("matching_source_hashes") == audit["indexed_paths"]
            and receipt.get("files") == audit.get("exported_paths")
            and all(audit.get(name) == [] for name in
                    ("source_hash_mismatches", "outside_export", "indexed_denies")),
            "source_audit_does_not_authenticate_export")
    audited_token = audit.get("publication_token", "")
    incarnation, _ = token_parts(audited_token)
    require(SHA256.fullmatch(audit.get("database_sha256", "")) is not None,
            "invalid_audited_database_pin")
    supplied_root = Path(receipt["root"])
    root = supplied_root.resolve(strict=True)
    supplied_database = Path(database_path)
    database = supplied_database.resolve(strict=True)
    require(supplied_root.is_absolute() and supplied_root == root and root.is_dir()
            and supplied_database.is_absolute() and supplied_database == database
            and database.name == "index.sqlite3" and database.parent.name == incarnation
            and not database.is_relative_to(root), "invalid_export_or_database_root")
    return Repository(key, root, commit, archive["sha256"], receipt_sha, database,
                      audit["database_sha256"], audit["indexed_paths"], audited_token, publication)


def check_closed_database(repository):
    for suffix in ("-wal", "-journal"):
        sidecar = repository.database.with_name(repository.database.name + suffix)
        require(not sidecar.exists() or sidecar.stat().st_size == 0, "audited_database_has_pending_sidecar")
    require(common.sha256_file(repository.database) == repository.database_sha256,
            "audited_database_digest_differs")


def verify_database(read, repository):
    read.execute("PRAGMA trusted_schema=OFF")
    read.execute("PRAGMA query_only=ON")
    read.execute("PRAGMA temp_store=MEMORY")
    scope_row = read.execute("SELECT value FROM meta WHERE key='scope'").fetchone()
    sequence_row = read.execute("SELECT value FROM meta WHERE key='sequence'").fetchone()
    incarnation, sequence = token_parts(repository.audited_token)
    require(scope_row is not None and sequence_row == (sequence,), "audited_publication_differs")
    scope = json.loads(scope_row[0])
    require(scope.get("incarnation") == incarnation
            and scope.get("collection") == "answer-eval-" + PHYSICAL[repository.key]
            and Path(scope["root"]).samefile(repository.root),
            "audited_publication_scope_differs")
    require(read.execute("SELECT count(*) FROM files").fetchone() == (repository.indexed_file_count,),
            "audited_indexed_roster_differs")


def build_case(root, freeze, provenance, repository, read, slot, fail):
    relative = f"scenarios/{repository.key}/{slot}.json"
    pin = freeze["files"].get(relative)
    doc = pinned_json(reopen_exact(root, relative), pin, "scenario")
    require(doc.get("schema") == "airk.private.answer-eval-scenario/1"
            and (doc.get("repository"), doc.get("slot"), doc.get("set"), doc.get("source_commit"))
            == (repository.key, slot, "development", repository.commit)
            and isinstance(doc.get("notes"), list), "scenario_identity_differs")
    ids = []
    spans = {}
    lengths = {}
    for note in doc["notes"]:
        if note.get("mark") != "essential":
            continue
        identity = note.get("id")
        require(isinstance(identity, str) and identity and identity not in ids, "invalid_essential_roster")
        ids.append(identity)
        try:
            path = note.get("path")
            lines = note.get("lines")
            require(isinstance(path, str) and isinstance(lines, list) and len(lines) == 2,
                    "missing_primary_source_range")
            first, last = lines
            require(type(first) is int and type(last) is int, "invalid_line_range")
            row = read.execute("SELECT lower(hex(source_hash)) FROM files WHERE path=?", (path,)).fetchone()
            require(row is not None and SHA256.fullmatch(row[0]) is not None, "primary_path_not_in_audited_index")
            key = (path, first, last)
            if key not in spans:
                span, byte_count = source_span(repository.root, path, first, last, row[0])
                spans[key] = {**span, "essential_ids": []}
                lengths[key] = byte_count
            spans[key]["essential_ids"].append(identity)
        except PreparationError as error:
            fail(repository.key, slot, len(ids), str(error))
    require(len(ids) >= 3, "invalid_essential_roster")
    covered = sum(len(span["essential_ids"]) for span in spans.values())
    manifest = {
        "schema": "airk.private.complete-evidence-input/1",
        "repo": repository.key,
        "slot": slot,
        "scenario_sha256": pin,
        "publication_token": repository.publication,
        "source_commit": repository.commit,
        "source_archive_sha256": repository.archive_sha256,
        "source_export_receipt_sha256": repository.receipt_sha256,
        "source_audit_publication_token": repository.audited_token,
        "source_audit_database_sha256": repository.database_sha256,
        **provenance,
        "spans": [spans[key] for key in sorted(spans)],
    }
    raw = common.canonical_json(manifest) if repository.publication is not None else None
    if covered == len(ids):
        try:
            if raw is None:
                for span in spans.values():
                    source_passage(repository.root, span)
            else:
                verified = CompleteEvidence.from_manifest(
                    raw, expected_sha256=common.sha256_bytes(raw), source_root=repository.root,
                    repo=repository.key, slot=slot, scenario_sha256=pin,
                    publication_token=repository.publication, essential_ids=ids,
                )
                require(len(verified.passages) == len(spans), "consumer_span_roster_differs")
        except (common.HarnessError, OSError, UnicodeError, ValueError, TypeError, KeyError) as error:
            raise PreparationError("consumer_recheck_refused") from error
    return raw, len(ids), covered, len(spans), sum(lengths.values())


def prepare(args):
    root = args.root.resolve(strict=True)
    freeze = pinned_json(reopen_exact(root, "scenario-freeze.json"), args.scenario_freeze_sha256, "freeze")
    require(freeze.get("schema") == "airk.private.answer-eval-scenario-freeze/1"
            and isinstance(freeze.get("files"), dict), "invalid_scenario_freeze")
    source_audit = pinned_json(args.source_audit, args.source_audit_sha256, "source_audit")
    require(source_audit.get("schema") == "airk.private.census-source-fidelity/1"
            and SHA256.fullmatch(source_audit.get("handle_sha256", "")) is not None
            and isinstance(source_audit.get("repositories"), list)
            and len(source_audit["repositories"]) == len(PHYSICAL),
            "invalid_source_audit")
    audits = {item["repository"]: item for item in source_audit["repositories"]}
    require(set(audits) == set(PHYSICAL.values()) and len(args.repository) == len(PHYSICAL)
            and {row[0] for row in args.repository} == set(PHYSICAL), "invalid_repository_roster")
    publications = dict(args.publication or [])
    if publications or args.output is not None:
        require(set(publications) == set(PHYSICAL) and len(args.publication or []) == len(PHYSICAL)
                and len(set(publications.values())) == len(PHYSICAL), "invalid_target_publication_roster")
        for publication in publications.values():
            token_parts(publication)
    repositories = {row[0]: repository_pin(row, freeze, audits, publications) for row in args.repository}
    provenance = {"scenario_freeze_sha256": args.scenario_freeze_sha256,
                  "source_audit_sha256": args.source_audit_sha256}
    results = {}
    summaries = {}
    failures = []
    failed_count = 0

    def fail(repo, slot, essential_ordinal, code):
        nonlocal failed_count
        failed_count += 1
        if len(failures) < FAILURE_LIMIT:
            failures.append({"repo": repo, "slot": slot, "essential_ordinal": essential_ordinal, "code": code})

    for repo, repository in sorted(repositories.items()):
        counts = dict(cases=0, essentials=0, authenticated_essentials=0, spans=0, excerpt_bytes=0)
        check_closed_database(repository)
        with closing(sqlite3.connect(repository.database.as_uri() + "?mode=ro&immutable=1", uri=True)) as read:
            verify_database(read, repository)
            for slot in common.SLOTS:
                raw, total, covered, spans, size = build_case(root, freeze, provenance, repository, read, slot, fail)
                counts["cases"] += 1
                counts["essentials"] += total
                counts["authenticated_essentials"] += covered
                counts["spans"] += spans
                counts["excerpt_bytes"] += size
                results[f"{repo}-{slot}"] = (raw, repository)
        check_closed_database(repository)
        require(counts["essentials"] == ESSENTIALS[repo], "frozen_essential_count_differs")
        summaries[repo] = counts
    require(len(results) == 20, "frozen_case_count_differs")
    report = {"ok": failed_count == 0, "repositories": summaries, "failures": failed_count,
              "target_publication_pins_supplied": bool(publications), "bounded_failure_codes": failures}
    if args.output is not None and report["ok"]:
        write_cases(args.output, root, results)
    return report


def write_cases(output, root, results):
    parent = output.parent.resolve(strict=True)
    require(output.is_absolute() and output.parent == parent and parent.is_relative_to(root)
            and not {"scenarios", "sealed"} & set(output.relative_to(root).parts),
            "output_must_be_private_fresh_directory")
    target = parent / output.name
    require(all(not target.is_relative_to(record.root)
                and not target.is_relative_to(record.database.parent) for _, record in results.values()),
            "output_overlaps_source_or_audited_index")
    target.mkdir()
    pins = {}
    for key, (raw, repository) in sorted(results.items()):
        require(raw is not None and repository.publication is not None, "missing_target_publication")
        path = target / f"{key}.json"
        with path.open("xb") as stream:
            stream.write(raw)
        pins[key] = {"manifest": str(path), "manifest_sha256": common.sha256_bytes(raw),
                     "source_root": str(repository.root), "publication_token": repository.publication}
    with (target / "complete_evidence_inputs.json").open("xb") as stream:
        stream.write(common.canonical_json(pins))


def main(argv=None):
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("--root", type=Path, required=True)
    parser.add_argument("--scenario-freeze-sha256", required=True)
    parser.add_argument("--source-audit", type=Path, required=True)
    parser.add_argument("--source-audit-sha256", required=True)
    parser.add_argument("--repository", action="append", nargs=4, required=True,
                        metavar=("REPO", "EXPORT_RECEIPT", "RECEIPT_SHA256", "AUDITED_DB"))
    parser.add_argument("--publication", action="append", nargs=2, metavar=("REPO", "ANSWER_TOKEN"),
                        help="explicit target publication for each repo, required to write")
    mode = parser.add_mutually_exclusive_group(required=True)
    mode.add_argument("--preflight", action="store_true")
    mode.add_argument("--output", type=Path)
    args = parser.parse_args(argv)
    try:
        report = prepare(args)
    except PreparationError as error:
        report = {"ok": False, "code": str(error)}
    except (OSError, UnicodeError, ValueError, KeyError, TypeError, AttributeError, sqlite3.Error):
        report = {"ok": False, "code": "invalid_or_unavailable_input"}
    print(json.dumps(report, sort_keys=True))
    return 0 if report["ok"] else 1


if __name__ == "__main__":
    sys.exit(main())
