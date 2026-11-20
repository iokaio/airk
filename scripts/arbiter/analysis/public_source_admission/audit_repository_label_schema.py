# SPDX-License-Identifier: Apache-2.0
"""Verify pinned MultiLang and ContextBench field inventories without materializing case content."""

import argparse
import hashlib
import json
from pathlib import Path
import runpy
import sys
import time

import pyarrow.parquet as parquet


COLLECTOR = "collect_repository_confirmation_candidates.py"
COLLECTOR_SHA = "03e7dd822ed1ea18d905a4cb0e940a80d7bb73955dd06899c78f13e73adf69cc"
CENSUS_SHA = "1b4b0dd4c402bf41a30002ea02ff0fb18a4ea771dc7df81f928338eb5370c4fe"
EXPECTED_FIELDS = ("repo pull_number instance_id issue_numbers base_commit patch test_patch problem_statement "
                   "hints_text all_hints_text commit_urls created_at commit_url rebuild_cmds test_cmds "
                   "print_cmds log_parser FAIL_TO_PASS PASS_TO_PASS docker_image").split()


def schema_only(owner, path, size, sha):
    with owner.regular(path).open("rb") as stream:
        state = owner.archive_state(stream)
        if state[2] != size or hashlib.file_digest(stream, "sha256").hexdigest() != sha:
            raise ValueError("archive identity differs")
        stream.seek(0)
        reader = parquet.ParquetFile(stream)
        fields = [(f.name, str(f.type)) for f in reader.schema_arrow]
        rows = reader.metadata.num_rows
        if owner.archive_state(stream) != state:
            raise ValueError("archive changed while reading metadata")
    return {"bytes": size, "sha256": sha, "rows": rows, "fields": fields}


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("--directory", required=True, type=Path)
    args = parser.parse_args()
    if not sys.flags.isolated or not sys.dont_write_bytecode:
        parser.error("use isolated Python with bytecode disabled")
    start = time.monotonic()
    collector = Path(__file__).with_name(COLLECTOR)
    if hashlib.sha256(collector.read_bytes()).hexdigest() != COLLECTOR_SHA:
        raise ValueError("metadata collector pin differs")
    source = runpy.run_path(str(collector))
    owner = source["load_blind_owner"]()
    shards = []
    for split, size, sha in source["SHARDS"]:
        filename = f"{split}-00000-of-00001.parquet"
        shard = schema_only(owner, args.directory / filename, size, sha)
        if [name for name, _ in shard["fields"]] != EXPECTED_FIELDS:
            raise ValueError("MultiLang schema differs")
        shards.append(dict(shard, split=split))
    census, _ = owner.json_pin(args.directory.parent / "contextbench-blind-metadata-eligibility-r1.json", CENSUS_SHA)
    pin = census["archive"]
    archive = schema_only(owner, Path(pin["path"]), pin["bytes"], pin["sha256"])
    if "gold_context" not in [name for name, _ in archive["fields"]]:
        raise ValueError("expected ContextBench annotation field unavailable")
    print(json.dumps({"schema": "airk.private.repository-source-label-schema/1",
                      "collector_sha256": hashlib.sha256(Path(__file__).read_bytes()).hexdigest(),
                      "multilang": {"revision": source["REVISION"], "shards": shards},
                      "contextbench": {"census_sha256": CENSUS_SHA, "archive": archive,
                                       "provisional": census["counts"]["provisional_after_literal_train_exposure_fence"]},
                      "case_columns_materialized": [], "elapsed_seconds": time.monotonic() - start},
                     sort_keys=True, allow_nan=False))


if __name__ == "__main__":
    main()
