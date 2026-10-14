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
        except BaseException:
            self.close()
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
        self.process.stdin.write(json.dumps(payload).encode() + b"\n")
        self.process.stdin.flush()

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
        if self.process.poll() is None:
            self.process.kill()
        self.process.wait(timeout=10)
        self.thread.join(timeout=10)
        self.process.stdin.close()
        self.process.stdout.close()
        self.errors.close()


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


def benchmark(binary, directory, files, edits, workers, reads):
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
    if moved["generation"] <= generation:
        raise AssertionError("moves failed to publish")
    info = run("moved-info", "info", "moved/module_0000.ts")
    if (info["generation"] != moved["generation"] or info["path"] != "moved/module_0000.ts"
            or info["content_hash"] != SCALE.hash_file(root / "moved/module_0000.ts")[0]):
        raise AssertionError("moved metadata did not bind the new path and original bytes")
    usage = run("usage", "usage", "report")
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
            "observations": observations, "usage": usage, "correctness_checks_passed": True,
            "unavailable": ["parser/inference invocation counts and inventory-deserialization counts",
                            "per-stage peak staging bytes and component-write bytes",
                            "local-model, branch-replacement, composite/notebook, scheduler and pinned-reader workloads",
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
    args = parser.parse_args()
    for name, ceiling in (("files", 4096), ("edits", 1000), ("workers", 100), ("reads", 1000)):
        if not 1 <= getattr(args, name) <= ceiling:
            parser.error(f"--{name} must be 1..{ceiling}")
    binary = args.binary.resolve(strict=True)
    with tempfile.TemporaryDirectory(prefix="airk-v2-benchmark-") as temporary:
        report = benchmark(binary, Path(temporary), args.files, args.edits, args.workers, args.reads)
    report["binary_version"] = subprocess.check_output([str(binary), "--version"], text=True).strip()
    args.output.parent.mkdir(parents=True, exist_ok=True)
    args.output.write_text(json.dumps(report, indent=2) + "\n", encoding="utf-8")
    print(f"Benchmark correctness passed; report: {args.output}")


if __name__ == "__main__":
    main()
