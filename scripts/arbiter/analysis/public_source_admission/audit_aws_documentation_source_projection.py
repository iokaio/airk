# SPDX-License-Identifier: Apache-2.0
"""Align the sealed AWS QA shortlist to exact publisher source locations.

Use checked same-user Python -I -B -S. The caller owns the private stdout
receipt. This script emits digests, locations and scoped comparison counts.
No publisher question, answer, or matched source text is written.
"""

import argparse
from collections import Counter
from concurrent.futures import ThreadPoolExecutor
import hashlib
import json
from pathlib import Path
import re
import runpy
import time
import unicodedata
from urllib.parse import quote


SCREEN_SOURCE_SHA = "8e6cdd9d312907d65bcc8f906efbbe2009ce1c308ff57d9077bbf300968b2f14"
SCREEN_RECEIPT = (42_360, "272de43ac4c79e941a053ae5e7f246d297e81f1c236f0aa00f9fe3194ba22132")
ROOT = Path(__file__).resolve().parents[4]
EXPOSURE_ROOT = ROOT / "artifacts/advanced-split/models/actual-composed-service-r2/public-human-label-admission-r1"
EXPOSURE = {
    "source_files": ("split-clone-chronology-r1/source-file-descriptors.json", 882_841,
                     "3f5f8b2c1ce3b217cd343a91c342f4b7a07e354995788be6cb0f44defb5261cb"),
    "train_rows": ("split-clone-chronology-r1/TRAIN-row-family-descriptors.json", 2_162_011,
                   "ef43c1965a5c60be7c41e157fc0e1a9d49897c40848aa6c9cdd84c6f04b5ab8b"),
    "techqa_train": ("techqa-original-train-audit-r1/document-source-candidates.json", 462_655,
                     "54db4bc5af4113e3b8cad76284cc7400322030b1f1f2b6279fd9305d0538784f"),
    "techqa_dev": ("techqa-original-publisher-archive-r2/full-original-DEV-source-fence-r5.json", 664_202,
                   "a0c1eeaf4302a7390d38494dfcde62950daeb239e2306cafdca53d3938473a1c"),
}
NOTICE_BLOBS = {
    "LICENSE": {"7785b9042bc90dad09ae994201568bbf395d8204"},
    "LICENSE-SUMMARY": {
        "04ce0f53927717645565690e39220cea636e6574", "56888df16c6d4f539c381bd4a2b19683c31dc556",
        "9a83a0946589013e6bafee02ee304162438edb80", "f92fab5dda6a137920e9f9728a8dcb226bf87577",
        "fa2e5700abe90c61460fce71f9ca73bbb6361053",
    },
    "LICENSE-SAMPLECODE": {
        "14aabc346a820d147f07eee4e26a5197d06b1670", "364f1dec0ec9aef92c76c7bbbb4e5e7875992cc7",
        "7e0bd89d36358c1348473c3b55ea8a19e291b298", "e0d30fcd310e254efc2a0090193d14a4562d07be",
    },
}
WHITESPACE_OR_CHARACTER = re.compile(r"\s+|[^\s]")
MARKDOWN_LINK = re.compile(r"!?\[([^\]]+)\]\([^)]+\)")
REMOVED_MARKDOWN = frozenset("*_+\x60[]\\")
CODE_FENCE = re.compile(r"^ {0,3}(\x60{3,}|~{3,})(.*)$")


def require(condition, message):
    if not condition:
        raise ValueError(message)


def sha256(data):
    return hashlib.sha256(data).hexdigest()


def pinned_local(path, length, sha):
    with path.open("rb") as stream:
        data = stream.read(length + 1)
    require(len(data) == length and sha256(data) == sha, "retained input pin differs")
    return data


def positions(source, target):
    require(bool(target), "empty match target")
    cursor = 0
    while (found := source.find(target, cursor)) != -1:
        yield found
        cursor = found + 1


def markdown_with_ranges(body):
    # All selected source blobs are already NFC. Require this before assigning
    # character ranges to the source rather than guessing about composition.
    require(unicodedata.is_normalized("NFC", body), "source normalization would alter code points")
    units = [(m[0] if not m[0].isspace() else " ", m.start(), m.end())
             for m in WHITESPACE_OR_CHARACTER.finditer(body)]
    if units and units[0][0] == " ":
        del units[0]
    if units and units[-1][0] == " ":
        units.pop()
    collapsed = "".join(unit[0] for unit in units)
    labels = {}
    endings = {}
    retained = []
    cursor = 0
    for link in MARKDOWN_LINK.finditer(collapsed):
        first, last = link.span(1)
        labels[units[first][1]] = units[link.start()][1]
        endings[units[last - 1][2]] = units[link.end() - 1][2]
        retained.extend(units[cursor:link.start()])
        retained.extend(units[first:last])
        cursor = link.end()
    retained.extend(units[cursor:])
    projected = [(folded, start, end) for character, start, end in retained
                 if character not in REMOVED_MARKDOWN for folded in character.casefold()]
    return "".join(unit[0] for unit in projected), projected, labels, endings


def project_unique_markdown(body, target, normalize):
    normalized, projected, labels, endings = markdown_with_ranges(body)
    require(normalized == normalize(body), "range map differs from the sealed screen transform")
    hits = list(positions(normalized, target))
    if len(hits) != 1:
        return len(hits), None
    off = hits[0]
    start, end = projected[off][1], projected[off + len(target) - 1][2]
    # A source range may need the original delimiters when its edge is
    # also the first or last retained character of an inline link label.
    ranges = {(a, b) for a in {start, labels.get(start, start)} for b in {end, endings.get(end, end)}
              if normalize(body[a:b]) == target}
    if not ranges:
        return 1, None
    # Use the shortest raw range that reproduces this unique normalized run.
    shortest = min(b - a for a, b in ranges)
    best = [(a, b) for a, b in ranges if b - a == shortest]
    return 1, best[0] if len(best) == 1 else None


def overlaps_fenced_block(body, first_line, last_line):
    open_fence = None
    for number, line in enumerate(body.split("\n"), 1):
        if number > last_line:
            break
        mark = CODE_FENCE.fullmatch(line.removesuffix("\r"))
        if open_fence is None:
            if mark:
                open_fence = mark[1]
                if number >= first_line:
                    return True
            continue
        if number >= first_line:
            return True
        if mark and mark[1][0] == open_fence[0] and (
                len(mark[1]) >= len(open_fence) and not mark[2].strip()):
            open_fence = None
    return False


def source_projection(original, answer, match_type, normalize):
    raw = original.encode("utf-8")
    if match_type == "literal":
        matches = list(positions(raw, answer.encode("utf-8")))
        occurrences = len(matches)
        bounds = (matches[0], matches[0] + len(answer.encode("utf-8"))) if occurrences == 1 else None
    else:
        require(match_type == "markdown_normalized", "unknown sealed AWS match type")
        occurrences, chars = project_unique_markdown(original, normalize(answer), normalize)
        bounds = tuple(len(original[:i].encode("utf-8")) for i in chars) if chars else None
    require(occurrences > 0, "sealed source match no longer exists")
    if bounds is None:
        return ("ambiguous_occurrences" if occurrences > 1 else "unresolved_range"), occurrences, None
    start, end = bounds
    first_line = raw.count(b"\n", 0, start) + 1
    last_line = raw.count(b"\n", 0, end - 1) + 1
    span = {
        "byte_start": start, "byte_end_exclusive": end, "span_sha256": sha256(raw[start:end]),
        "line_start": first_line, "line_end_inclusive": last_line,
        "overlaps_markdown_fenced_code": overlaps_fenced_block(original, first_line, last_line),
    }
    return ("unique_literal" if match_type == "literal" else "unique_markdown_projection"), occurrences, span


def notice_pin(family, name, screening, deadline):
    path = f"documents/{family}/{name}"
    notice = screening["fetch"](screening["RAW"] + quote(path, safe="/"), 32 << 10, deadline)
    git = hashlib.sha1(f"blob {len(notice)}\0".encode("ascii"), usedforsecurity=False)
    git.update(notice)
    require(git.hexdigest() in NOTICE_BLOBS[name], "selected AWS rights notice differs")
    return {"path": path, "bytes": len(notice), "git_blob_sha1": git.hexdigest(), "sha256": sha256(notice)}


def rights_evidence(candidates, screening, deadline):
    screening["pinned_raw"]("LICENSE", screening["QA_LICENSE"], deadline)
    families = sorted({row["document"]["family"] for row in candidates})
    require(len(families) == 12 and screening["KENDRA_FAMILY"] not in families,
            "sealed AWS family permission scope differs")
    with ThreadPoolExecutor(max_workers=10) as workers:
        notices = {(family, name): workers.submit(notice_pin, family, name, screening, deadline)
                   for family in families for name in NOTICE_BLOBS}
        return {
            "selected_families": {
                family: {"document_notice": notices[family, "LICENSE"].result(),
                         "license_summary": notices[family, "LICENSE-SUMMARY"].result(),
                         "sample_code_notice": notices[family, "LICENSE-SAMPLECODE"].result()}
                for family in families
            },
            "csv_notice": {"path": "LICENSE", "bytes": screening["QA_LICENSE"][0],
                           "sha256": screening["QA_LICENSE"][1], "declared_license": "CC-BY-4.0"},
            "scope": "repository CSV notice and separate pinned AWS family document and sample-code notices",
            "document_notice_license": "CC-BY-SA-4.0",
            "declared_terms_compliance_reviewed": False,
            "third_party_content_reviewed": False,
        }


def compare_field(rows, key, selection):
    values = [row[key] for row in rows if key in row and isinstance(row[key], str) and row[key]]
    return {"available": len(values), "exact_matches": sum(value in selection for value in values)}


def exposure_evidence(candidates):
    opened = {name: json.loads(pinned_local(EXPOSURE_ROOT / name_path, length, sha))
              for name, (name_path, length, sha) in EXPOSURE.items()}
    documents = {row["document"]["sha256"] for row in candidates}
    questions = {row["question_sha256"] for row in candidates}
    paths = {row["document"]["path"] for row in candidates}
    files = opened["source_files"]
    train = opened["train_rows"]
    techqa_train = opened["techqa_train"]["rows"]
    dev_ids = opened["techqa_dev"]["entire_DEV_source_id_closure"]
    require([len(files), len(train), len(techqa_train), len(dev_ids)] == [1514, 2525, 394, 12355],
            "retained exposure metadata census differs")
    return {
        "pins": {name: {"path": path, "bytes": size, "sha256": sha}
                 for name, (path, size, sha) in EXPOSURE.items()},
        "airk_file_descriptors": {"rows": len(files),
                                  "source_sha256": compare_field(files, "sha256", documents),
                                  "logical_path": compare_field(files, "logical_path", paths)},
        "airk_train_rows": {"rows": len(train),
                            "source_sha256": compare_field(train, "source_sha256", documents),
                            "question_sha256": compare_field(train, "question_sha256", questions),
                            "source_path": compare_field(train, "source_path", paths)},
        "techqa_train_questions": {"rows": len(techqa_train),
                                   "question_sha256": compare_field(techqa_train, "question_sha256", questions)},
        "techqa_dev_ids": {"ids": len(dev_ids), "literal_aws_doc_path_matches": len(set(dev_ids) & paths),
                           "shared_id_namespace": False},
        "full_calibration_lineage_reviewed": False,
        "semantic_clone_reviewed": False,
        "model_pretraining_exposure_known": False,
    }


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("--screen", type=Path, required=True, help="exact private 65-row shortlist")
    parser.add_argument("--seconds", type=int, default=75)
    args = parser.parse_args()
    if not 1 <= args.seconds <= 120:
        parser.error("require a 1–120 second allowance")
    started = time.monotonic()
    deadline = started + args.seconds
    source = Path(__file__).with_name("audit_aws_documentation_metadata.py")
    require(sha256(source.read_bytes()) == SCREEN_SOURCE_SHA, "screening implementation differs")
    screening = runpy.run_path(str(source))
    sealed = json.loads(pinned_local(args.screen, *SCREEN_RECEIPT))
    candidates = sealed["candidates"]
    originals = screening["read_rows"](deadline)
    # The sealed shortlist already authenticated the immutable Git tree.
    # Check raw contents against both its Git blob IDs and its body digests.
    docs = {}
    for row in candidates:
        doc = row["document"]
        path = doc["path"]
        identity = (doc["git_blob_sha1"], doc["bytes"], doc["sha256"])
        require(docs.setdefault(path, identity) == identity, "sealed source pins disagree")
    require(len(candidates) == 65 and len(docs) == 35, "sealed AWS shortlist census differs")
    with ThreadPoolExecutor(max_workers=10) as workers:
        pending = {path: workers.submit(screening["inspect_document"], path, pin[:2], deadline)
                   for path, pin in docs.items()}
        bodies = {path: job.result() for path, job in pending.items()}
    projections = []
    for row in candidates:
        screening["check_deadline"](deadline)
        selected = originals[row["selector"]["row_index"]]
        _, path = screening["document_path"](selected["Document_True"])
        body, body_sha = bodies[path]
        require(row["selector"]["member"] == screening["MEMBER"]
                and sha256(selected["Question"].encode("utf-8")) == row["question_sha256"]
                and path == row["document"]["path"] and body_sha == row["document"]["sha256"],
                "sealed AWS candidate no longer binds its native source identity")
        status, occurrences, span = source_projection(body, selected["Answer_True"], row["match_type"],
                                                      screening["normalize_markdown"])
        projections.append({
            "selector": row["selector"], "question_sha256": row["question_sha256"],
            "document": row["document"], "match_type": row["match_type"],
            "projection_status": status, "occurrences_in_document": occurrences, "source_span": span,
            "sufficient_support_judged": False, "native_visible": False,
        })
    rights = rights_evidence(candidates, screening, deadline)
    overlap = exposure_evidence(candidates)
    screening["check_deadline"](deadline)
    receipt = {
        "schema": "airk.private.aws-documentation-technical-support-source-projection/1",
        "complete": True,
        "screen_receipt": {"bytes": SCREEN_RECEIPT[0], "sha256": SCREEN_RECEIPT[1]},
        "screen_source_sha256": SCREEN_SOURCE_SHA,
        "source_revision": screening["REVISION"],
        "counts": dict(Counter(row["projection_status"] for row in projections)),
        "projection_coordinate_system": "zero-based original UTF-8 bytes [start,end); one-based inclusive lines",
        "publisher_rights_artifacts": rights,
        "retained_exposure_metadata": overlap,
        "source_rights_resolved": False, "exposure_review_complete": False,
        "acceptable_set_complete": False, "new_human_judgments": False,
        "native_compiler_projection_complete": False, "qualification": False,
        "candidates": projections,
        "elapsed_seconds": time.monotonic() - started,
    }
    print(json.dumps(receipt, separators=(",", ":"), sort_keys=True, allow_nan=False))


if __name__ == "__main__":
    main()
