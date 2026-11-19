# SPDX-License-Identifier: Apache-2.0
"""Read the held local TechQA validation metadata and print one private aggregate JSON receipt.

Use isolated Windows Python -I -B -S through the checked human shell. The caller
owns the private output path and captures stdout. No files are written here.
"""

import hashlib
import json
import os
from pathlib import Path, PurePosixPath
import sys
import tarfile
import time


ROOT = Path(__file__).resolve().parents[4]
INPUTS = ROOT / "artifacts/advanced-split/models/actual-composed-service-r2/public-human-label-admission-r1"
ARCHIVE_ROOT = INPUTS / "techqa-original-publisher-archive-r2"
ARCHIVE = ARCHIVE_ROOT / "TechQA.publisher-original.tar.gz"
ARCHIVE_BYTES = 2_959_973_525
ARCHIVE_SHA = "6b094ef9a69718f727ce8d7e15c4d961e51032cefaa952e0d6af9d176d7ba118"
DEV_FENCE_SHA = "a0c1eeaf4302a7390d38494dfcde62950daeb239e2306cafdca53d3938473a1c"
DEV_CAVEAT_SHA = "6d16534c8a314524fc766a91637fd17de186d9f012cceca3ee2810a87871665b"
VALIDATION = "TechQA/validation/"
MEMBERS = {
    VALIDATION + "validation_questions.json": "questions",
    VALIDATION + "validation_reference.json": "references",
    VALIDATION + "validation_technotes.json": "corpus",
    VALIDATION + "validation_example_output.json": "example",
}
MAX_MEMBER_BYTES = 64 << 20
MAX_ROWS = 20_000
WS = frozenset(b" \t\r\n")


def require(condition, message):
    if not condition:
        raise ValueError(message)


class DeadlineStream:
    def __init__(self, source, check):
        self.source = source
        self.check = check

    def read(self, length):
        self.check()
        return self.source.read(length)


class MetadataReader:
    """Byte-token reader that decodes only selected identifier and label metadata."""

    def __init__(self, source, check):
        self.source = source
        self.check = check
        self.buffer = b""
        self.index = 0
        self.bytes = 0
        self.digest = hashlib.sha256()

    def peek(self):
        if self.index == len(self.buffer):
            self.check()
            self.buffer = self.source.read(64 << 10)
            self.index = 0
            self.bytes += len(self.buffer)
            require(self.bytes <= MAX_MEMBER_BYTES, "validation metadata member exceeds its bound")
            self.digest.update(self.buffer)
        return self.buffer[self.index] if self.buffer else -1

    def take(self):
        value = self.peek()
        if value != -1:
            self.index += 1
        return value

    def nonspace(self):
        while self.peek() in WS:
            self.take()
        return self.peek()

    def expect(self, token):
        require(self.nonspace() == token, "unexpected validation JSON delimiter")
        self.take()

    def string(self, capture=True):
        self.expect(34)
        raw = bytearray(b'"') if capture else None
        escaped = False
        while True:
            char = self.take()
            require(char >= 32, "invalid validation JSON string")
            if raw is not None:
                raw.append(char)
                require(len(raw) <= 1024, "selected validation metadata string exceeds its bound")
            if not escaped and char == 34:
                break
            escaped = not escaped and char == 92
        if raw is not None:
            try:
                return json.loads(bytes(raw))
            except (ValueError, UnicodeError):
                raise ValueError("invalid selected validation metadata string") from None
        return None

    def skip_value(self):
        first = self.nonspace()
        if first == 34:
            self.string(capture=False)
        elif first in (91, 123):
            self.take()
            end = [93 if first == 91 else 125]
            while end:
                char = self.peek()
                if char == 34:
                    self.string(capture=False)
                    continue
                self.take()
                require(char != -1, "unterminated unselected validation value")
                if char in (91, 123):
                    end.append(93 if char == 91 else 125)
                    require(len(end) <= 64, "unselected validation nesting exceeds its bound")
                elif char in (93, 125):
                    require(char == end.pop(), "invalid unselected validation nesting")
        else:
            self.atom(capture=False)

    def atom(self, capture=True):
        self.nonspace()
        raw = bytearray() if capture else None
        length = 0
        while (char := self.peek()) != -1 and char not in WS and char not in (44, 93, 125):
            self.take()
            length += 1
            if raw is not None:
                raw.append(char)
                require(length <= 32, "selected numeric metadata exceeds its bound")
        require(length > 0, "missing validation JSON scalar")
        if raw is not None:
            try:
                return json.loads(bytes(raw))
            except (ValueError, UnicodeError):
                raise ValueError("invalid selected validation numeric metadata") from None
        return None

    def collection(self, opening, closing, item):
        self.expect(opening)
        if self.nonspace() == closing:
            self.take()
            return
        while True:
            yield item()
            boundary = self.nonspace()
            if boundary == closing:
                self.take()
                return
            self.expect(44)

    def finish(self, expected_size):
        require(self.nonspace() == -1 and self.bytes == expected_size,
                "validation member has trailing bytes or incomplete source coverage")
        return {"bytes": self.bytes, "sha256": self.digest.hexdigest()}


def parse_roster(reader, *, reference):
    selected = {"QUESTION_ID", "DOC_IDS"}
    if reference:
        selected.update(("ANSWERABLE", "DOCUMENT", "START_OFFSET", "END_OFFSET"))

    def field():
        key = reader.string()
        reader.expect(58)
        if key == "DOC_IDS":
            values = []
            for item in reader.collection(91, 93, reader.string):
                values.append(item)
                require(len(values) <= 4096, "validation document menu exceeds its bound")
            value = tuple(values)
        elif key in ("START_OFFSET", "END_OFFSET") and key in selected:
            value = reader.string() if reader.nonspace() == 34 else reader.atom()
        elif key in selected:
            value = reader.string()
        else:
            reader.skip_value()
            return None
        return key, value

    rows = {}
    for fields in reader.collection(91, 93, lambda: list(reader.collection(123, 125, field))):
        kept = [entry for entry in fields if entry is not None]
        row = dict(kept)
        require(len(kept) == len(row) and "QUESTION_ID" in row and "DOC_IDS" in row,
                "missing or duplicate selected validation metadata")
        key = row.pop("QUESTION_ID")
        require(key and key not in rows and len(rows) < MAX_ROWS, "invalid validation question identifier census")
        rows[key] = row
    return rows


def parse_corpus(reader):
    def document():
        identifier = reader.string()
        reader.expect(58)
        require(reader.nonspace() == 123, "expected a validation corpus document object")
        reader.skip_value()
        return identifier

    docs = set()
    for identifier in reader.collection(123, 125, document):
        require(identifier and identifier not in docs and len(docs) < MAX_ROWS,
                "invalid validation corpus document census")
        docs.add(identifier)
    return docs


def numeric_offset(value):
    if type(value) is int:
        return value if 0 <= value < 1 << 31 else None
    if isinstance(value, str) and 0 < len(value) <= 10 and value.isascii() and value.isdecimal():
        return int(value) if int(value) < 1 << 31 else None
    return None


def paired_counts(questions, references, docs, dev, train_gold):
    paired = questions.keys() & references.keys()
    result = {"questions": len(questions), "references": len(references), "matched_question_ids": len(paired),
              "question_ids_without_reference": len(questions) - len(paired),
              "reference_ids_without_question": len(references) - len(paired),
              "corpus_document_ids": len(docs), "matched_menu_rosters": 0, "matched_menu_orders": 0,
              "answerable_reference_rows": 0, "unanswerable_reference_rows": 0,
              "other_answerability_reference_rows": 0, "matched_complete_menus": 0,
              "answerable_matched_complete_menus_with_offsets": 0}
    question_docs = set()
    for row in questions.values():
        question_docs.update(row["DOC_IDS"])
    for row in references.values():
        label = row.get("ANSWERABLE")
        category = "answerable" if label == "Y" else "unanswerable" if label == "N" else "other_answerability"
        result[category + "_reference_rows"] += 1
    for identifier in paired:
        menu = questions[identifier]["DOC_IDS"]
        reference = references[identifier]
        other = reference["DOC_IDS"]
        menu_set = set(menu)
        same_roster = bool(menu) and len(menu_set) == len(menu) == len(other) and menu_set == set(other)
        same_order = same_roster and menu == other
        complete = same_order and menu_set <= docs
        start = numeric_offset(reference.get("START_OFFSET"))
        end = numeric_offset(reference.get("END_OFFSET"))
        result["matched_menu_rosters"] += same_roster
        result["matched_menu_orders"] += same_order
        result["matched_complete_menus"] += complete
        result["answerable_matched_complete_menus_with_offsets"] += (
            complete and reference.get("ANSWERABLE") == "Y" and reference.get("DOCUMENT") in menu_set
            and start is not None and end is not None and start < end)
    result.update(unique_menu_documents=len(question_docs), menu_documents_missing_in_corpus=len(question_docs - docs),
                  menu_documents_overlapping_dev_menu_ids=len(question_docs & dev),
                  menu_documents_overlapping_train_gold_ids=len(question_docs & train_gold))
    return result


def read_checked_json(path, expected):
    with path.open("rb") as stream:
        raw = stream.read(8 << 20 | 1)
    require(len(raw) <= 8 << 20 and hashlib.sha256(raw).hexdigest() == expected,
            "retained TechQA metadata provenance differs")
    return json.loads(raw), {"path": str(path), "bytes": len(raw), "sha256": expected}


def archive_state(source):
    observed = os.fstat(source.fileno())
    return (observed.st_dev, observed.st_ino, observed.st_size, observed.st_mtime_ns)


def audit():
    started = time.monotonic()

    def check():
        if time.monotonic() >= started + 300:
            raise TimeoutError("original 300-second TechQA metadata audit expired")

    source_pin = {"path": str(Path(__file__).resolve()),
                  "sha256": hashlib.sha256(Path(__file__).read_bytes()).hexdigest()}
    require(ARCHIVE.resolve(strict=True) == ARCHIVE
            and not any(path.is_symlink() or path.is_junction() for path in (ARCHIVE, *ARCHIVE.parents)),
            "canonical local TechQA publisher archive required")
    results, members = {}, {}
    with ARCHIVE.open("rb") as source:
        before = archive_state(source)
        require(before[2] == ARCHIVE_BYTES, "TechQA archive size differs")
        digest = hashlib.sha256()
        while block := source.read(16 << 20):
            check()
            digest.update(block)
        require(digest.hexdigest() == ARCHIVE_SHA, "TechQA archive digest differs")
        source.seek(0)
        with tarfile.open(fileobj=DeadlineStream(source, check), mode="r|gz") as archive:
            for member in archive:
                check()
                require(not PurePosixPath(member.name).is_absolute() and ".." not in PurePosixPath(member.name).parts,
                        "unsafe archive member path")
                if member.name not in MEMBERS:
                    continue
                kind = MEMBERS[member.name]
                require(kind not in members and member.isfile() and 0 < member.size <= MAX_MEMBER_BYTES,
                        "duplicate, nonregular, empty or oversized validation member")
                identity = {"member": member.name, "bytes": member.size, "content_read": kind != "example"}
                if kind != "example":
                    with archive.extractfile(member) as entry:
                        reader = MetadataReader(entry, check)
                        results[kind] = (parse_corpus(reader) if kind == "corpus"
                                         else parse_roster(reader, reference=kind == "references"))
                        identity.update(reader.finish(member.size))
                members[kind] = identity
        require(set(members) == set(MEMBERS.values()) and archive_state(source) == before,
                "incomplete validation archive roster or publisher input changed")

    fence, fence_pin = read_checked_json(ARCHIVE_ROOT / "full-original-DEV-source-fence-r5.json", DEV_FENCE_SHA)
    caveat, caveat_pin = read_checked_json(ARCHIVE_ROOT / "r4-scope-caveat.json", DEV_CAVEAT_SHA)
    require(fence["original_archive_sha256"] == ARCHIVE_SHA and fence["original_DEV_rows_DOC_IDS_only"] == 310
            and fence["counts"]["whole_candidate_pool_disjoint_positive"] == 15
            and caveat["r4_not_clean_exposure_proof"] is True, "retained DEV/TRAIN scope proof differs")
    dev = set(fence["entire_DEV_source_id_closure"])
    train = {row["original_gold_document_id"] for row in fence["records"] if row["answerable"] is True}
    counts = paired_counts(results["questions"], results["references"], results["corpus"], dev, train)
    check()
    return {
        "schema": "airk.private.techqa-publisher-validation-metadata/1", "complete": True,
        "archive": {"path": str(ARCHIVE), "bytes": ARCHIVE_BYTES, "sha256": ARCHIVE_SHA},
        "source": source_pin, "members": members, "counts": counts,
        "metadata_only": True, "question_or_answer_text_decoded": False,
        "document_text_decoded": False, "qualification": False,
        "dev": {"metadata_rows": 310, "menu_source_ids": len(dev), "fence": fence_pin, "prior_exposure_caveat": caveat_pin},
        "train": {"answerable_whole_original_menus_disjoint_from_dev": 15, "gold_source_ids": len(train)},
        "eligibility": "metadata counts only; native visibility, source spans, split/exposure, and statistical power remain unaudited",
        "elapsed_seconds": time.monotonic() - started,
    }


def main():
    require(sys.platform == "win32" and sys.flags.isolated and sys.flags.no_site and sys.dont_write_bytecode,
            "use checked Windows Python -I -B -S")
    print(json.dumps(audit(), allow_nan=False, separators=(",", ":")))


if __name__ == "__main__":
    main()
