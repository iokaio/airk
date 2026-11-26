# SPDX-License-Identifier: Apache-2.0
"""Compare native CPU and CUDA query vectors against a pinned closed matrix."""

from __future__ import annotations

import argparse
import json
import math
import mmap
from pathlib import Path
import sqlite3
import struct
import sys
from time import monotonic

HERE = Path(__file__).resolve().parent
if str(HERE) not in sys.path:
    sys.path.insert(0, str(HERE))

import common
import search_focus_ab as search
import source_fidelity_audit as fidelity


DIMENSIONS = 384
ASSET = "f3c3a4eed373650ba1e2f4d3d53437f1ecf92d4c50e5994e07e163751923ee4a"
GRAPH = "8d6ee9ccbeeb7a2c9a5dca313cb7b63ba7eb860fdfeab00bed7cc2fb3056bcb4"
PLACEMENT = "3761c7dc65d393a8ac2593b87578fe2bb7a2d714318aad902de2577ec4878801"
RUNTIME = "9433753580c8a370bbea42119aa67ac81306229bcf6876514213f3e2979083b5"
HEADER = 64
ROW = struct.Struct(f"<{DIMENSIONS}f")


def frozen_query(args):
    cohort = json.loads(search.pinned(args.cohort, args.cohort_sha256))
    search.roster(cohort["rows"])
    selected = [row for row in cohort["rows"] if f"{row['repo']}/{row['slot']}" == args.case]
    common.require(len(selected) == 1, "query case is not in the frozen development roster")
    case = selected[0]
    raw = case["arguments"]["query"].encode("utf-8")
    common.require(0 < len(raw) <= 16384, "native query input exceeds its bound")
    repo = "bookbuilder-v2" if case["repo"] == "bookbuilder" else case["repo"]
    return repo, raw


def read_probe(args, query):
    report = json.loads(search.pinned(args.probe, args.probe_sha256))
    common.require(report["schema"] == "airk.private.minilm-cuda-smoke/1"
                   and report["ok"] is True and report["runtime_poisoned"] is False
                   and report["model"] == ASSET and report["gpu_graph"] == GRAPH
                   and report["runtime"] == RUNTIME and report["placement_sha256"] == PLACEMENT,
                   "native embedding probe did not establish the exact CPU/CUDA pair")
    vectors = report["query_embedding"]
    common.require(vectors["input_sha256"] == common.sha256_bytes(query), "native query digest differs")
    cpu, cuda = (vectors[key] for key in ("cpu", "cuda"))
    common.require(all(len(row) == DIMENSIONS and all(math.isfinite(x) for x in row)
                       for row in (cpu, cuda)), "invalid native query coordinates")
    cpu, cuda = (ROW.unpack(ROW.pack(*row)) for row in (cpu, cuda))
    delta = max(abs(a - b) for a, b in zip(cpu, cuda, strict=True))
    dot = math.fsum(a * b for a, b in zip(cpu, cuda, strict=True))
    common.require(delta <= 0.005 and dot >= 0.9995, "query vector parity exceeds the admitted numeric tolerance")
    return cpu, cuda, delta, dot


def rank_rows(args, repository, cpu, cuda):
    started = monotonic()
    writer = json.loads(search.pinned(args.writer, args.writer_sha256))
    common.require(writer["state"] == "closed" and writer["final_stage_idle"] is True
                   and writer["service_exit_code"] == 0, "rank proof needs a clean closed reference")
    index = writer["indexes"][repository]
    database = fidelity.publication_database(repository, index)
    fidelity.check_sidecars(database)
    db_hash = common.sha256_file(database)
    with sqlite3.connect(database.resolve().as_uri() + "?mode=ro&immutable=1", uri=True) as conn:
        conn.execute("PRAGMA trusted_schema=OFF")
        conn.execute("PRAGMA query_only=ON")
        conn.execute("PRAGMA temp_store=MEMORY")
        meta = dict(conn.execute("SELECT key, value FROM meta"))
        rows = list(conn.execute(
            "SELECT DISTINCT v.row, lower(hex(v.embedding_key)) FROM vector_rows v "
            "JOIN units u ON u.embedding_key = v.embedding_key JOIN files f ON f.id = u.file_id ORDER BY v.row"))
    pipeline = meta["vector_pipeline"].hex()
    committed = meta["vector_rows"]
    common.require(meta["vector_dimensions"] == DIMENSIONS and pipeline == index["pipeline_digest"]
                   and meta["sequence"] == int(index["publication_token"].split(":")[2])
                   and len(rows) >= 40 and all(0 <= row < committed for row, _ in rows),
                   "committed row authority or native pipeline differs")
    matrix = database.parent / f"vectors-{meta['vector_generation']}.f32"
    common.require(not matrix.is_symlink() and common.sha256_file(matrix) == args.matrix_sha256,
                   "authenticated closed matrix differs")
    scored_cpu, scored_cuda = [], []
    score_delta = 0.0
    with matrix.open("rb") as file, mmap.mmap(file.fileno(), 0, access=mmap.ACCESS_READ) as memory:
        common.require(len(memory) >= HEADER + committed * ROW.size and memory[:8] == b"AIRKVEC1"
                       and struct.unpack_from("<I", memory, 8)[0] == DIMENSIONS
                       and memory[16:48].hex() == pipeline, "incompatible committed matrix header or extent")
        for row, embedding_key in rows:
            vector = ROW.unpack_from(memory, HEADER + row * ROW.size)
            score_cpu = math.fsum(a * b for a, b in zip(cpu, vector, strict=True))
            score_cuda = math.fsum(a * b for a, b in zip(cuda, vector, strict=True))
            common.require(math.isfinite(score_cpu) and math.isfinite(score_cuda), "nonfinite committed vector row")
            score_delta = max(score_delta, abs(score_cpu - score_cuda))
            scored_cpu.append((score_cpu, -row, embedding_key))
            scored_cuda.append((score_cuda, -row, embedding_key))
    fidelity.check_sidecars(database)
    common.require(common.sha256_file(database) == db_hash and common.sha256_file(matrix) == args.matrix_sha256,
                   "closed publication changed during independent ranking")
    arms = {}
    for name, scores in (("cpu", scored_cpu), ("cuda", scored_cuda)):
        scores.sort(reverse=True)
        arms[name] = [{"row": -row, "embedding_key": key, "score": score}
                      for score, row, key in scores[:40]]
    keys = [[item["embedding_key"] for item in arms[name]] for name in ("cpu", "cuda")]
    return {"publication": index["publication_token"], "source_commit": index["source_commit"],
            "pipeline": pipeline, "db_sha256": db_hash, "committed_rows": committed,
            "scored_live_rows": len(rows), "maximum_score_difference": score_delta,
            "shared_top10": len(set(keys[0][:10]) & set(keys[1][:10])),
            "shared_top40": len(set(keys[0]) & set(keys[1])), "same_top40_order": keys[0] == keys[1],
            "rankings": arms, "elapsed_seconds": monotonic() - started}


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    modes = parser.add_subparsers(dest="mode", required=True)
    prepare = modes.add_parser("prepare")
    compare = modes.add_parser("compare")
    for cmd in (prepare, compare):
        cmd.add_argument("--cohort", type=Path, required=True)
        cmd.add_argument("--cohort-sha256", required=True)
        cmd.add_argument("--case", required=True)
        cmd.add_argument("--output", type=Path, required=True)
    for name in ("writer", "probe"):
        compare.add_argument(f"--{name}", type=Path, required=True)
        compare.add_argument(f"--{name}-sha256", required=True)
    compare.add_argument("--matrix-sha256", required=True)
    args = parser.parse_args()
    repository, query = frozen_query(args)
    if args.mode == "prepare":
        with args.output.open("xb") as stream:
            stream.write(query)
        return
    cpu, cuda, delta, dot = read_probe(args, query)
    rank = rank_rows(args, repository, cpu, cuda)
    search.write_new(args.output, {"schema": "airk.private.closed-matrix-cross-route/1",
        "case": args.case, "repository": repository,
        "input_sha256": {"cohort": args.cohort_sha256, "query": common.sha256_bytes(query),
                         "native_probe": args.probe_sha256, "writer": args.writer_sha256,
                         "matrix": args.matrix_sha256},
        "query_maximum_coordinate_difference": delta, "query_dot": dot, "result": rank,
        "scope": "Independent double-precision dots over closed live rows. This does not reproduce each row's writer route."})


if __name__ == "__main__":
    try:
        main()
    except (common.HarnessError, OSError, ValueError, TypeError, KeyError, sqlite3.Error) as error:
        print(str(error), file=sys.stderr)
        sys.exit(1)
