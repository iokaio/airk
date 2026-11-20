# SPDX-License-Identifier: Apache-2.0
"""Materialize every exact-case TechQA menu from the authenticated publisher archive.

Run --layout to enumerate all destinations for checked preflight. The caller
creates the fresh output directories before --stage. No case text, labels,
or menu weights are placed in the source directories.
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
STAGE_SHA = "f27405ca35abb894648f21620421172e222bf89357c064600cf8a57670560808"
CAMPAIGN = HERE.parents[3] / "artifacts/v190/final-plan/exec-20261116T052942Z"
OUTPUT = CAMPAIGN / "techqa-full-menu-corpus-r1"


def require(ok, message):
    if not ok:
        raise ValueError(message)


def plan(freeze):
    cases = {}
    for case_id, row in freeze["cases"].items():
        if row["source_status"] == "withheld_publisher_span_mismatch":
            continue
        ids = row["ordered_document_ids"]
        require(row["source_status"] == "exact_publisher_span" and len(ids) == len(set(ids)) == 50,
                "invalid source status or publisher menu")
        require(all(re.fullmatch(r"[A-Za-z0-9_-]+", doc) and doc in freeze["documents"] for doc in ids),
                "unsupported or unavailable menu document")
        cases[case_id] = ids
    # Choose the lexicographically first exact case, solely by opaque case ID.
    # Keep its authentic documents together for a one-root model-blind fixture
    # setup. Every other case refers to the same canonical copies.
    first = min(cases)
    first_docs = set(cases[first])
    ids = sorted({doc for menu in cases.values() for doc in menu})
    require(len({doc.casefold() for doc in ids}) == len(ids), "case-insensitive file collision")
    paths = {doc: f"{'pilot-menu' if doc in first_docs else 'remaining-documents'}/{doc}.txt" for doc in ids}
    return cases, first, paths


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("--layout", action="store_true", help="print only relative output paths for a checked shell")
    args = parser.parse_args()
    stage_path = HERE / "stage_techqa_exact_gold_corpus.py"
    require(hashlib.sha256(stage_path.read_bytes()).hexdigest() == STAGE_SHA, "gold corpus reader changed")
    stage = runpy.run_path(str(stage_path))
    data = (CAMPAIGN / "techqa-original-dev-ordered-menu-freeze-r1.json").read_bytes()
    require(stage["sha"](data) == stage["FREEZE"], "menu freeze changed")
    metadata = json.loads(data)
    freeze = metadata["menu_freeze"]
    cases, first, paths = plan(freeze)
    require(len(freeze["cases"]) == 77 and len(cases) == 76 and len(paths) == 3526, "menu census differs")
    if args.layout:
        print(json.dumps([*paths.values(), "manifest.private.json"]))
        return
    require(OUTPUT.resolve(strict=True) == OUTPUT and {p.name for p in OUTPUT.iterdir()} == {
        "pilot-menu", "remaining-documents"}, "fresh checked output required")
    for name in ("pilot-menu", "remaining-documents"):
        root = OUTPUT / name
        require(root.resolve(strict=True) == root and not any(root.iterdir()), "fresh source subdirectory required")
    started = time.monotonic()

    def deadline():
        require(time.monotonic() - started < 300, "original TechQA archive deadline expired")

    pins = {doc: freeze["documents"][doc] for doc in paths}
    owner = stage["load_reader"]("audit_techqa_validation_metadata.py", stage["READER"])
    spans = stage["load_reader"]("audit_techqa_dev_source_spans.py", stage["SPAN_READER"])
    with owner["ARCHIVE"].open("rb") as stream:
        before = owner["archive_state"](stream)
        require(before[2] == owner["ARCHIVE_BYTES"] and hashlib.file_digest(stream, "sha256").hexdigest()
                == metadata["archive_sha256"] == owner["ARCHIVE_SHA"], "archive identity differs")
        stream.seek(0)
        with tarfile.open(fileobj=owner["DeadlineStream"](stream, deadline), mode="r|gz") as tar:
            for entry in tar:
                deadline()
                if entry.name != stage["MEMBER"]:
                    continue
                require(entry.isfile() and entry.size == owner["EXACT_DEV_CORPUS_BYTES"], "corpus member differs")
                with tar.extractfile(entry) as member:
                    reader = owner["MetadataReader"](member, deadline, maximum_bytes=entry.size)
                    documents = stage["gather"](reader, pins, spans["read_document_text"])
                    require(reader.finish(entry.size)["sha256"] == metadata["members"][entry.name],
                            "corpus member hash differs")
                break
            else:
                raise ValueError("corpus member missing")
        require(owner["archive_state"](stream) == before, "source archive changed during projection")

    for doc, relative in paths.items():
        deadline()
        with (OUTPUT / relative).open("xb") as member:
            member.write(documents[doc])
    manifest = {
        "schema": "airk.private.techqa-dev-full-menu-corpus/1", "archive_sha256": owner["ARCHIVE_SHA"],
        "source_member": stage["MEMBER"], "source_member_sha256": metadata["members"][stage["MEMBER"]],
        "ordered_menu_freeze_sha256": stage["FREEZE"], "source_sha256": stage["sha"](Path(__file__).read_bytes()),
        "stage_owner_sha256": STAGE_SHA, "case_count": len(cases), "case_document_memberships": sum(map(len, cases.values())),
        "source_document_count": len(paths), "source_bytes": sum(pin["bytes"] for pin in pins.values()),
        "pilot_case": first, "pilot_source_root": "pilot-menu", "withheld_cases": len(freeze["cases"]) - len(cases),
        "ordered_menus": cases, "documents": {doc: {"path": rel, **pins[doc]} for doc, rel in paths.items()},
    }
    raw = (json.dumps(manifest, sort_keys=True, indent=2) + "\n").encode()
    with (OUTPUT / "manifest.private.json").open("xb") as member:
        member.write(raw)
    print(json.dumps({"sha256": stage["sha"](raw), "elapsed_seconds": time.monotonic() - started,
                      "cases": len(cases), "documents": len(documents)}))


if __name__ == "__main__":
    main()
