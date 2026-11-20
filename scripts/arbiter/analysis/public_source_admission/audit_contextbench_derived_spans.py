# SPDX-License-Identifier: Apache-2.0
"""Project unmatched raw annotations only when the exact body uniquely covers whole source lines."""

import argparse
from collections import Counter
from concurrent.futures import ThreadPoolExecutor
import hashlib
import json
from pathlib import Path
import re
import runpy
import sys
import time


PREVIOUS = "a80f26b9026bf0ee3fdf3dc4f9c10523cb6bfa61b734dcd3d61f9b154636c66e"
OWNER_SHA = "4298c6b464615a305fcbe48e96d5ca92711d4b0a008e7aabbd2f6528e8b79509"
NUMBERED = re.compile(r"[0-9]+\t.*")


def unique_whole_lines(raw, content):
    if not isinstance(content, str) or not content:
        return {"status": "empty_or_invalid_gold"}
    raw_lines = content.strip("\r\n").splitlines()
    if raw_lines and all(NUMBERED.fullmatch(line) for line in raw_lines):
        return {"status": "numbered_formatter_not_eligible"}
    needle = content.encode("utf-8")
    start = raw.find(needle)
    if start < 0:
        return {"status": "raw_body_absent", "occurrences": 0}
    count, next_start = 1, raw.find(needle, start + 1)
    while next_start >= 0:
        count += 1
        next_start = raw.find(needle, next_start + 1)
    if count != 1:
        return {"status": "raw_body_nonunique", "occurrences": count}
    end = start + len(needle)
    at_start = start == 0 or raw[start - 1:start] == b"\n"
    at_end = end == len(raw) or needle.endswith(b"\n") or raw[end:end + 1] == b"\n" or raw[end:end + 2] == b"\r\n"
    if not (at_start and at_end):
        return {"status": "raw_body_inside_line", "occurrences": 1}
    return {"status": "strict_derived_match", "occurrences": 1,
            "derived": {"byte_start": start, "byte_end_exclusive": end,
                        "start_line": raw.count(b"\n", 0, start) + 1,
                        "end_line": raw.count(b"\n", 0, end - 1) + 1}}


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("--directory", required=True, type=Path)
    args = parser.parse_args()
    if not sys.flags.isolated or not sys.dont_write_bytecode:
        parser.error("use isolated PyArrow Python -I -B")
    started = time.monotonic()
    folder = Path(__file__).parent
    path = folder / "audit_contextbench_confirmation_spans.py"
    if hashlib.sha256(path.read_bytes()).hexdigest() != OWNER_SHA:
        raise ValueError("declared alignment owner changed")
    owner = runpy.run_path(str(path))
    census_module = folder / "contextbench_blind_metadata_census.py"
    if hashlib.sha256(census_module.read_bytes()).hexdigest() != owner["OWNER_SHA"]:
        raise ValueError("census owner changed")
    blind = runpy.run_path(str(census_module))
    previous, _ = blind["json_pin"](args.directory / "contextbench-14-declared-lines-r1.private.json", PREVIOUS)
    selection, _ = blind["json_pin"](args.directory / "contextbench-14-selection-r1.private.json", owner["SELECTION_SHA"])
    census, _ = blind["json_pin"](args.directory.parent / "contextbench-blind-metadata-eligibility-r1.json",
                                 owner["CENSUS_SHA"])
    partial = [r for r in previous["rows"] if r["status"] not in ("exact_declared_lines", "exact_numbered_declared_lines")]
    needed = {r["instance_id"] for r in partial}
    candidates = [r for r in selection["rows"] if r["instance_id"] in needed]
    if len(partial) != 24 or len(candidates) != 8 or len(needed) != 8:
        raise ValueError("frozen partial cohort differs")
    annotations = owner["selected_annotations"](blind, Path(census["archive"]["path"]), selection["inputs"]["archive"],
                                                candidates)
    by_id = {r["instance_id"]: r for r in candidates}
    pinned = {(r["repo"], r["base"], r["path"]): r for r in previous["files"]}
    tasks = sorted({(r["repo"], r["base"], r["path"]) for r in partial})
    def read(key):
        row = pinned[key]
        if row["sha256"] is None:
            return None, "previous_blob_unresolved"
        raw, status = owner["fetch_blob"](key, row["metadata"], {})
        if raw is not None and hashlib.sha256(raw).hexdigest() != row["sha256"]:
            return None, "prior_sha256_differs"
        return raw, status
    with ThreadPoolExecutor(max_workers=5) as pool:
        files = dict(zip(tasks, pool.map(read, tasks), strict=True))
    results = []
    for record in partial:
        case = by_id[record["instance_id"]]
        span = annotations[case["original_inst_id"]][record["ordinal"]]
        key = record["repo"], record["base"], record["path"]
        if (key != (case["repo"], case["base_commit"], owner["relative_file"](span["file"], case["repo"]))
                or (span["start_line"], span["end_line"]) != (record["start"], record["end"])):
            raise ValueError("original span locator drift")
        raw, origin = files[key]
        content = span["content"]
        outcome = {"status": origin} if raw is None else unique_whole_lines(raw, content)
        results.append({"original": record, "source_sha256": pinned[key]["sha256"],
                        "gold_sha256": hashlib.sha256(content.encode()).hexdigest(), **outcome})
    print(json.dumps({"schema": "airk.private.contextbench-confirmation-strict-derived/1", "previous": PREVIOUS,
                      "selection": owner["SELECTION_SHA"], "declared_owner_sha256": OWNER_SHA,
                      "collector_sha256": hashlib.sha256(Path(__file__).read_bytes()).hexdigest(),
                      "rows": results, "counts": Counter(r["status"] for r in results),
                      "files_verified": sum(raw is not None for raw, _ in files.values()),
                      "original_cases": len(needed), "admitted": 0, "elapsed_seconds": time.monotonic() - started},
                     sort_keys=True, allow_nan=False))


if __name__ == "__main__":
    main()
