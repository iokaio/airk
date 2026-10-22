# SPDX-License-Identifier: Apache-2.0
"""Alternate two binaries against isolated synthetic source-feedback workloads.

This measures CLI wall time and native stage counters for lexical feedback or
scheduled local-model drains. It does not qualify neural relevance, device I/O,
or cold OS caches.
"""
from __future__ import annotations

import argparse
import hashlib
import json
import os
from pathlib import Path
import statistics
import subprocess
import tempfile
import time


def percentile(values: list[float], quantile: float) -> float:
    ordered = sorted(values)
    return ordered[min(len(ordered) - 1, int((len(ordered) - 1) * quantile + 0.999))]


def invoke(binary: Path, root: Path, state: Path, *args: str) -> tuple[dict, float]:
    env = {**os.environ, "AIRK_USAGE_MODE": "disabled"}
    started = time.perf_counter()
    result = subprocess.run(
        [str(binary), "--root", str(root), "--data-dir", str(state), *args],
        capture_output=True, text=True, encoding="utf-8", env=env, timeout=1200,
    )
    elapsed = time.perf_counter() - started
    if result.returncode:
        raise RuntimeError(f"{binary.name} {args}: {result.stderr[-4096:]}")
    return json.loads(result.stdout), elapsed


def fixture(root: Path, files: int, size: int) -> None:
    root.mkdir()
    for index in range(files):
        directory = root / f"group_{index // 100:03}"
        directory.mkdir(exist_ok=True)
        prefix = f"Original fictional feedback fixture {index}.\n\n"
        paragraph = "Saved source hashing and cited repository evidence.\n\n"
        text = (prefix + paragraph * (size // len(paragraph) + 1))[:size]
        (directory / f"file_{index:05}.txt").write_bytes(text.encode("utf-8"))


def storage_snapshot(state: Path) -> dict:
    """Logical file bytes after helpers exit; excludes filesystem allocation slack."""
    groups: dict[str, int] = {}
    count = 0
    for path in state.rglob("*"):
        if not path.is_file():
            continue
        parts = path.relative_to(state).parts
        if "objects" in parts:
            position = parts.index("objects")
            category = "/".join(parts[position:position + 2])
        elif parts[0] == "collections-1.2":
            category = "collection/" + ("/".join(parts[2:-1]) if len(parts) > 3 else path.name)
        else:
            category = "root/" + parts[0]
        groups[category] = groups.get(category, 0) + path.stat().st_size
        count += 1
    return {"logical_bytes": sum(groups.values()), "files": count,
            "categories": dict(sorted(groups.items()))}


def workload(binary: Path, directory: Path, files: int, size: int, paths: bool) -> dict:
    root, state = directory / "source", directory / "state"
    directory.mkdir()
    fixture(root, files, size)
    invoke(binary, root, state, "collections", "vectors", "set", "--enabled", "false")
    cold, cold_wall = invoke(binary, root, state, "index", "--scheduled")
    noop, noop_wall = invoke(binary, root, state, "index")
    if noop["indexed"] or noop["reused"] != files:
        raise AssertionError("unchanged rescan lost extraction reuse")
    relative = "group_000/file_00000.txt"
    current = root / relative
    sentinel = "freshfeedbacksentinel"
    current.write_bytes(f"{sentinel}\n".encode("utf-8") + current.read_bytes())
    report, edit_wall = invoke(binary, root, state, "index", *(["--paths", relative] if paths else []))
    if report["indexed"] != 1 or report["removed"]:
        raise AssertionError("one-file edit changed unrelated inventory")
    hit, query_wall = invoke(binary, root, state, "search", sentinel, "--mode", "lexical")
    expected = hashlib.sha256(current.read_bytes()).hexdigest()
    if not any(item["citation"]["path"] == relative and item["citation"]["content_hash"] == expected for item in hit["hits"]):
        raise AssertionError("fresh evidence failed its source hash/citation check")
    # Include the verification query in the agent-visible edit-to-evidence time.
    return {
        "cold_s": cold_wall, "noop_s": noop_wall, "edit_s": edit_wall,
        "edit_to_evidence_s": edit_wall + query_wall, "query_s": query_wall,
        "first_ready_us": cold.get("first_ready_us"),
        "noop_scan_extract_us": noop.get("scan_extract_us"),
        "edit_scan_extract_us": report.get("scan_extract_us"),
        "edit_publish_us": report.get("publish_us"),
        "path_scoped": report.get("path_scoped", False),
        "citation_verified": True,
        "storage": storage_snapshot(state),
    }


def semantic_workload(binary: Path, directory: Path, files: int, size: int, model: Path) -> dict:
    root, state = directory / "source", directory / "state"
    directory.mkdir()
    fixture(root, files, size)
    invoke(binary, root, state, "collections", "vectors", "install", str(model))
    base, base_wall = invoke(binary, root, state, "index", "--scheduled")
    started = time.perf_counter()
    calls, status_calls, model_load_us, inference_us = 0, 0, 0, 0
    for _ in range((files + 31) // 32 + 2):
        outcome, _ = invoke(binary, root, state, "semantic", "run", "--scheduled")
        calls += 1
        if outcome["state"] != "enriched":
            raise AssertionError(f"semantic work failed: {outcome}")
        report = outcome.get("report") or {}
        model_load_us += report.get("embedding_model_load_us", 0)
        inference_us += report.get("embedding_inference_us", 0)
        readiness = report.get("semantic_readiness")
        if readiness is None:
            readiness, _ = invoke(binary, root, state, "semantic", "status")
            status_calls += 1
        if readiness["state"] == "ready":
            break
    else:
        raise AssertionError("semantic drain did not finish within its batch bound")
    elapsed = time.perf_counter() - started
    if readiness["completed_artifacts"] != files or readiness["selected_artifacts"] != files:
        raise AssertionError("semantic coverage differs from the fixture")
    result, query_wall = invoke(binary, root, state, "search", "hashing", "--mode", "hybrid")
    if not result["hits"]:
        raise AssertionError("hybrid fixture has no cited evidence")
    for hit in result["hits"]:
        citation = hit["citation"]
        if citation["content_hash"] != hashlib.sha256((root / citation["path"]).read_bytes()).hexdigest():
            raise AssertionError("hybrid citation differs from its source hash")
    return {"base_s": base_wall, "semantic_s": elapsed, "query_s": query_wall,
            "semantic_processes": calls, "status_processes": status_calls,
            "first_ready_us": base.get("first_ready_us"),
            "reported_model_load_us": model_load_us if status_calls == 0 else None,
            "reported_inference_us": inference_us if status_calls == 0 else None,
            "completed_artifacts": readiness["completed_artifacts"], "citation_verified": True,
            "storage": storage_snapshot(state)}


def main() -> None:
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("--before", type=Path, required=True)
    parser.add_argument("--after", type=Path, required=True)
    parser.add_argument("--files", type=int, default=54)
    parser.add_argument("--bytes-per-file", type=int, default=16384)
    parser.add_argument("--trials", type=int, default=5)
    parser.add_argument("--output", type=Path, required=True)
    parser.add_argument("--semantic-model", type=Path, help="Verified local model directory; measure scheduled semantic completion instead of edit feedback")
    options = parser.parse_args()
    if not 1 <= options.files <= 100000 or not 128 <= options.bytes_per_file <= 1048576 or options.files * options.bytes_per_file > 512 * 1024 * 1024 or not 1 <= options.trials <= 20:
        parser.error("workload exceeds supported fixture bounds")
    before, after = options.before.resolve(strict=True), options.after.resolve(strict=True)
    output = options.output.resolve()
    output.parent.mkdir(parents=True, exist_ok=True)
    samples: dict[str, list[dict]] = {"before": [], "after": []}
    identity = {name: {"sha256": hashlib.sha256(binary.read_bytes()).hexdigest(), "path": str(binary)} for name, binary in [("before", before), ("after", after)]}
    with tempfile.TemporaryDirectory(prefix="airk-feedback-", dir=output.parent) as temporary:
        directory = Path(temporary).resolve()
        assert directory.is_relative_to(output.parent)
        for trial in range(options.trials):
            order = [("before", before), ("after", after)]
            if trial % 2:
                order.reverse()
            for name, binary in order:
                destination = directory / f"{trial}-{name}"
                sample = semantic_workload(binary, destination, options.files, options.bytes_per_file, options.semantic_model.resolve(strict=True)) if options.semantic_model else workload(binary, destination, options.files, options.bytes_per_file, name == "after")
                samples[name].append(sample)
                print(json.dumps({"trial": trial + 1, "variant": name, **sample}), flush=True)
    summary = {}
    metrics = ["base_s", "semantic_s", "query_s"] if options.semantic_model else ["cold_s", "noop_s", "edit_s", "edit_to_evidence_s", "query_s"]
    for metric in metrics:
        summary[metric] = {name: {"p50": statistics.median([sample[metric] for sample in values]), "p95": percentile([sample[metric] for sample in values], .95)} for name, values in samples.items()}
        summary[metric]["median_reduction_percent"] = 100 * (1 - summary[metric]["after"]["p50"] / summary[metric]["before"]["p50"])
    model_digest = hashlib.sha256((options.semantic_model.resolve() / "model.json").read_bytes()).hexdigest() if options.semantic_model else None
    output.write_text(json.dumps({"schema": 1, "files": options.files, "bytes_per_file": options.bytes_per_file, "trials": options.trials, "vectors": "local-model" if options.semantic_model else "disabled", "model_manifest_sha256": model_digest, "identity": identity, "samples": samples, "summary": summary}, indent=2) + "\n", encoding="utf-8")
    print(json.dumps({"output": str(output), "summary": summary}), flush=True)


if __name__ == "__main__":
    main()
