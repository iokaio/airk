# SPDX-License-Identifier: Apache-2.0
"""Measure original v2 edit/move fixtures and persistent MCP metadata reads."""
import argparse
import importlib.util
import json
import math
import platform
import queue
import subprocess
import tempfile
import threading
import time
from pathlib import Path


SPEC = importlib.util.spec_from_file_location("benchmark_scale", Path(__file__).with_name("benchmark-scale.py"))
SCALE = importlib.util.module_from_spec(SPEC)
SPEC.loader.exec_module(SCALE)


def percentiles(values):
    """Nearest-rank percentiles; never interpolate an unobserved measurement."""
    ordered = sorted(values)
    if not ordered:
        raise ValueError("percentiles require observations")
    return {"samples": len(ordered), **{
        f"p{rank}_ms": ordered[math.ceil(len(ordered) * rank / 100) - 1]
        for rank in (50, 95)}}


def fixture(root, files):
    root.mkdir()
    for number in range(files):
        (root / f"module_{number:04}.ts").write_bytes(
            (f"export function task{number}(value: string): string {{\n"
             "  return value;\n}\n").encode())
    (root / "requirements.md").write_bytes(b"# Delivery\n\nREQ-001: The queue must retain 20 tasks.\n")
    (root / "package.json").write_bytes(b'{"name":"original-benchmark","private":true}\n')


def mixed_fixture():
    """Original small multi-format corpus; bytes are the retained citation oracle."""
    notebook = {"nbformat": 4, "nbformat_minor": 5,
                "metadata": {"language_info": {"name": "python"}}, "cells": [
                    {"cell_type": "markdown", "id": "requirements", "metadata": {},
                     "source": ["# Notebook requirements\n", "REQ-902: The notebook must retain original measurements.\n"]},
                    {"cell_type": "code", "id": "metrics", "metadata": {}, "execution_count": None,
                     "outputs": [], "source": ["def mapped_metric(value):\n", "    return value + 41\n"]}]}
    boilerplate = b"Original repeated benchmark boilerplate. Shared text is not a durable file identity.\n"
    return {
        "mixed/code.rs": b"pub fn mixed_metric(value: u64) -> u64 {\n    let original_measurement = value + 41;\n    original_measurement\n}\n",
        "mixed/requirements.md": "# Original café requirements\r\n\r\nREQ-900: The archive must retain original source locations and bounded measurements.\r\n".encode(),
        "mixed/api.yaml": b"openapi: 3.0.3\ninfo:\n  title: Original benchmark\n  version: '1'\npaths:\n  /metrics:\n    get:\n      operationId: originalMetrics\n      responses:\n        '200':\n          description: Original measurement\n",
        "mixed/component.vue": b"<template><section>Original measurements</section></template>\r\n<script setup lang=\"ts\">\r\nexport function mappedMetric(value: number) { return value + 41; }\r\n</script>\r\n<style>.metric { color: blue; }</style>\r\n",
        "mixed/notes.ipynb": (json.dumps(notebook, ensure_ascii=False, indent=2) + "\n").encode(),
        "mixed/boilerplate-a.txt": boilerplate,
        "mixed/boilerplate-b.txt": boilerplate,
    }


def verify_read(response, path, original, generation):
    """Reject stale/misbound/truncated evidence instead of recording a success."""
    import hashlib
    hits = response.get("hits", [])
    if (response.get("generation") != generation or response.get("truncated")
            or response.get("read", {}).get("status") != "content" or len(hits) != 1):
        raise AssertionError("retained read did not return one complete pinned representation")
    hit = hits[0]
    citation = hit.get("citation", {})
    if (hit.get("truncated") or hit.get("text", "").encode() != original
            or citation.get("path") != path
            or citation.get("content_hash") != hashlib.sha256(original).hexdigest()
            or citation.get("start_line") != 1
            or citation.get("end_line") != len(original.splitlines())):
        raise AssertionError("retained citation/text differs from original bytes")


def extended_workloads(root, state, run, generation, files, moved_count):
    original = mixed_fixture()
    for path, content in original.items():
        destination = root / path
        destination.parent.mkdir(parents=True, exist_ok=True)
        destination.write_bytes(content)
    mixed_source_digest = SCALE.source_digest(root)
    mixed = run("mixed-add", "index")
    total = files + 2 + len(original)
    if mixed["indexed"] != len(original) or mixed["reused"] != files + 2 or mixed["generation"] <= generation:
        raise AssertionError("mixed addition did not isolate new extraction")
    generation = mixed["generation"]
    citations = []

    def retained(path, content, expected_generation):
        result = run(f"retained-read-{len(citations):03}-" + path.replace("/", "-"), "read", path,
                     "--start-line", "1", "--end-line", str(len(content.splitlines())), "--max-bytes", "65536")
        verify_read(result, path, content, expected_generation)
        citations.append({"path": path, "generation": expected_generation,
                          "citation": result["hits"][0]["citation"], "validator": result["read"].get("validator")})

    for path, content in original.items():
        retained(path, content, generation)
    # A read must continue using committed bytes even when the live file differs.
    probe = root / "mixed/code.rs"
    probe.write_bytes(b"unindexed replacement must never leak into retained evidence\n")
    try:
        retained("mixed/code.rs", original["mixed/code.rs"], generation)
    finally:
        probe.write_bytes(original["mixed/code.rs"])
    outlines = []
    for path, expected in (("mixed/code.rs", "mixed_metric"), ("mixed/component.vue", "mappedMetric"),
                           ("mixed/notes.ipynb", "mapped_metric"), ("mixed/requirements.md", "REQ-900")):
        result = run("mixed-outline-" + path.replace("/", "-"), "outline", path, "--limit", "100", "--max-bytes", "65536")
        if not any(expected in fact["name"] for fact in result.get("facts", [])):
            raise AssertionError("mixed outline omitted required original evidence: " + path)
        outlines.append({"path": path, "expected": expected, "facts": len(result["facts"])})
    if SCALE.source_digest(root) != mixed_source_digest:
        raise AssertionError("mixed indexing/read operations modified source files")
    child_paths = ("mixed/component.vue", "mixed/notes.ipynb")
    for path in child_paths:
        original[path] = original[path].replace(b"value + 41", b"value + 43")
        (root / path).write_bytes(original[path])
    child = run("mixed-child-edits", "index")
    if child["indexed"] != 2 or child["reused"] != total - 2 or child["generation"] <= generation:
        raise AssertionError("embedded edits did not isolate the two host extractions")
    generation = child["generation"]
    for path in child_paths:
        retained(path, original[path], generation)

    selected = min(files, 12)
    removed, changed, created = [], [], []
    for number in range(selected):
        path = ("moved/" if number < moved_count else "") + f"module_{number:04}.ts"
        if number % 2 == 0:
            (root / path).unlink()
            removed.append(path)
            replacement = f"replacement/branch_{number:04}.ts"
            (root / replacement).parent.mkdir(exist_ok=True)
            (root / replacement).write_bytes(f"export function branchMetric{number}(value: number) {{ return value + {9000 + number}; }}\n".encode())
            created.append(replacement)
        else:
            (root / path).write_bytes(f"export function revisedMetric{number}(value: number) {{ return value * {9000 + number}; }}\n".encode())
            changed.append(path)
    branch_source_digest = SCALE.source_digest(root)
    branch = run("branch-replacement", "index")
    if (branch["indexed"] != selected or branch["reused"] != total - selected
            or branch["removed"] != len(removed) or branch["generation"] <= generation):
        raise AssertionError("branch replacement extraction/removal receipt is inconsistent")
    for path in created + changed:
        retained(path, (root / path).read_bytes(), branch["generation"])
    warm = run("extended-warm", "index")
    if warm["indexed"] != 0 or warm["reused"] != total or warm["generation"] != branch["generation"]:
        raise AssertionError("extended warm scan changed or re-extracted the publication")
    if SCALE.source_digest(root) != branch_source_digest:
        raise AssertionError("branch indexing/read operations modified source files")
    return {"mixed_index": mixed, "mixed_files": len(original), "outlines": outlines,
            "child_edits": child, "branch_replacement": {"index": branch, "removed": removed,
            "changed": changed, "created": created}, "warm_index": warm, "retained_citations": citations,
            "fixture_after_mixed_add": mixed_source_digest, "fixture_after_branch": branch_source_digest,
            "retained_logical_bytes": SCALE.logical_bytes(state), "correctness_checks_passed": True}


class McpReader:
    """One bounded stdio client; the same kernel services all warm reads."""
    def __init__(self, command, timeout=30):
        self.timeout = timeout
        self.sequence = 0
        self.responses = queue.Queue(maxsize=32)
        self.errors = tempfile.TemporaryFile()
        self.process = subprocess.Popen(command, stdin=subprocess.PIPE, stdout=subprocess.PIPE,
                                        stderr=self.errors)
        self.thread = threading.Thread(target=self._read, daemon=True)
        self.thread.start()
        try:
            self.request("initialize", {"protocolVersion": "2024-11-05", "capabilities": {},
                                        "clientInfo": {"name": "airk-original-benchmark", "version": "1"}})
            self._send({"jsonrpc": "2.0", "method": "notifications/initialized"})
        except BaseException as error:
            try:
                self.close()
            except Exception as cleanup_error:
                error.add_note(f"MCP cleanup failed: {cleanup_error}")
            raise

    def _read(self):
        try:
            while True:
                line = self.process.stdout.readline(1_048_577)
                if not line:
                    raise RuntimeError("MCP worker closed stdout")
                if len(line) > 1_048_576:
                    raise RuntimeError("MCP response exceeds 1 MiB")
                self.responses.put(json.loads(line), timeout=self.timeout)
        except Exception as error:
            try:
                self.responses.put(error, timeout=self.timeout)
            except queue.Full:
                pass

    def _send(self, payload):
        try:
            self.process.stdin.write(json.dumps(payload).encode() + b"\n")
            self.process.stdin.flush()
        except OSError as error:
            # A worker can exit before initialization reaches its stdin, even
            # before the reader thread observes EOF. Keep that race in the same
            # explicit worker-failure contract as a closed response stream.
            raise RuntimeError("MCP worker closed stdout or input pipe") from error

    def request(self, method, params):
        self.sequence += 1
        self._send({"jsonrpc": "2.0", "id": self.sequence, "method": method, "params": params})
        deadline = time.monotonic() + self.timeout
        while True:
            try:
                response = self.responses.get(timeout=max(0, deadline - time.monotonic()))
            except queue.Empty as error:
                raise TimeoutError("MCP response deadline exceeded") from error
            if isinstance(response, Exception):
                raise response
            if response.get("id") != self.sequence:
                if "id" in response:
                    raise RuntimeError("unexpected MCP response identifier")
                continue
            if "error" in response:
                raise RuntimeError(f"MCP error: {response['error']}")
            return response["result"]

    def info(self, path):
        result = self.request("tools/call", {"name": "airk_file_info", "arguments": {"path": path}})
        if result.get("isError"):
            raise RuntimeError(f"MCP tool failed: {result}")
        return json.loads(next(item["text"] for item in result["content"] if item["type"] == "text"))

    def close(self):
        try:
            if self.process.poll() is None:
                self.process.kill()
            self.process.wait(timeout=10)
        finally:
            self.thread.join(timeout=10)
            for stream in (self.process.stdin, self.process.stdout, self.errors):
                try:
                    stream.close()
                except OSError:
                    # Buffered stdin.close may retry the failed write after the
                    # child exits. This must not replace the request failure or
                    # prevent stdout/stderr from being released.
                    pass


def read_samples(command, path, workers, reads):
    cold, warm, startup = [], [], []
    expected = None
    for _ in range(workers):
        start = time.perf_counter_ns()
        reader = McpReader(command)
        startup.append((time.perf_counter_ns() - start) / 1_000_000)
        try:
            for sample in range(reads + 1):
                start = time.perf_counter_ns()
                result = reader.info(path)
                (cold if sample == 0 else warm).append((time.perf_counter_ns() - start) / 1_000_000)
                result.pop("request_id", None)
                if result.get("path") != path:
                    raise AssertionError("metadata response selected a different file")
                if expected is None:
                    expected = result
                elif result != expected:
                    raise AssertionError("metadata changed across immutable baseline reads")
        finally:
            reader.close()
    return {"cold_worker": percentiles(cold), "warm_worker": percentiles(warm),
            "worker_initialization": percentiles(startup)}



def local_model_workload(root, state, run):
    """Explicit local original vectors; pipeline correctness, never model quality."""
    import hashlib
    root.mkdir()
    model = state / "models" / "local"
    model.mkdir(parents=True)
    vocabulary = b'{"login":[1.0,0.0],"invoice":[0.0,1.0]}'
    (model / "vocabulary.json").write_bytes(vocabulary)
    manifest = {"schema": 1, "name": "original bounded benchmark vectors",
                "license": "Apache-2.0", "dimensions": 2,
                "sha256": hashlib.sha256(vocabulary).hexdigest()}
    (model / "model.json").write_text(json.dumps(manifest) + "\n", encoding="utf-8")
    sources = {"login.txt": b"login original retained evidence\n",
               "invoice.txt": b"invoice original retained evidence\n"}
    for path, content in sources.items():
        (root / path).write_bytes(content)
    cold = run("model-cold", "index")
    if (cold["indexed"] != 2 or cold["reused"] != 0
            or cold["embedding_inferences"] != 2 or cold["embedding_no_vector"] != 0
            or cold["embedding_cache_hits"] != 0):
        raise AssertionError("local model cold extraction/inference counts differ")
    warm = run("model-warm", "index")
    if (warm["indexed"] != 0 or warm["reused"] != 2
            or warm["generation"] != cold["generation"] or warm["embedding_inferences"] != 0):
        raise AssertionError("local model unchanged publication was rebuilt")
    sources["login.txt"] = b"login changed original retained evidence\n"
    (root / "login.txt").write_bytes(sources["login.txt"])
    digest = SCALE.source_digest(root)
    edited = run("model-edit", "index")
    if (edited["indexed"] != 1 or edited["reused"] != 1
            or edited["generation"] <= cold["generation"]
            or edited["embedding_inferences"] != 1 or edited["embedding_cache_hits"] != 1
            or edited["embedding_no_vector"] != 0):
        raise AssertionError("local model edit failed exact unchanged-vector reuse")
    query = run("model-query", "search", "login", "--semantic", "--limit", "2", "--max-bytes", "8192")
    hits = query.get("hits", [])
    if query.get("generation") != edited["generation"] or not hits or query.get("truncated"):
        raise AssertionError("local model query lacks current bounded evidence")
    if hits[0].get("citation", {}).get("path") != "login.txt":
        raise AssertionError("original orthogonal-vector query selected unexpected evidence")
    for hit in hits:
        path = hit.get("citation", {}).get("path")
        if path not in sources:
            raise AssertionError("local model query selected foreign source")
        verify_read({"generation": query["generation"], "read": {"status": "content"},
                     "hits": [hit]}, path, sources[path], edited["generation"])
    for path, content in sources.items():
        retained = run("model-read-" + path, "read", path, "--start-line", "1", "--end-line", "1", "--max-bytes", "8192")
        verify_read(retained, path, content, edited["generation"])
    if SCALE.source_digest(root) != digest:
        raise AssertionError("local model operations modified sources")
    return {"model": manifest, "vocabulary_bytes": len(vocabulary), "cold": cold,
            "unchanged": warm, "edit": edited, "query": query,
            "source_digest": digest, "logical_state_bytes": SCALE.logical_bytes(state),
            "quality": "unqualified; original two-token vectors only, no ONNX or pretrained model"}


def benchmark(binary, directory, files, edits, workers, reads, local_model=False):
    machine = SCALE.machine_environment(directory)
    binary_hash, binary_bytes = SCALE.hash_file(binary)
    root, state = directory / "sources", directory / "state"
    fixture(root, files)
    initial_fixture = SCALE.source_digest(root)
    base = [str(binary), "--root", str(root), "--data-dir", str(state), "--collection", "v2-benchmark"]
    observations = []

    def run(label, *args):
        payload, metrics = SCALE.measured(base + list(args), directory, label)
        observations.append(metrics)
        return payload

    first = run("initial", "index")
    if first["indexed"] != files + 2:
        raise AssertionError("incomplete initial fixture")
    unchanged = run("unchanged", "index")
    if unchanged["generation"] != first["generation"] or unchanged["reused"] != files + 2:
        raise AssertionError("unchanged scan did not reuse the initial publication")
    reads_report = read_samples(base + ["mcp"], "module_0000.ts", workers, reads)
    if SCALE.source_digest(root) != initial_fixture:
        raise AssertionError("index/read operations modified original sources")
    generation = first["generation"]
    publications = []
    for edit in range(edits):
        (root / "module_0000.ts").write_bytes(f"export function task0() {{ return {edit}; }}\n".encode())
        result = run(f"edit-{edit:04}", "index")
        if result["generation"] <= generation or result["indexed"] != 1 or result["reused"] != files + 1:
            raise AssertionError("small edit did not publish exactly one changed extraction")
        generation = result["generation"]
        publications.append({"edit": edit, "generation": generation,
                             "index_receipt": result,
                             "retained_logical_bytes": SCALE.logical_bytes(state)})
    moved_count = min(files, 300)
    (root / "moved").mkdir()
    for number in range(moved_count):
        (root / f"module_{number:04}.ts").rename(root / "moved" / f"module_{number:04}.ts")
    moved = run("moves", "index")
    if moved["generation"] <= generation or moved["indexed"] != 0 or moved["reused"] != files + 2:
        raise AssertionError("compatible moves failed to publish with complete extraction reuse")
    info = run("moved-info", "info", "moved/module_0000.ts")
    if (info["generation"] != moved["generation"] or info["path"] != "moved/module_0000.ts"
            or info["content_hash"] != SCALE.hash_file(root / "moved/module_0000.ts")[0]):
        raise AssertionError("moved metadata did not bind the new path and original bytes")
    extended = extended_workloads(root, state, run, moved["generation"], files, moved_count)
    usage = run("usage", "usage", "report")
    model_report = None
    if local_model:
        model_root, model_state = directory / "model-sources", directory / "model-state"
        model_base = [str(binary), "--root", str(model_root), "--data-dir", str(model_state),
                      "--collection", "v2-model-benchmark"]
        def model_run(label, *args):
            payload, metrics = SCALE.measured(model_base + list(args), directory, label)
            observations.append(metrics)
            return payload
        model_report = local_model_workload(model_root, model_state, model_run)
    if SCALE.hash_file(binary) != (binary_hash, binary_bytes):
        raise AssertionError("benchmark executable changed during the campaign")
    return {"schema": "airk-v2-benchmark/1", "platform": platform.platform(),
            "machine": machine, "binary_sha256": binary_hash,
            "binary_bytes": binary_bytes, "initial_fixture": initial_fixture,
            "final_fixture": SCALE.source_digest(root), "mode": "lexical; default ingestion/quota policy",
            "first_index": first, "unchanged_index": unchanged, "metadata_reads": reads_report,
            "successful_small_edit_publications": edits, "publications": publications,
            "small_edit_latency": percentiles([item["wall_ms"] for item in observations if item["label"].startswith("edit-")]),
            "moved_files": moved_count, "move_index": moved, "moved_info": info,
            "extended_workloads": extended, "local_model_workload": model_report,
            "observations": observations, "usage": usage, "correctness_checks_passed": True,
            "unavailable": ["parser-internal invocation and inventory-deserialization counts (index receipts expose extraction and embedding counters)",
                            "per-stage peak staging bytes and component-write bytes",
                            "scheduler and pinned-reader standalone campaign (separately covered by Rust/editor tests)",
                            *( [] if local_model else ["local-model campaign not requested; use --local-model"]),
                            "paired real Claude Code/Codex tasks and independent held-out relevance judgments",
                            "clean build time and shipping-package size (binary size is a debug/release input measurement)"],
            "limitations": ["Cold means a new MCP worker, not flushed OS disk caches",
                            "Warm measurements use one persistent initialized MCP worker; initialization excluded",
                            "CLI publication wall time includes startup; usage stages retain their native accounting",
                            "Windows transfer counts are not physical storage I/O",
                            "Logical state bytes include usage metadata; no plateau or savings assertion",
                            "Latency observations do not prove absence of full-inventory parsing",
                            "Original tiny fixtures do not qualify large repositories or other platforms"]}


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("--binary", type=Path, required=True)
    parser.add_argument("--output", type=Path, required=True)
    parser.add_argument("--files", type=int, default=300)
    parser.add_argument("--edits", type=int, default=200)
    parser.add_argument("--workers", type=int, default=5)
    parser.add_argument("--reads", type=int, default=20)
    parser.add_argument("--local-model", action="store_true", help="Also measure original tiny locally provisioned word vectors")
    args = parser.parse_args()
    for name, ceiling in (("files", 4096), ("edits", 1000), ("workers", 100), ("reads", 1000)):
        if not 1 <= getattr(args, name) <= ceiling:
            parser.error(f"--{name} must be 1..{ceiling}")
    binary = args.binary.resolve(strict=True)
    with tempfile.TemporaryDirectory(prefix="airk-v2-benchmark-") as temporary:
        report = benchmark(binary, Path(temporary), args.files, args.edits, args.workers, args.reads, args.local_model)
    report["binary_version"] = subprocess.check_output([str(binary), "--version"], text=True).strip()
    args.output.parent.mkdir(parents=True, exist_ok=True)
    args.output.write_text(json.dumps(report, indent=2) + "\n", encoding="utf-8")
    print(f"Benchmark correctness passed; report: {args.output}")


if __name__ == "__main__":
    main()
