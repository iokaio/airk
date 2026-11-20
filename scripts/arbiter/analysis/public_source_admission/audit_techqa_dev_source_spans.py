# SPDX-License-Identifier: Apache-2.0
"""Verify original TechQA DEV publisher offsets and retain only aggregate results.

Run through checked Windows Python -I -B -S with a fresh private --output path.
The retained metadata audit owns archive identity, parsing and cohort policy.
"""

import argparse
from collections import Counter
import hashlib
import io
import json
from pathlib import Path
import runpy
import sys
import tarfile
import time


HERE = Path(__file__).resolve().parent
OWNER_SHA = "1f511e1d6c0f0a1d254a77b802aedbce7a5ae8a3d134c9f7b91ed605f7c3874d"
DEV = "TechQA/training_and_dev/dev_Q_A.json"
DEV_SHA = "df62a05619b07b386159bcbaaf3e0e50be93eaf983aea8a5f8ed1513a3c2ec6f"
CORPUS = "TechQA/training_and_dev/training_dev_technotes.json"
CORPUS_SHA = "69261b5f4436d70379fd7172a8d97062c93f5d433c386d196937d198c66bf75a"


def require(condition, message):
    if not condition:
        raise ValueError(message)


def decode_string(raw):
    try:
        value = json.loads(raw)
        value.encode("utf-8")
        return value
    except (ValueError, UnicodeError, AttributeError):
        raise ValueError("selected TechQA value must be a Unicode scalar string") from None


def selected_answers(reader, raw, selected):
    def field():
        name = reader.string()
        reader.expect(58)
        if name == "QUESTION_ID":
            return name, reader.string()
        if name == "ANSWER" and reader.nonspace() == 34:
            start = reader.bytes - len(reader.buffer) + reader.index
            reader.skip_opaque_string()
            end = reader.bytes - len(reader.buffer) + reader.index
            return name, (start, end)
        reader.skip_opaque_value()
        return None

    answers = {}
    for fields in reader.collection(91, 93, lambda: list(reader.collection(123, 125, field))):
        data = [field for field in fields if field is not None]
        row = dict(data)
        require(len(row) == len(data), "duplicate selected TechQA reference field")
        identifier = row.get("QUESTION_ID")
        if identifier in selected:
            require(identifier not in answers and "ANSWER" in row, "missing or duplicate selected answer")
            start, end = row["ANSWER"]
            require(end - start <= 64 << 10, "selected answer string exceeds its bound")
            answers[identifier] = decode_string(raw[start:end])
    require(answers.keys() == selected.keys(), "provisional TechQA answers missing")
    return answers


def read_document_text(reader):
    reader.expect(34)
    raw = bytearray(b'"')
    escaped = False
    while True:
        char = reader.take()
        require(char >= 32 and len(raw) < 8 << 20, "invalid or oversized selected document text")
        raw.append(char)
        if not escaped and char == 34:
            return decode_string(raw)
        escaped = not escaped and char == 92


def freeze_document(freeze, identifier, body, annotations):
    raw = body.encode("utf-8")
    freeze["documents"][identifier] = {"bytes": len(raw), "sha256": hashlib.sha256(raw).hexdigest()}
    for start, end, answer, case_id in annotations:
        case = freeze["cases"][case_id]
        if not (0 <= start < end <= len(body)) or body[start:end] != answer:
            case["source_status"] = "withheld_publisher_span_mismatch"
            continue
        answer_bytes = answer.encode("utf-8")
        offset = len(body[:start].encode("utf-8"))
        case.update(source_status="exact_publisher_span", byte_start=offset,
                    byte_end_exclusive=offset + len(answer_bytes),
                    span_sha256=hashlib.sha256(answer_bytes).hexdigest())


def inspect_corpus(reader, menu_ids, gold, freeze=None):
    def field():
        name = reader.string()
        reader.expect(58)
        if name == "id":
            return name, reader.string()
        if name == "text" and (key in gold or freeze is not None):
            return name, read_document_text(reader)
        reader.skip_opaque_value()
        return None

    def document():
        nonlocal key, entries
        key = reader.string()
        reader.expect(58)
        entries += 1
        if key not in menu_ids:
            reader.skip_opaque_value()
            return
        require(key not in visited, "duplicate provisional menu document")
        data = [field for field in reader.collection(123, 125, field) if field is not None]
        fields = dict(data)
        require(len(fields) == len(data) and fields.get("id") == key, "provisional menu document ID differs")
        visited.add(key)
        if freeze is not None:
            require("text" in fields, "publisher menu document text missing")
            freeze_document(freeze, key, fields["text"], gold.get(key, []))
        if key in gold:
            require("text" in fields, "publisher gold document text missing")
            body = fields["text"]
            for start, end, answer, *_ in gold[key]:
                if not 0 <= start < end <= len(body):
                    counts["out_of_range"] += 1
                elif body[start:end] != answer:
                    counts["literal_mismatch"] += 1
                else:
                    counts["literal_match"] += 1

    key = None
    visited = set()
    entries = 0
    counts = Counter({"literal_match": 0, "literal_mismatch": 0, "out_of_range": 0})
    for _ in reader.collection(123, 125, document):
        pass
    require(visited == menu_ids, "provisional menus lack archived documents")
    return entries, dict(counts)


def audit(*, freeze_menus=False):
    started = time.monotonic()

    def check():
        require(time.monotonic() < started + 300, "original 300-second TechQA audit expired")

    owner_path = HERE / "audit_techqa_validation_metadata.py"
    require(hashlib.sha256(owner_path.read_bytes()).hexdigest() == OWNER_SHA, "metadata reader changed")
    owner = runpy.run_path(str(owner_path))
    archive = owner["ARCHIVE"]
    require(archive.resolve(strict=True) == archive
            and not any(path.is_symlink() or path.is_junction() for path in (archive, *archive.parents)),
            "canonical TechQA archive required")
    fence, fence_pin = owner["read_checked_json"](owner["ARCHIVE_ROOT"] / "full-original-DEV-source-fence-r5.json",
                                                  owner["DEV_FENCE_SHA"])
    caveat, caveat_pin = owner["read_checked_json"](owner["ARCHIVE_ROOT"] / "r4-scope-caveat.json",
                                                    owner["DEV_CAVEAT_SHA"])
    require(fence["original_archive_sha256"] == owner["ARCHIVE_SHA"]
            and caveat["r4_not_clean_exposure_proof"] is True, "TechQA source fence differs")
    dev_menu_ids = set(fence["entire_DEV_source_id_closure"])
    train_gold = {row["original_gold_document_id"] for row in fence["records"] if row["answerable"] is True}

    with archive.open("rb") as source:
        before = owner["archive_state"](source)
        require(before[2] == owner["ARCHIVE_BYTES"], "TechQA archive extent differs")
        digest = hashlib.sha256()
        while block := source.read(16 << 20):
            check()
            digest.update(block)
        require(digest.hexdigest() == owner["ARCHIVE_SHA"], "TechQA archive digest differs")

        def stream_member(name, expected_size, exact_size, visit):
            source.seek(0)
            with tarfile.open(fileobj=owner["DeadlineStream"](source, check), mode="r|gz") as tar:
                for entry in tar:
                    check()
                    if entry.name == name:
                        require(entry.isfile() and 0 < entry.size <= expected_size
                                and (not exact_size or entry.size == expected_size), "TechQA member extent differs")
                        with tar.extractfile(entry) as stream:
                            return visit(stream, entry.size)
            raise ValueError("pinned TechQA member missing")

        raw_dev = stream_member(DEV, owner["MAX_MEMBER_BYTES"], False, lambda stream, size: stream.read(size))
        require(hashlib.sha256(raw_dev).hexdigest() == DEV_SHA, "TechQA DEV member digest differs")
        refs = owner["MetadataReader"](io.BytesIO(raw_dev), check)
        roster = owner["parse_roster"](refs, reference=True)
        refs.finish(len(raw_dev))
        require(len(roster) == 310 and set().union(*(row["DOC_IDS"] for row in roster.values())) == dev_menu_ids,
                "TechQA DEV metadata scope differs")
        selected = {}
        for identifier, row in roster.items():
            menu = row["DOC_IDS"]
            docs = set(menu)
            if docs and len(docs) == len(menu) and all(docs) and docs.isdisjoint(train_gold):
                if owner["has_gold_metadata"](row, docs):
                    selected[identifier] = row
        menu_ids = set().union(*(row["DOC_IDS"] for row in selected.values()))
        require(len(selected) == 77 and len(menu_ids) == 3527
                and all(len(row["DOC_IDS"]) == 50 for row in selected.values()), "provisional TechQA cohort differs")

        answer_reader = owner["MetadataReader"](io.BytesIO(raw_dev), check)
        answers = selected_answers(answer_reader, raw_dev, selected)
        answer_reader.finish(len(raw_dev))
        freeze = {"cases": {}, "documents": {}} if freeze_menus else None
        gold = {}
        for identifier, row in selected.items():
            gold.setdefault(row["DOCUMENT"], []).append(
                (owner["numeric_offset"](row["START_OFFSET"]), owner["numeric_offset"](row["END_OFFSET"]),
                 answers[identifier], identifier))
            if freeze is not None:
                freeze["cases"][identifier] = {"ordered_document_ids": row["DOC_IDS"],
                                               "publisher_document_id": row["DOCUMENT"],
                                               "publisher_start": owner["numeric_offset"](row["START_OFFSET"]),
                                               "publisher_end": owner["numeric_offset"](row["END_OFFSET"])}

        require(len(gold) == 71, "provisional TechQA gold document count differs")

        def corpus_member(stream, size):
            reader = owner["MetadataReader"](stream, check, maximum_bytes=size)
            entries, counts = inspect_corpus(reader, menu_ids, gold, freeze)
            require(reader.finish(size)["sha256"] == CORPUS_SHA, "TechQA corpus member digest differs")
            return entries, counts

        entries, counts = stream_member(CORPUS, owner["EXACT_DEV_CORPUS_BYTES"], True, corpus_member)
        require(owner["archive_state"](source) == before, "TechQA archive changed during audit")
    require(entries == 28482 and sum(counts.values()) == len(selected), "TechQA audit denominator differs")
    check()
    if freeze is not None:
        require(len(freeze["documents"]) == len(menu_ids)
                and all("source_status" in case for case in freeze["cases"].values()), "incomplete menu freeze")
    return {
        "schema": ("airk.private.techqa-publisher-dev-menu-freeze/1" if freeze_menus
                   else "airk.private.techqa-publisher-dev-source-span-audit/1"),
        **({"menu_freeze": freeze} if freeze_menus else {}),
        "source_sha256": hashlib.sha256(Path(__file__).read_bytes()).hexdigest(),
        "metadata_owner_sha256": OWNER_SHA,
        "archive_sha256": owner["ARCHIVE_SHA"],
        "members": {DEV: DEV_SHA, CORPUS: CORPUS_SHA},
        "source_fence": fence_pin,
        "prior_exposure_caveat": caveat_pin,
        "provisional_cases": len(selected),
        "gold_documents": len(gold),
        "menu_documents": len(menu_ids),
        "corpus_entries": entries,
        "span_results": counts,
        "offset_rule": "zero-based Unicode scalar offsets, exclusive end, literal original text and answer equality",
        "text_output": False,
        "native_visibility": False,
        "qualification": False,
        "elapsed_seconds": time.monotonic() - started,
    }


def main():
    require(sys.platform == "win32" and sys.flags.isolated and sys.flags.no_site and sys.dont_write_bytecode,
            "use checked Windows Python -I -B -S")
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("--output", required=True, type=Path)
    parser.add_argument("--freeze-menus", action="store_true", help="retain ordered publisher menus and original byte pins")
    args = parser.parse_args()
    output = args.output
    require(output.is_absolute() and not output.exists() and output.parent.resolve(strict=True) == output.parent,
            "fresh absolute checked private output path required")
    result = audit(freeze_menus=args.freeze_menus)
    with output.open("x", encoding="utf-8", newline="\n") as stream:
        stream.write(json.dumps(result, allow_nan=False, separators=(",", ":")) + "\n")


if __name__ == "__main__":
    main()
