# SPDX-License-Identifier: Apache-2.0
"""Freeze ContextBench confirmation identities before projecting their annotated evidence."""

import argparse
from collections import Counter, defaultdict
import hashlib
import json
from pathlib import Path
import re
import runpy
import sys
import time

import pyarrow.parquet as parquet


CENSUS_SHA = "1b4b0dd4c402bf41a30002ea02ff0fb18a4ea771dc7df81f928338eb5370c4fe"
OWNER_SHA = "a2106ef61e28edb61dbf77e6d26a1a074dc851e8c40a893a3efa46e8f9203d55"


def select(owner, rows, train):
    metadata = []
    claims, leaves, bases = set(), set(), set()
    leaf_names, base_names = defaultdict(set), defaultdict(set)
    issue_counts = Counter(r["original_inst_id"] for r in rows)
    instance_counts = Counter(r["instance_id"] for r in rows)
    train_ids = {r["instance_id"] for r in rows if r["original_inst_id"] in train}
    for row in rows:
        repo, _ = owner["repository"](row["repo"], row["repo_url"])
        raw = row["base_commit"]
        base = raw.lower() if isinstance(raw, str) and re.fullmatch("[0-9a-fA-F]{40}", raw) else None
        if repo:
            leaf_names[repo.split("/")[1]].add(repo)
            if base:
                base_names[base].add(repo)
        if row["original_inst_id"] in train:
            claims.update(owner["credible_repository_claims"](row))
            if base:
                bases.add(base)
        metadata.append((row, repo, base))
    leaves.update(name.split("/")[1] for name in claims)
    output = []
    for row, repo, base in metadata:
        issue, instance = row["original_inst_id"], row["instance_id"]
        if not repo or not base or issue in train or issue in train_ids or instance in train_ids:
            continue
        leaf = repo.split("/")[1]
        if (issue_counts[issue] == instance_counts[instance] == 1 and repo not in claims and leaf not in leaves
                and base not in bases and len(leaf_names[leaf]) == len(base_names[base]) == 1):
            output.append(dict(original_inst_id=issue, instance_id=instance, repo=repo, base_commit=base))
    return output


SELECTION_SHA = "51adc46cedf7f9d8803a91e790dffae5c2cfbabcc2c9018bae9221ae67dc722b"
SCREEN_SHA = "c8c5a96c858819e8218d43edc383c2e78441be616aa100554f2e44f11edb00ad"
CACHE_SHA = "155f576ca8f56121f8acfe0ea139c93066723e4feac1312db605c394765f1039"


def relative_file(value, repo):
    if not isinstance(value, str):
        return None
    root = "/workspace/" + repo.replace("/", "__") + "__0.1/"
    if value.lower().startswith(root.lower()):
        value = value[len(root):]
    elif value.startswith("/testbed/"):
        value = value[len("/testbed/"):]
    parts = value.split("/")
    if (any(part in ("", ".", "..") for part in parts) or ":" in value or "\\" in value
            or len(value) > 1024 or any(ord(c) < 32 for c in value)):
        return None
    return value


def compare(content, first, last, raw):
    if not isinstance(content, str) or type(first) != int or type(last) != int or not 1 <= first <= last:
        return "invalid_annotation"
    if not content:
        return "empty_annotation"
    try:
        lines = raw.decode("utf-8").replace("\r\n", "\n").split("\n")
    except UnicodeDecodeError:
        return "source_not_utf8"
    if lines and lines[-1] == "":
        lines.pop()
    if last > len(lines):
        return "out_of_bounds"
    text = content.replace("\r\n", "\n")
    target = "\n".join(lines[first - 1:last])
    if text in (target, target + "\n"):
        return "exact_declared_lines"
    numbered = text.removesuffix("\n").split("\n")
    if len(numbered) == last - first + 1 and all(
            line.startswith(f"{number}\t") for number, line in enumerate(numbered, first)):
        if [line.split("\t", 1)[1] for line in numbered] == lines[first - 1:last]:
            return "exact_numbered_declared_lines"
    return "declared_lines_differ"


def selected_annotations(owner, archive, expected, rows):
    ids = {row["original_inst_id"] for row in rows}
    with owner["regular"](archive).open("rb") as source:
        before = owner["archive_state"](source)
        if hashlib.file_digest(source, "sha256").hexdigest() != expected:
            raise ValueError("archive changed since selection")
        source.seek(0)
        table = parquet.read_table(source, columns=["original_inst_id", "gold_context"],
                                   filters=[("original_inst_id", "in", sorted(ids))], use_threads=False)
        selected_ids = table["original_inst_id"].to_pylist()
        if len(selected_ids) != len(ids) or set(selected_ids) != ids or owner["archive_state"](source) != before:
            raise ValueError("filtered annotation identity mismatch")
        return {key: json.loads(table["gold_context"][idx].as_py()) for idx, key in enumerate(selected_ids)}


def blob_metadata(helper, gh, files):
    requests, request_keys = [], {}
    grouped = defaultdict(list)
    for repo, base, path in sorted(files):
        grouped[repo, base].append(path)
    for (repo, base), paths in grouped.items():
        for start in range(0, len(paths), 10):
            key = str(len(requests))
            keys = [(repo, base, path) for path in paths[start:start + 10]]
            fields = ["nameWithOwner visibility"]
            for i, (_, _, path) in enumerate(keys):
                expression = json.dumps(base + ":" + path)
                fields.append(f"p{i}:object(expression:{expression}) {{ ... on Blob {{ oid byteSize }} }}")
            requests.append((key, repo, " ".join(fields)))
            request_keys[key] = keys
    values, batches = helper["graph_batches"](gh, requests, 4)
    metadata = {}
    for key, keys in request_keys.items():
        data = values.get(key) or {}
        for i, file in enumerate(keys):
            v = data.get(f"p{i}") or {}
            if (data.get("visibility") == "PUBLIC" and (data.get("nameWithOwner") or "").lower() == file[0]
                    and helper["OID"].fullmatch(v.get("oid") or "") and type(v.get("byteSize")) == int):
                metadata[file] = v
    return metadata, batches


def fetch_blob(key, metadata, cache):
    import urllib.error
    from urllib.parse import quote
    from urllib.request import urlopen

    if not metadata:
        return None, "pinned_blob_unresolved"
    size = metadata["byteSize"]
    if not 0 <= size <= 2_000_000:
        return None, "size_refusal"
    repo, base, path = key
    try:
        if key in cache:
            raw = Path(cache[key]["path"]).read_bytes()
            if hashlib.sha256(raw).hexdigest() != cache[key]["sha256"]:
                return None, "retained_file_hash_differs"
            origin = "retained_blob"
        else:
            with urlopen(f"https://raw.githubusercontent.com/{repo}/{base}/{quote(path, safe='/')}",
                         timeout=12) as response:
                raw = response.read(size + 1)
            origin = "public_blob"
    except (OSError, TimeoutError) as error:
        return None, f"source_read_{type(error).__name__}"
    identity = hashlib.sha1(f"blob {len(raw)}\0".encode() + raw, usedforsecurity=False).hexdigest()
    if len(raw) != size or identity != metadata["oid"]:
        return None, "blob_identity_differs"
    return raw, origin


def span_audit(owner, directory, archive, selected, gh):
    prior, _ = owner["json_pin"](directory / "contextbench-14-selection-r1.private.json", SELECTION_SHA)
    if prior["rows"] != selected:
        raise ValueError("sealed selection differs")
    script = Path(__file__).with_name("screen_repository_confirmation_upstreams.py")
    if hashlib.sha256(script.read_bytes()).hexdigest() != SCREEN_SHA:
        raise ValueError("GitHub query helper differs")
    helper = runpy.run_path(str(script))
    helper["pinned_bytes"](gh, helper["GH_SHA"], 64 << 20)
    cache_path = archive.parent / "full450-source-audit-r1/source-observations.json"
    observations = json.loads(helper["pinned_bytes"](cache_path, CACHE_SHA, 2 << 20))
    cache = {(r["key"][0].lower(), *r["key"][1:]): r["pin"] for r in observations if r.get("pin")}
    annotations = selected_annotations(owner, archive, prior["inputs"]["archive"], selected)
    spans, by_file = [], defaultdict(list)
    for row in selected:
        key = row["original_inst_id"]
        gold = annotations[key]
        if not isinstance(gold, list) or not gold:
            raise ValueError("selected case has malformed annotation collection")
        for ordinal, span in enumerate(gold):
            where = relative_file(span.get("file"), row["repo"])
            record = dict(instance_id=row["instance_id"], ordinal=ordinal, repo=row["repo"], base=row["base_commit"],
                          path=where, start=span.get("start_line"), end=span.get("end_line"), status="invalid_path")
            spans.append(record)
            if where:
                by_file[row["repo"], row["base_commit"], where].append((record, span.get("content")))
    metadata, batches = blob_metadata(helper, gh, by_file)
    if sum(v["byteSize"] for v in metadata.values() if 0 <= v["byteSize"] <= 2_000_000) > 128 << 20:
        raise ValueError("total transfer budget exceeded")
    def verify(key):
        raw, origin = fetch_blob(key, metadata.get(key), cache)
        for record, content in by_file[key]:
            record["status"] = origin if raw is None else compare(content, record["start"], record["end"], raw)
        return {"repo": key[0], "base": key[1], "path": key[2], "metadata": metadata.get(key),
                "origin": origin, "sha256": hashlib.sha256(raw).hexdigest() if raw is not None else None}
    from concurrent.futures import ThreadPoolExecutor
    with ThreadPoolExecutor(max_workers=5) as executor:
        files = list(executor.map(verify, sorted(by_file)))
    return {"schema": "airk.private.contextbench-confirmation-declared-lines/1",
            "input_selection_sha256": SELECTION_SHA, "github_helper_sha256": SCREEN_SHA,
            "cache_index_sha256": CACHE_SHA, "batches": batches, "files": files,
            "rows": spans, "counts": Counter(row["status"] for row in spans), "cases": len(annotations),
            "native_visible_cases": 0, "admitted": 0}


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("--directory", type=Path, required=True)
    parser.add_argument("--verify-spans", action="store_true")
    parser.add_argument("--gh", type=Path)
    args = parser.parse_args()
    if not sys.flags.isolated or not sys.dont_write_bytecode:
        parser.error("use isolated PyArrow Python -I -B")
    started = time.monotonic()
    path = Path(__file__).with_name("contextbench_blind_metadata_census.py")
    if hashlib.sha256(path.read_bytes()).hexdigest() != OWNER_SHA:
        raise ValueError("blind owner changed")
    owner = runpy.run_path(str(path))
    census, _ = owner["json_pin"](args.directory.parent / "contextbench-blind-metadata-eligibility-r1.json", CENSUS_SHA)
    train, _ = owner["json_pin"](Path(census["train_selection"]["path"]), owner["SELECTION_SHA"])
    archive = Path(census["archive"]["path"])
    rows = owner["projected_rows"](archive, parquet)
    train_ids = set(train["selected_ids"])
    counts = owner["census"](rows, train_ids)
    if counts != census["counts"]:
        raise ValueError("retained census differs")
    selected = select(owner, rows, train_ids)
    if owner["clustered"]([(r["repo"], r["base_commit"], r["original_inst_id"]) for r in selected]) != counts["provisional_after_literal_train_exposure_fence"]:
        raise ValueError("provisional selection differs")
    if len(selected) != 14 or len({r["original_inst_id"] for r in selected}) != 14:
        raise ValueError("case identity roster differs")
    if args.verify_spans:
        if args.gh is None:
            parser.error("--verify-spans requires --gh")
        result = span_audit(owner, args.directory, archive, selected, args.gh)
        result.update(elapsed_seconds=time.monotonic() - started,
                      collector_sha256=hashlib.sha256(Path(__file__).read_bytes()).hexdigest())
        print(json.dumps(result, sort_keys=True, allow_nan=False))
        return
    print(json.dumps({"schema": "airk.private.contextbench-confirmation-selection/1", "inputs": {
        "census": CENSUS_SHA, "archive": census["archive"]["sha256"], "train": owner["SELECTION_SHA"]},
        "rows": selected, "elapsed_seconds": time.monotonic() - started}, sort_keys=True, allow_nan=False))


if __name__ == "__main__":
    main()
