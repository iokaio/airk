# SPDX-License-Identifier: Apache-2.0
"""Compare the active ordered TechQA menus with retained TRAIN metadata."""
from collections import Counter
import hashlib
import json
from pathlib import Path
import runpy
import time


HERE = Path(__file__).resolve().parent
MENU_SHA = "50f876ebb9cf3a8328314e7be3e01f343b2d1b67cd7979fe2aaceeb2c82064f7"
SOURCE_SHA = "8b0aad68fc2220b1aa36ee9c43f51e332c9c3e3730b1b998a8cc46909d4f51e3"
FILES_SHA = "3f5f8b2c1ce3b217cd343a91c342f4b7a07e354995788be6cb0f44defb5261cb"


def pinned_module(path, digest):
    if hashlib.sha256(path.read_bytes()).hexdigest() != digest:
        raise ValueError("metadata reader bytes differ")
    return runpy.run_path(str(path))


def compare(metadata, gold, cases, documents, file_hashes, normalize_url):
    def keys(identifier):
        row = metadata[identifier]
        return {"canonical_url": normalize_url(row["canonicalUrl"]), "upstream_id": row["sourceDocumentId"],
                "literal_product_id": row["productId"], "literal_product_name": row["productName"]}

    training = {}
    for doc in gold:
        for field, value in keys(doc).items():
            if value:
                training.setdefault(field, set()).add(value)
    active = {}
    for key, case in cases.items():
        if case["source_status"] != "exact_publisher_span":
            continue
        menu = case["ordered_document_ids"]
        if case["publisher_document_id"] not in menu or set(menu) & gold:
            raise ValueError("previous gold ID fence differs")
        overlaps = Counter()
        missing = Counter()
        gold_matches = []
        for doc in menu:
            fields = keys(doc)
            for field, value in fields.items():
                if not value:
                    missing[field] += 1
                elif value in training.get(field, set()):
                    overlaps[field] += 1
                    if doc == case["publisher_document_id"]:
                        gold_matches.append(field)
            if documents[doc]["sha256"] in file_hashes:
                overlaps["retained_file_bytes"] += 1
                if doc == case["publisher_document_id"]:
                    gold_matches.append("retained_file_bytes")
        active[key] = {"menu_matches": dict(overlaps), "menu_missing_fields": dict(missing),
                       "gold_matches": sorted(gold_matches)}
    return active


def main():
    started = time.monotonic()

    def deadline():
        if time.monotonic() - started >= 300:
            raise TimeoutError("TechQA exposure audit exceeded 300 seconds")

    owner = pinned_module(HERE / "audit_mtrag_cloud_admission.py", SOURCE_SHA)
    collector = pinned_module(owner["COLLECTOR"], owner["COLLECTOR_SHA"])
    parser = pinned_module(collector["READER"], collector["READER_SHA"])
    menu_path = owner["CAMPAIGN"] / "techqa-original-dev-ordered-menu-freeze-r1.json"
    freeze, menu_pin = parser["read_checked_json"](menu_path, MENU_SHA)
    descriptors = owner["SOURCE_ROOT"] / "split-clone-chronology-r1/source-file-descriptors.json"
    rows, file_pin = parser["read_checked_json"](descriptors, FILES_SHA)
    if len(rows) != 1514 or any(not row["sha256"] for row in rows):
        raise ValueError("file descriptor closure differs")
    metadata, train, _, exposure_pins = owner["techqa_exposure"](parser, collector["object_projection"], deadline)
    menus = freeze["menu_freeze"]
    active = compare(metadata, train, menus["cases"], menus["documents"], {r["sha256"] for r in rows},
                     owner["source_url"])
    if len(active) != 76:
        raise ValueError("active menu count differs")
    deadline()
    print(json.dumps({"schema": "airk.private.techqa-menu-train-exposure/1", "menus": menu_pin,
                      "retained_file_descriptors": file_pin, "train_gold_documents": len(train),
                      "exposure_pins": exposure_pins, "cases": active, "elapsed_seconds": time.monotonic()-started},
                     sort_keys=True, allow_nan=False))


if __name__ == "__main__":
    main()
