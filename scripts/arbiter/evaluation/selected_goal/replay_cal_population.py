# SPDX-License-Identifier: Apache-2.0
"""Reauthenticate the partially exposed CAL100 population before fresh capture."""

import argparse
import hashlib
import json
from pathlib import Path

from closure import checked_bytes, checked_json, require
from prepare_packets import partitions


BASE = Path("artifacts/advanced-split")
FIXTURE = BASE / "models/context-selected-native-data-anchor-r1/CAL/source-fixture"
PHASES = BASE / "stage-service-preparation-r1/cal-execution-r102/CAL-chunks-plan-r154"
INPUTS = {
    "controller": (PHASES / "controller-packet.json", 6521,
                   "3f5c8eca617d6b44981f1de5d9c396dd43dc07dca178682788f879536449fdcb"),
    "manifest": (FIXTURE / "manifest.private.json", 1265066,
                 "52860e855c9580c602abb5f30e08c149be92449b534d0f2ef2ab79a97b508caf"),
    "scopes": (FIXTURE / "question-scopes.private.json", 37150,
               "75ed0d8cb5317dd7462986a74b5a7ec1e0774d1d41c0670c403aaa18141c5c9b"),
    "corpus": (BASE / "models/context-generic-read-materialized-anchor-r1/materialized/CAL/corpus.private.json",
               3141030, "c51a9aa5985f2c505a2839d7b426d1614390b35639b89d9aae93681878d6ffe4"),
    "rubric": (BASE / "models/context-selected-native-data-anchor-r3/CAL/rubric/rubric.private.json",
               228524, "83dd02f57e54f3b11d5b5c93b2279a0affc6bea94c66887463bf61fc1760fd9f"),
}


def check_partition(scopes, manifest):
    require(scopes["experiment_partition"] == manifest["experiment_partition"] == "CAL"
            and manifest["goal_ids"] == [], "separate CAL partition required")
    ids = [row["id"] for row in scopes["questions"]]
    require(ids == manifest["calibration_ids"] and all(
        row["split"] == "calibration" for row in scopes["questions"]), "CAL membership or order differs")
    return partitions(ids)


def check_sources(root, files, paragraphs):
    rows = {row["file"]: row for row in paragraphs}
    require(len(rows) == len(files) == len(paragraphs) == len({row["file"] for row in files}),
            "expected one distinct source paragraph per file")
    checked = 0
    for row in files:
        relative = Path(row["file"])
        require(not relative.drive and not relative.root and ".." not in relative.parts
                and relative.parts[:-1] == ("sources", "hotpot"), "source outside CAL root")
        body = checked_bytes({"path": str(root / relative), "bytes": row["bytes"], "sha256": row["sha256"]})
        p = rows[row["file"]]
        start, end = p["byte_start"], p["byte_end"]
        require(type(start) is int and type(end) is int and 0 <= start < end <= len(body)
                and hashlib.sha256(body[start:end]).hexdigest() == p["paragraph_sha256"],
                "source paragraph range or digest differs")
        checked += len(body)
    return checked


def prepare(repository):
    refs = {name: {"path": str(repository / path), "bytes": size, "sha256": digest}
            for name, (path, size, digest) in INPUTS.items()}
    for name in ("corpus", "rubric"):
        checked_bytes(refs[name])
    scopes, manifest, controller = (checked_json(refs[name]) for name in ("scopes", "manifest", "controller"))
    require(scopes["corpus_sha256"] == manifest["corpus_sha256"] == refs["corpus"]["sha256"],
            "source corpus binding differs")
    phases = check_partition(scopes, manifest)
    sources_root = str(repository / FIXTURE / "sources/hotpot")
    require(scopes["source_roots"] == [sources_root] and all(
        row["source_root"] == sources_root for row in scopes["questions"]), "CAL source root differs")
    for phase, pin in zip(phases, controller["phase_packets"], strict=True):
        index = phase["chunk_index"]
        require(pin["chunk_index"] == index and Path(pin["path"]) == repository / PHASES / f"phase-{index:02}.json",
                "historical CAL phase path differs")
        saved = checked_json({key: pin[key] for key in ("path", "bytes", "sha256")})
        require(all(saved[key] == value for key, value in phase.items()), "historical CAL membership differs")
    require(len(manifest["files"]) == 753, "CAL source population differs")
    size = check_sources(repository / FIXTURE, manifest["files"], manifest["paragraphs"])
    census = {"schema": "airk.selected-goal-census/1", "question_ids": manifest["calibration_ids"]}
    return {"schema": "airk.private.cal100-source-replay/1", "inputs": refs, "census": census,
            "phase_sizes": [len(phase["questions"]) for phase in phases], "source_files": len(manifest["files"]),
            "source_bytes": size, "source_paragraphs": len(manifest["paragraphs"]),
            "native_calls": 0, "fresh_capture_complete": False, "calibration_fitted": False}


if __name__ == "__main__":
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("--census-only", action="store_true", help="emit prepare_packets.py's exact census input")
    args = parser.parse_args()
    result = prepare(Path(__file__).resolve().parents[4])
    print(json.dumps(result["census"] if args.census_only else result, sort_keys=True, allow_nan=False))
