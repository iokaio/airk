# SPDX-License-Identifier: Apache-2.0
"""Measure indexing operations through the kernel's component harness.

Each operation runs in its own process of the `vector_performance` test
binary, built with the `airk-core/stage-timing` feature. A receipt records
wall time, the kernel's index report, stage totals and the size of the index.
Receipts hold counts and timings only: no source text and no search hits.
"""
import argparse
import hashlib
import json
import os
from pathlib import Path
import platform
import subprocess
import sys
import tempfile
import time
from datetime import datetime, timezone

SCHEMA = "airk.indexing-plan.measurement/1"
# Bounded diagnostics in an index report name source paths; keep their counts.
PATH_LISTS = ("skipped", "selection_diagnostics")


def sha256_file(path):
    digest = hashlib.sha256()
    with open(path, "rb") as stream:
        for block in iter(lambda: stream.read(1 << 20), b""):
            digest.update(block)
    return digest.hexdigest()


def storage_summary(data):
    """Count files and bytes of an index, grouped by the first three folder levels."""
    data = Path(data)
    groups = {}
    files = size = 0
    for directory, _, names in os.walk(data):
        key = "/".join(Path(directory).relative_to(data).parts[:3]) or "."
        for name in names:
            try:
                length = os.path.getsize(os.path.join(directory, name))
            except OSError:
                continue
            entry = groups.setdefault(key, {"files": 0, "bytes": 0})
            entry["files"] += 1
            entry["bytes"] += length
            files += 1
            size += length
    return {"files": files, "bytes": size, "groups": dict(sorted(groups.items()))}


def stage_rows(totals):
    """Stage totals in seconds, largest first. A parent stage includes its children."""
    rows = [{"stage": total["stage"], "calls": total["calls"],
             "seconds": total["nanoseconds"] / 1e9} for total in totals]
    return sorted(rows, key=lambda row: (-row["seconds"], row["stage"]))


def without_paths(report):
    """Replace the report's path-bearing diagnostic lists by their lengths."""
    if not isinstance(report, dict):
        return report
    cleaned = dict(report)
    for name in PATH_LISTS:
        if isinstance(cleaned.get(name), list):
            cleaned[name] = len(cleaned[name])
    return cleaned


def child_environment(base, root, data, arguments, result, stages, temp=None, workers=None):
    """The harness reads its request and both result paths from the environment."""
    environment = dict(base)
    environment["AIRK_PERF_REQUEST"] = json.dumps([str(root), str(data), list(arguments)])
    environment["AIRK_PERF_RESULT"] = str(result)
    environment["AIRK_STAGE_TIMING_RESULT"] = str(stages)
    environment["AIRK_USAGE_MODE"] = "disabled"
    if workers is not None:
        environment["AIRK_INDEX_WORKERS"] = str(workers)
    if temp is not None:
        environment["TEMP"] = environment["TMP"] = str(temp)
    return environment


def run_component(harness, root, data, arguments, temp=None, workers=None):
    """Run one adapter operation; return wall seconds, its JSON value and stage rows."""
    with tempfile.TemporaryDirectory(dir=temp) as scratch:
        result = Path(scratch) / "result.json"
        stages = Path(scratch) / "stages.json"
        environment = child_environment(os.environ, root, data, arguments, result, stages,
                                        temp, workers)
        started = time.perf_counter()
        completed = subprocess.run(
            [str(harness), "--exact", "component_adapter", "--nocapture"],
            env=environment, capture_output=True, text=True, check=False)
        wall = time.perf_counter() - started
        if completed.returncode != 0:
            raise RuntimeError(
                f"harness exited {completed.returncode}: {completed.stderr[-2000:]}")
        value = json.loads(result.read_text(encoding="utf-8"))
        totals = json.loads(stages.read_text(encoding="utf-8")) if stages.exists() else []
        return wall, value, stage_rows(totals)


def search_arguments(query, mode, prefix=None):
    arguments = ["search", query, "--mode", mode]
    if prefix:
        arguments += ["--path-prefix", prefix]
    return arguments


def search_counts(value):
    """Counts only: a search result's text and paths never enter a receipt."""
    hits = value.get("hits") if isinstance(value, dict) else None
    return {"hits": len(hits) if isinstance(hits, list) else None,
            "truncated": value.get("truncated") if isinstance(value, dict) else None}


def first_hit_path(value):
    try:
        return value["hits"][0]["citation"]["path"]
    except (KeyError, IndexError, TypeError):
        return None


def measure(arguments):
    harness, root, data = arguments.harness, arguments.source, arguments.data
    receipt = {
        "schema": SCHEMA, "label": arguments.label, "operation": arguments.operation,
        "started_utc": datetime.now(timezone.utc).isoformat(),
        "harness": {"path": str(harness), "sha256": sha256_file(harness)},
        "source": str(root), "data": str(data), "workers": arguments.workers,
        "host": {"logical_cpus": os.cpu_count(), "platform": platform.platform()},
    }
    common = {"temp": arguments.temp, "workers": arguments.workers}
    operation = arguments.operation
    if operation in ("index", "reindex", "status", "install-model"):
        adapter = {"index": ["index"], "reindex": ["index", "--full"], "status": ["status"],
                   "install-model": ["collections", "vectors", "install",
                                     str(arguments.model_dir)]}[operation]
        wall, value, stages = run_component(harness, root, data, adapter, **common)
        receipt.update(wall_seconds=wall, report=without_paths(value), stages=stages)
    elif operation == "search":
        samples = []
        for _ in range(arguments.repeat):
            wall, value, _ = run_component(
                harness, root, data,
                search_arguments(arguments.query, arguments.mode, arguments.path_prefix),
                **common)
            samples.append({"wall_seconds": wall, **search_counts(value)})
        receipt.update(mode=arguments.mode, samples=samples,
                       note="each sample is a new process: open, model load and query")
    elif operation == "edit":
        target = Path(root) / arguments.edit_file
        original = target.read_bytes()
        try:
            target.write_bytes(original + b"\n" + arguments.marker.encode("utf-8") + b"\n")
            index_wall, value, stages = run_component(harness, root, data, ["index"], **common)
            search_wall, found, _ = run_component(
                harness, root, data, search_arguments(arguments.marker, "lexical"), **common)
            located = first_hit_path(found)
            receipt.update(
                wall_seconds=index_wall, search_wall_seconds=search_wall,
                report=without_paths(value), stages=stages,
                marker_found_in_edited_file=located == arguments.edit_file.replace("\\", "/"))
        finally:
            target.write_bytes(original)
    else:
        raise ValueError(f"unsupported operation: {operation}")
    if operation in ("index", "reindex", "edit"):
        receipt["storage"] = storage_summary(data)
    Path(arguments.output).write_text(json.dumps(receipt, indent=2, sort_keys=True) + "\n",
                                      encoding="utf-8")
    return receipt


def summary_row(receipt):
    """One flat row per receipt for the comparison table."""
    report = receipt.get("report") or {}
    files = (report.get("indexed") or 0) + (report.get("reused") or 0)
    wall = receipt.get("wall_seconds")
    return {
        "label": receipt.get("label"), "operation": receipt.get("operation"),
        "wall_seconds": wall, "files": files,
        "files_per_second": files / wall if wall and files else None,
        "indexed": report.get("indexed"), "reused": report.get("reused"),
        "vectors": report.get("indexed_vectors"),
        "embedding_inferences": report.get("embedding_inferences"),
        "storage_files": (receipt.get("storage") or {}).get("files"),
        "storage_bytes": (receipt.get("storage") or {}).get("bytes"),
    }


def summarize(paths, top):
    lines = ["| Label | Operation | Wall s | Files | Files/s | Index files | Index bytes |",
             "|---|---|---:|---:|---:|---:|---:|"]
    details = []
    for path in paths:
        receipt = json.loads(Path(path).read_text(encoding="utf-8"))
        row = summary_row(receipt)
        number = lambda value, digits=1: "" if value is None else f"{value:,.{digits}f}"
        lines.append("| {label} | {operation} | {wall} | {files} | {rate} | {sf} | {sb} |".format(
            label=row["label"], operation=row["operation"], wall=number(row["wall_seconds"]),
            files=number(row["files"], 0), rate=number(row["files_per_second"]),
            sf=number(row["storage_files"], 0), sb=number(row["storage_bytes"], 0)))
        stages = receipt.get("stages") or []
        if stages:
            details.append(f"\n{row['label']} ({row['operation']}), largest stages:")
            for stage in stages[:top]:
                per_file = stage["calls"] / row["files"] if row["files"] else 0
                details.append(f"  {stage['seconds']:>10.2f} s  {stage['calls']:>10,} calls"
                               f"  {per_file:>7.2f} per file  {stage['stage']}")
    return "\n".join(lines + details)


def main(argv=None):
    parser = argparse.ArgumentParser(description=__doc__)
    commands = parser.add_subparsers(dest="command", required=True)
    run = commands.add_parser("run", help="measure one operation and write a receipt")
    run.add_argument("--harness", type=Path, required=True,
                     help="vector_performance test binary built with airk-core/stage-timing")
    run.add_argument("--source", type=Path, required=True, help="private copy of a source tree")
    run.add_argument("--data", type=Path, required=True, help="index state folder for this label")
    run.add_argument("--label", required=True)
    run.add_argument("--operation", required=True,
                     choices=("index", "reindex", "status", "install-model", "search", "edit"))
    run.add_argument("--output", type=Path, required=True)
    run.add_argument("--temp", type=Path, help="TEMP for the harness and its scratch files")
    run.add_argument("--workers", type=int, help="AIRK_INDEX_WORKERS for the harness")
    run.add_argument("--model-dir", type=Path, help="one model profile folder (install-model)")
    run.add_argument("--query", help="search text")
    run.add_argument("--mode", choices=("lexical", "vector"), default="lexical")
    run.add_argument("--path-prefix")
    run.add_argument("--repeat", type=int, default=3, help="search samples")
    run.add_argument("--edit-file", help="path below --source to append the marker to (edit)")
    run.add_argument("--marker", help="one unique token appended by the edit operation")
    table = commands.add_parser("summary", help="print a table from receipts")
    table.add_argument("receipts", nargs="+", type=Path)
    table.add_argument("--top", type=int, default=25, help="stages listed per receipt")
    arguments = parser.parse_args(argv)
    if arguments.command == "summary":
        print(summarize(arguments.receipts, arguments.top))
        return 0
    needs = {"install-model": ("model_dir",), "search": ("query",), "edit": ("edit_file", "marker")}
    missing = [name for name in needs.get(arguments.operation, ()) if not getattr(arguments, name)]
    if missing:
        parser.error(f"{arguments.operation} requires --" + ", --".join(
            name.replace("_", "-") for name in missing))
    receipt = measure(arguments)
    print(json.dumps({"label": receipt["label"], "operation": receipt["operation"],
                      "wall_seconds": receipt.get("wall_seconds"),
                      "output": str(arguments.output)}))
    return 0


if __name__ == "__main__":
    sys.exit(main())
