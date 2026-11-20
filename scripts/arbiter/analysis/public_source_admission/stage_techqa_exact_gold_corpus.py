# SPDX-License-Identifier: Apache-2.0
"""Stage exact original TechQA DEV gold documents for an indexed source-read diagnostic.

The checked caller creates an empty private output and corpus directory and
preflights all destinations from the sealed menu receipt. Publisher menu order
remains in that receipt. Only exact-match cases are staged.
"""

import argparse
import hashlib
import json
from pathlib import Path
import re
import runpy
import tarfile
import time

HERE = Path(__file__).resolve().parent
FREEZE = "50f876ebb9cf3a8328314e7be3e01f343b2d1b67cd7979fe2aaceeb2c82064f7"
READER = "1f511e1d6c0f0a1d254a77b802aedbce7a5ae8a3d134c9f7b91ed605f7c3874d"
SPAN_READER = "7f45cb47d897154cdf50505a6ca6b6e99b9acbe9baf83c3f6e33e0c3bf5f8958"
CAMPAIGN = HERE.parents[3] / "artifacts/v190/final-plan/exec-20261116T052942Z"
MEMBER = "TechQA/training_and_dev/training_dev_technotes.json"


def require(ok, message):
    if not ok:
        raise ValueError(message)


def sha(raw):
    return hashlib.sha256(raw).hexdigest()


def load_reader(name, expected):
    path = HERE / name
    require(sha(path.read_bytes()) == expected, "source archive reader changed")
    return runpy.run_path(str(path))


def gather(reader, pins, decode):
    selected = {}

    def document():
        identifier = reader.string()
        reader.expect(58)
        if identifier not in pins:
            reader.skip_opaque_value()
            return

        def field():
            name = reader.string()
            reader.expect(58)
            if name == "id":
                return name, reader.string()
            if name == "text":
                return name, decode(reader)
            reader.skip_opaque_value()
            return None

        fields = [pair for pair in reader.collection(123, 125, field) if pair is not None]
        record = dict(fields)
        require(identifier not in selected and len(fields) == 2 and record.get("id") == identifier,
                "missing, duplicate or mismatched publisher fields")
        raw = record["text"].encode()
        require(pins[identifier] == {"sha256": sha(raw), "bytes": len(raw)}, "publisher source bytes changed")
        selected[identifier] = raw

    count = sum(1 for _ in reader.collection(123, 125, document))
    require(count == 28482 and selected.keys() == pins.keys(), "incomplete selected corpus")
    return selected


def source_span(raw, case):
    start, end = case["byte_start"], case["byte_end_exclusive"]
    require(0 <= start < end <= len(raw) and sha(raw[start:end]) == case["span_sha256"],
            "publisher span bytes differ from freeze")
    text = raw.decode()
    left, right = case["publisher_start"], case["publisher_end"]
    require(0 <= left < right <= len(text) and len(text[:left].encode()) == start
            and len(text[:right].encode()) == end, "publisher scalar and byte offsets differ")
    return {"byte_start": start, "byte_end_exclusive": end,
            "line_start": raw.count(b"\n", 0, start) + 1,
            "line_end_inclusive": raw.count(b"\n", 0, end - 1) + 1, "span_sha256": sha(raw[start:end])}


def stage(output):
    started = time.monotonic()

    def deadline():
        require(time.monotonic() - started < 300, "original 300-second archive deadline expired")

    require(output.parent == CAMPAIGN and re.fullmatch(r"techqa-exact-gold-source-corpus-r[1-9][0-9]*", output.name)
            and output.resolve(strict=True) == output and {p.name for p in output.iterdir()} == {"corpus"}
            and (output / "corpus").resolve(strict=True) == output / "corpus"
            and not any((output / "corpus").iterdir()), "checked fresh private directory required")
    pinned = (CAMPAIGN / "techqa-original-dev-ordered-menu-freeze-r1.json").read_bytes()
    require(sha(pinned) == FREEZE, "menu freeze differs")
    freeze = json.loads(pinned)
    original = freeze["menu_freeze"]
    cases = {i: c for i, c in original["cases"].items() if c["source_status"] == "exact_publisher_span"}
    pins = {c["publisher_document_id"]: original["documents"][c["publisher_document_id"]] for c in cases.values()}
    require(len(cases) == 76 and len(pins) == 71 and len(original["cases"]) == 77, "wrong accepted cohort")
    require(all(re.fullmatch(r"[A-Za-z0-9_-]+", key) for key in pins)
            and len({key.casefold() for key in pins}) == len(pins), "document names collide")
    owner = load_reader("audit_techqa_validation_metadata.py", READER)
    spans = load_reader("audit_techqa_dev_source_spans.py", SPAN_READER)
    with owner["ARCHIVE"].open("rb") as stream:
        before = owner["archive_state"](stream)
        require(before[2] == owner["ARCHIVE_BYTES"] and hashlib.file_digest(stream, "sha256").hexdigest()
                == freeze["archive_sha256"] == owner["ARCHIVE_SHA"], "archive identity differs")
        stream.seek(0)
        with tarfile.open(fileobj=owner["DeadlineStream"](stream, deadline), mode="r|gz") as tar:
            for entry in tar:
                deadline()
                if entry.name != MEMBER:
                    continue
                require(entry.isfile() and entry.size == owner["EXACT_DEV_CORPUS_BYTES"], "corpus member differs")
                with tar.extractfile(entry) as member:
                    reader = owner["MetadataReader"](member, deadline, maximum_bytes=entry.size)
                    documents = gather(reader, pins, spans["read_document_text"])
                    require(reader.finish(entry.size)["sha256"] == freeze["members"][MEMBER], "corpus digest differs")
                break
            else:
                raise ValueError("original corpus member absent")
        require(before == owner["archive_state"](stream), "source archive changed during projection")
    rows = []
    for key, case in cases.items():
        identifier = case["publisher_document_id"]
        rows.append({"selector": {"question_id": key}, "document": {"path": identifier + ".txt", **pins[identifier]},
                     "publisher_document_id": identifier, "source_span": source_span(documents[identifier], case)})
    deadline()
    for identifier, raw in documents.items():
        with (output / "corpus" / (identifier + ".txt")).open("xb") as stream:
            stream.write(raw)
    manifest = {"schema": "airk.private.techqa-dev-gold-source-corpus/1", "archive_sha256": owner["ARCHIVE_SHA"],
                "source_member": MEMBER, "source_member_sha256": freeze["members"][MEMBER],
                "ordered_menu_freeze_sha256": FREEZE, "corpus_root_relative_to_manifest": "corpus",
                "corpus_role": "exact gold documents for indexed source reads only",
                "case_count": len(rows), "source_document_count": len(documents), "withheld_publisher_mismatches": 1,
                "cases": rows, "collector_sha256": sha(Path(__file__).read_bytes())}
    raw = (json.dumps(manifest, sort_keys=True, indent=2) + "\n").encode()
    with (output / "manifest.private.json").open("xb") as stream:
        stream.write(raw)
    print(json.dumps({"sha256": sha(raw), "cases": len(rows), "docs": len(documents),
                      "elapsed_seconds": time.monotonic() - started}))


if __name__ == "__main__":
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("--output", type=Path, required=True)
    stage(parser.parse_args().output)
