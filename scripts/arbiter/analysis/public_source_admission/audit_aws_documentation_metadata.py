# SPDX-License-Identifier: Apache-2.0
"""Screen pinned public AWS documentation questions and emit private identifiers.

Run with isolated Python -I -B -S and --count through the checked Windows
human shell. The caller owns the receipt. Never print question, answer or
document text. A source match only identifies a candidate for later review.
"""

import argparse
from collections import Counter, defaultdict, deque
from concurrent.futures import ThreadPoolExecutor
import csv
import hashlib
import io
import json
import re
import time
import unicodedata
from urllib.parse import quote
from urllib.request import Request, urlopen


REPOSITORY = "https://github.com/siagholami/aws-documentation"
REVISION = "2d06ee9011f3192b2ff38c09f04e01f1ea9e0191"
PUBLICATION = "https://www.mdpi.com/2076-3417/12/1/111"
RAW = f"https://raw.githubusercontent.com/siagholami/aws-documentation/{REVISION}/"
TREE = f"https://api.github.com/repos/siagholami/aws-documentation/git/trees/{REVISION}?recursive=1"
MEMBER = "QA_true.csv"
CSV_PIN = (16_596, "548c26d6bbf46c3b80a244c5ad7c13a517f8395eedbf344058ec38a249ea7dd6")
QA_LICENSE = (18_650, "7e7170e3cebf88a9f60c7b8421418323c09304da1af4d5e90f4da1dc1c8a2661")
GUIDE_LICENSE = "7785b9042bc90dad09ae994201568bbf395d8204"
KENDRA_FAMILY = "amazon-kendra-developer-guide"
KENDRA_LICENSE = "1bb4f21e5d8b5b21b6952c0b9bf2e6220d9a56be"
PATH_ROSTER_SHA = "188868f0efea3a750929901444cd250311e4f91a1a69a1a6b277444bea9bf136"
BODY_ROSTER_SHA = "29e501a8807e31c8dc09a26118bb19dcd751072e48ed91f261903d783465e64d"
DOC_PATH = re.compile(r"[a-z0-9-]+/[A-Za-z0-9_.-]+[.]md")
GIT_HASH = re.compile(r"[a-f0-9]{40}")
READ_SIZE = 64 << 10


def require(condition, message):
    if not condition:
        raise ValueError(message)


def digest(data):
    return hashlib.sha256(data).hexdigest()


def check_deadline(deadline):
    remaining = deadline - time.monotonic()
    if remaining <= 0:
        raise TimeoutError("original AWS source screening deadline expired")
    return remaining


def fetch(url, limit, deadline):
    request = Request(url, headers={"User-Agent": "AIRK-public-source-screen"})
    result = bytearray()
    with urlopen(request, timeout=min(12, check_deadline(deadline))) as response:
        require(response.geturl() == url and response.status == 200, "AWS public response changed")
        while True:
            check_deadline(deadline)
            chunk = response.read(min(READ_SIZE, limit + 1 - len(result)))
            if not chunk:
                break
            result.extend(chunk)
            require(len(result) <= limit, "AWS public response exceeds its limit")
    check_deadline(deadline)
    return bytes(result)


def pinned_raw(path, pin, deadline):
    length, sha = pin
    data = fetch(RAW + quote(path, safe="/"), length, deadline)
    require(len(data) == length and digest(data) == sha, "AWS raw artifact pin differs")
    return data


def document_path(label):
    # The pinned CSV has one unquoted label with a space after the delimiter.
    if label == " amazon-ec2-user-guide/AmazonEBS.md":
        label = label[1:]
    require(DOC_PATH.fullmatch(label), "invalid or ambiguous AWS document label")
    family, leaf = label.split("/", 1)
    return family, f"documents/{family}/doc_source/{leaf}"


def read_rows(deadline):
    data = pinned_raw(MEMBER, CSV_PIN, deadline)
    reader = csv.DictReader(io.StringIO(data.decode("utf-8-sig"), newline=""), strict=True)
    require(reader.fieldnames == ["Question", "Answer_True", "Yes_No_True", "Document_True"],
            "AWS QA columns differ")
    rows = list(reader)
    require(len(rows) == 100, "AWS QA row count differs")
    for row in rows:
        require(None not in row and all(row.values()), "empty or malformed AWS QA row")
    require(Counter(row["Yes_No_True"] for row in rows) == {"None": 68, "Yes": 22, "No": 10},
            "AWS QA labels differ")
    return rows


def roster_digest(rows):
    content = "".join("\t".join(map(str, row)) + "\n" for row in sorted(rows))
    return digest(content.encode("utf-8"))


def read_tree(rows, deadline):
    tree = json.loads(fetch(TREE, 16 << 20, deadline))
    require(tree["sha"] == REVISION and tree["truncated"] is False and len(tree["tree"]) == 37_602,
            "immutable AWS tree identity or census differs")
    index = {}
    for entry in tree["tree"]:
        path = entry["path"]
        require(path not in index, "duplicate AWS tree path")
        index[path] = entry
    docs = {}
    rights = {}
    for row in rows:
        family, path = document_path(row["Document_True"])
        source = index[path]
        require(source["type"] == "blob" and source["mode"] == "100644"
                and GIT_HASH.fullmatch(source["sha"]) and 0 < source["size"] <= 2 << 20,
                "AWS document tree entry differs")
        docs[path] = (source["sha"], source["size"])
        license_path = f"documents/{family}/LICENSE"
        family_grant = index[license_path]
        expected = KENDRA_LICENSE if family == KENDRA_FAMILY else GUIDE_LICENSE
        require(family_grant["type"] == "blob" and family_grant["sha"] == expected,
                "AWS family license tree entry differs")
        rights[family] = {
            "path": license_path,
            "git_blob_sha1": expected,
            "bytes": family_grant["size"],
            "document_license": "unconfirmed_distinct_family_license" if family == KENDRA_FAMILY
            else "CC-BY-SA-4.0",
        }
    require(len(docs) == 56 and len(rights) == 14
            and roster_digest((path, sha, size) for path, (sha, size) in docs.items()) == PATH_ROSTER_SHA,
            "AWS document roster differs")
    return docs, rights


def normalize_markdown(text):
    text = re.sub(r"\s+", " ", unicodedata.normalize("NFC", text)).strip()
    text = re.sub(r"!?\[([^\]]+)\]\([^)]+\)", r"\1", text)
    return re.sub(r"[*_+\x60\[\]\\]", "", text).casefold()


def inspect_document(path, pin, deadline):
    git_sha, length = pin
    data = fetch(RAW + quote(path, safe="/"), length, deadline)
    git = hashlib.sha1(f"blob {len(data)}\0".encode("ascii"), usedforsecurity=False)
    git.update(data)
    require(len(data) == length and git.hexdigest() == git_sha, "AWS source document blob differs")
    return data.decode("utf-8"), digest(data)


def screening_rows(rows, docs, deadline):
    bodies = {}
    body_pins = {}
    with ThreadPoolExecutor(max_workers=10) as workers:
        pending = {
            path: workers.submit(inspect_document, path, pin, deadline)
            for path, pin in docs.items()
        }
        for path, future in pending.items():
            body, sha = future.result()
            bodies[path] = body, normalize_markdown(body)
            body_pins[path] = sha
    require(roster_digest((path, sha, length, body_pins[path])
                          for path, (sha, length) in docs.items()) == BODY_ROSTER_SHA,
            "AWS source content roster differs")
    candidates = []
    for row_index, row in enumerate(rows):
        check_deadline(deadline)
        family, path = document_path(row["Document_True"])
        body, markdown = bodies[path]
        answer = row["Answer_True"]
        normalized = normalize_markdown(answer)
        if answer in body:
            match_type = "literal"
        elif normalized and normalized in markdown:
            match_type = "markdown_normalized"
        else:
            continue
        blob, length = docs[path]
        candidates.append({
            "selector": {"member": MEMBER, "row_index": row_index},
            "question_sha256": digest(row["Question"].encode("utf-8")),
            "document": {"path": path, "git_blob_sha1": blob, "sha256": body_pins[path],
                         "bytes": length, "family": family},
            "match_type": match_type,
            "needs_native_alignment": match_type == "markdown_normalized",
            "exposure_reviewed": False,
            "source_rights_resolved": False,
            "acceptable_set_complete": False,
            "native_visibility_verified": False,
        })
    require(Counter(row["match_type"] for row in candidates) == {"literal": 53, "markdown_normalized": 14},
            "AWS source-match census differs")
    return candidates


def fixed_order(value):
    return digest(f"airk.private.aws-source-screen/1:{REVISION}:{value}".encode("utf-8"))


def rotate(groups):
    queues = deque(deque(groups[key]) for key in sorted(groups, key=fixed_order))
    while queues:
        members = queues.popleft()
        yield members.popleft()
        if members:
            queues.append(members)


def select_candidates(candidates, count):
    # Label tier comes first. Within a tier, alternate product families and
    # their documents using only path identities and immutable CSV row indices.
    selected = []
    seen = set()
    for tier in ("literal", "markdown_normalized"):
        families = defaultdict(lambda: defaultdict(list))
        for row in sorted(candidates, key=lambda row: fixed_order(row["selector"]["row_index"])):
            if row["match_type"] == tier:
                document = row["document"]
                families[document["family"]][document["path"]].append(row)
        for row in rotate({family: rotate(docs) for family, docs in families.items()}):
            question = row["question_sha256"]
            if question not in seen:
                seen.add(question)
                selected.append(row)
    require(count <= len(selected),
            f"AWS distinct source-matched candidate capacity is {len(selected)}, requested {count}")
    return selected[:count], len(selected)


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("--count", required=True, type=int, help="exact number of candidate rows")
    parser.add_argument("--seconds", type=int, default=45, help="bounded read allowance (1–120)")
    args = parser.parse_args()
    if not 1 <= args.count <= 67 or not 1 <= args.seconds <= 120:
        parser.error("require 1–67 candidates and a 1–120 second allowance")
    started = time.monotonic()
    deadline = started + args.seconds
    rows = read_rows(deadline)
    pinned_raw("LICENSE", QA_LICENSE, deadline)
    docs, rights = read_tree(rows, deadline)
    candidates = screening_rows(rows, docs, deadline)
    selected, capacity = select_candidates(candidates, args.count)
    check_deadline(deadline)
    receipt = {
        "schema": "airk.private.aws-documentation-technical-support-screen/1",
        "complete": True,
        "source": {"repository": REPOSITORY, "revision": REVISION, "publication": PUBLICATION,
                   "member": MEMBER, "bytes": CSV_PIN[0], "sha256": CSV_PIN[1],
                   "license": {"path": "LICENSE", "id": "CC-BY-4.0", "bytes": QA_LICENSE[0],
                               "sha256": QA_LICENSE[1]},
                   "document_roster_sha256": PATH_ROSTER_SHA,
                   "body_roster_sha256": BODY_ROSTER_SHA},
        "source_census": {"rows": len(rows), "mapped_rows": len(rows), "documents": len(docs),
                          "families": len(rights), "matching_rows": dict(Counter(
                              row["match_type"] for row in candidates)),
                          "distinct_candidate_capacity": capacity},
        "selected_count": len(selected),
        "selected_families": len({row["document"]["family"] for row in selected}),
        "selected_match_types": dict(Counter(row["match_type"] for row in selected)),
        "selected_family_licenses": {family: rights[family] for family in
                                     sorted({row["document"]["family"] for row in selected})},
        "exposure_reviewed": False,
        "source_rights_resolved": False,
        "acceptable_set_complete": False,
        "native_visibility_verified": False,
        "new_human_judgments": False,
        "qualification": False,
        "candidates": selected,
        "elapsed_seconds": time.monotonic() - started,
    }
    print(json.dumps(receipt, separators=(",", ":"), sort_keys=True, allow_nan=False))


if __name__ == "__main__":
    main()
