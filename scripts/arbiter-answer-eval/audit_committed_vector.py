# SPDX-License-Identifier: Apache-2.0
"""Authenticate an exact retained embedding input and compare its committed f32 row."""

from __future__ import annotations

import argparse
from hashlib import sha256
import json
import math
from pathlib import Path, PurePosixPath
import sqlite3
import struct
import sys

HERE = Path(__file__).resolve().parent
if str(HERE) not in sys.path:
    sys.path.insert(0, str(HERE))

from closed_matrix_cross_route import DIMENSIONS, HEADER, ROW, read_probe
import common
import search_focus_ab as search
import source_fidelity_audit as fidelity


def embedding_key(pipeline, data):
    fields = (b"embedding-input/1", bytes.fromhex(pipeline), data)
    digest = sha256(b"AIRK-DIGEST-1\0")
    for value in fields:
        digest.update(len(value).to_bytes(8, "big"))
        digest.update(value)
    return digest.hexdigest()


def committed_input(args):
    writer = json.loads(search.pinned(args.writer, args.writer_sha256))
    common.require(writer["state"] == "closed" and writer["final_stage_idle"] is True
                   and writer["service_exit_code"] == 0, "embedding row requires an authenticated closed fixture")
    index = writer["indexes"][args.repository]
    database = fidelity.publication_database(args.repository, index)
    fidelity.check_sidecars(database)
    db_hash = common.sha256_file(database)
    relative = PurePosixPath(args.path)
    source = Path(index["root"]).resolve(strict=True)
    common.require(not relative.is_absolute() and ".." not in relative.parts, "invalid vector source path")
    path = (source / relative).resolve(strict=True)
    common.require(path.is_relative_to(source), "vector source escapes pinned export")
    content = path.read_bytes()
    with sqlite3.connect(database.resolve().as_uri() + "?mode=ro&immutable=1", uri=True) as db:
        db.execute("PRAGMA trusted_schema=OFF")
        db.execute("PRAGMA query_only=ON")
        rows = db.execute(
            "SELECT f.source_hash, u.members, v.embedding_key, v.row FROM files f "
            "JOIN units u ON u.file_id=f.id JOIN vector_rows v ON v.embedding_key=u.embedding_key "
            "WHERE f.path=? AND u.ordinal=?", (relative.as_posix(), args.ordinal)).fetchall()
        common.require(len(rows) == 1, "expected one exact committed source member")
        source_hash, raw_members, key, row = rows[0]
        meta = dict(db.execute("SELECT key,value FROM meta"))
    members = json.loads(raw_members)
    common.require(len(members) == 1 and members[0]["chunk_index"] == 0
                   and sha256(content).digest() == source_hash, "source hash or member cardinality differs")
    span = members[0]["span"]
    first, last = span["start"], span["end"]
    common.require(0 <= first < last <= len(content) and last - first <= 16384, "source member span is invalid")
    data = content[first:last]
    common.require(bool(data.decode("utf-8").strip()), "source member text is blank")
    pipeline = meta["vector_pipeline"].hex()
    common.require(pipeline == index["pipeline_digest"] and meta["vector_dimensions"] == DIMENSIONS
                   and meta["sequence"] == int(index["publication_token"].split(":")[2])
                   and 0 <= row < meta["vector_rows"] and row == args.row
                   and key.hex() == args.key_sha256 == embedding_key(pipeline, data),
                   "scoped exact source span does not reproduce the committed embedding key")
    matrix = database.parent / f"vectors-{meta['vector_generation']}.f32"
    common.require(not matrix.is_symlink() and common.sha256_file(matrix) == args.matrix_sha256,
                   "committed matrix SHA-256 differs")
    with matrix.open("rb") as handle:
        header = handle.read(HEADER)
        common.require(len(header) == HEADER and header[:8] == b"AIRKVEC1"
                       and struct.unpack_from("<I", header, 8)[0] == DIMENSIONS
                       and header[16:48].hex() == pipeline, "committed matrix header differs")
        handle.seek(HEADER + row * ROW.size)
        stored = handle.read(ROW.size)
    common.require(len(stored) == ROW.size and sha256(stored).hexdigest() == args.row_sha256,
                   "selected committed f32 row differs")
    fidelity.check_sidecars(database)
    common.require(common.sha256_file(database) == db_hash and common.sha256_file(matrix) == args.matrix_sha256,
                   "closed matrix or publication changed")
    identity = {"publication": index["publication_token"], "source_commit": index["source_commit"],
                "repository": args.repository, "source_file_sha256": source_hash.hex(),
                "input_sha256": common.sha256_bytes(data), "pipeline": pipeline, "row": row,
                "row_sha256": args.row_sha256, "embedding_key": key.hex(),
                "member_bytes": last - first, "database_sha256": db_hash}
    return data, stored, identity


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    modes = parser.add_subparsers(dest="mode", required=True)
    prepare = modes.add_parser("prepare")
    compare = modes.add_parser("compare")
    for cmd in (prepare, compare):
        cmd.add_argument("--writer", type=Path, required=True)
        cmd.add_argument("--output", type=Path, required=True)
        cmd.add_argument("--repository", choices=("bookbuilder-v2", "munarium"), required=True)
        cmd.add_argument("--path", required=True)
        cmd.add_argument("--ordinal", type=int, required=True)
        cmd.add_argument("--row", type=int, required=True)
        for key in ("writer", "matrix", "row", "key"):
            cmd.add_argument(f"--{key}-sha256", required=True)
    compare.add_argument("--probe", type=Path, required=True)
    compare.add_argument("--probe-sha256", required=True)
    args = parser.parse_args()
    data, stored, identity = committed_input(args)
    if args.mode == "prepare":
        with args.output.open("xb") as handle:
            handle.write(data)
        return
    cpu, cuda, _, _ = read_probe(args, data)
    actual = ROW.unpack(stored)
    common.require(all(math.isfinite(value) for value in actual), "nonfinite committed coordinates")
    routes = {}
    for name, vector in (("cpu", cpu), ("cuda", cuda)):
        maximum = max(abs(a - b) for a, b in zip(actual, vector, strict=True))
        dot = math.fsum(a * b for a, b in zip(actual, vector, strict=True))
        routes[name] = {"bit_equal": ROW.pack(*vector) == stored, "max_abs": maximum, "dot": dot,
                        "within_tolerance": maximum <= 0.005 and dot >= 0.9995}
    search.write_new(args.output, {"schema": "airk.private.committed-minilm-row-audit/1",
        "identity": identity, "writer_sha256": args.writer_sha256, "matrix_sha256": args.matrix_sha256,
        "probe_sha256": args.probe_sha256, "routes": routes,
        "scope": "Exact single-member input and stored coordinates. Historical per-row writer route unavailable."})


if __name__ == "__main__":
    try:
        main()
    except (common.HarnessError, OSError, ValueError, TypeError, KeyError, sqlite3.Error) as error:
        print(str(error), file=sys.stderr)
        sys.exit(1)
