# SPDX-License-Identifier: Apache-2.0
"""Inventory the complete revision-pinned AWS publisher corpus, without opening QA rows."""
import argparse
from collections import Counter, defaultdict
from concurrent.futures import ThreadPoolExecutor
import hashlib
import json
from pathlib import Path, PurePosixPath
import runpy
import time
from urllib.parse import quote

ROOT = Path(__file__).resolve().parents[4]
OWNER = Path(__file__).with_name("audit_aws_documentation_metadata.py")
OWNER_SHA = "8e6cdd9d312907d65bcc8f906efbbe2009ce1c308ff57d9077bbf300968b2f14"
CAMPAIGN = ROOT / "artifacts/v190/final-plan/exec-20261116T052942Z"
ACTIVE = CAMPAIGN / "aws-unique-source-corpus-r1/manifest.private.json"
ACTIVE_SHA = "ef2844ba2406de3b57ac1f5a6b13e582320bacae27779b704d38d4786375e882"
NAMES = ("LICENSE", "LICENSE-SUMMARY", "LICENSE-SAMPLECODE", "NOTICE", "COPYING")


def digest(raw):
    return hashlib.sha256(raw).hexdigest()


def git_blob(raw):
    return hashlib.sha1(f"blob {len(raw)}\0".encode() + raw, usedforsecurity=False).hexdigest()


def kind(path):
    parts = PurePosixPath(path).parts
    if len(parts) >= 4 and parts[2] == "doc_source":
        return "guide_topic" if parts[-1].lower().endswith(".md") else "guide_resource"
    if len(parts) == 3 and parts[-1].upper() in NAMES:
        return "guide_notice"
    return "other_guide_path"


def inventory():
    if digest(OWNER.read_bytes()) != OWNER_SHA or digest(ACTIVE.read_bytes()) != ACTIVE_SHA:
        raise ValueError("retained publisher fetch owner or active roster changed")
    api = runpy.run_path(str(OWNER))
    active = json.loads(ACTIVE.read_bytes())
    deadline = time.monotonic() + 180
    raw = api["fetch"](api["TREE"], 16 << 20, deadline)
    tree = json.loads(raw)
    if tree["sha"] != api["REVISION"] or tree["truncated"] or len(tree["tree"]) != 37602:
        raise ValueError("publisher tree identity differs")
    entries = {}
    groups = defaultdict(list)
    notices = {}
    for row in tree["tree"]:
        path = row["path"]
        if row["type"] != "blob" or not path.startswith("documents/"):
            continue
        parts = PurePosixPath(path).parts
        if len(parts) < 3 or ".." in parts or path in entries or "\\" in path or row["size"] < 0:
            raise ValueError("unsafe publisher document location")
        record = {"bytes":row["size"],"git_blob":row["sha"],"mode":row["mode"],"kind":kind(path)}
        entries[path] = record
        family = parts[1]
        groups[family].append(path)
        if record["kind"] == "guide_notice":
            notices.setdefault(row["sha"], path)
    def read_notice(item):
        blob, path = item
        raw = api["fetch"](api["RAW"] + quote(path, safe="/"), 128 << 10, deadline)
        if git_blob(raw) != blob or len(raw) != entries[path]["bytes"]:
            raise ValueError("original guide notice changed")
        text = raw.decode("utf-8-sig", errors="strict")
        return blob, {"sha256":digest(raw), "text":text}
    with ThreadPoolExecutor(max_workers=6) as executor:
        texts = dict(executor.map(read_notice, sorted(notices.items())))
    families = {}
    for family, paths in sorted(groups.items()):
        notice_rows = {PurePosixPath(p).name: entries[p]["git_blob"] for p in paths if entries[p]["kind"]=="guide_notice"}
        families[family] = {"paths":len(paths),"bytes":sum(entries[p]["bytes"] for p in paths),
                            "kinds":dict(Counter(entries[p]["kind"] for p in paths)), "notices":notice_rows}
    cases = []
    for case in active["cases"]:
        doc = case["document"]
        row = entries[doc["path"]]
        if (row["git_blob"], row["bytes"], row["kind"]) != (doc["git_blob_sha1"], doc["bytes"], "guide_topic"):
            raise ValueError("known positive differs from full tree")
        cases.append({"selector":case["selector"], "question_sha256":case["question_sha256"],
                      "source_path":doc["path"], "menu_scope":"documents/"})
    if len(cases) != 41 or len({c["source_path"] for c in cases}) != 27:
        raise ValueError("active roster differs")
    readme = api["fetch"](api["RAW"]+"README.md", 32 << 10, deadline)
    readme_row = next(row for row in tree["tree"] if row["path"]=="README.md")
    if git_blob(readme)!=readme_row["sha"] or b"25,175 documents" not in readme:
        raise ValueError("publisher corpus description changed")
    return {"schema":"airk.private.aws-full-source-menu-inventory/1","repository":api["REPOSITORY"],
            "revision":api["REVISION"],"tree_response_sha256":digest(raw),
            "repository_readme_sha256":digest(readme),"publisher_reported_documents":25175,
            "canonical_scope":"documents/","complete_tree_entries":len(tree["tree"]),
            "file_count":len(entries),"bytes":sum(r["bytes"] for r in entries.values()),
            "families":families,"source_files":entries,"notice_blobs":texts,"cases":cases,
            "active_roster_sha256":ACTIVE_SHA,"known_positive_document_count":27,
            "all_native_paths_verified":False,"rights_review_complete":False,"model_scoring":False}


if __name__ == "__main__":
    p=argparse.ArgumentParser(description=__doc__)
    p.parse_args()
    print(json.dumps(inventory(),sort_keys=True,separators=(",",":"),ensure_ascii=True))
