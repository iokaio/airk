# SPDX-License-Identifier: Apache-2.0
"""Binary/source-bound synthetic retrieval receipts through persistent stdio MCP.

Fixed judgments are outside indexed sources. Original static vectors exercise the
adapter and ranking contracts; they do not qualify pretrained semantic relevance.
"""
import argparse
from concurrent.futures import ThreadPoolExecutor
import hashlib
import importlib.util
import json
import math
import os
from pathlib import Path
import platform
import queue
import statistics
import subprocess
import tempfile
import threading
import time

fixture_spec = importlib.util.spec_from_file_location("airk_service_fixture", Path(__file__).with_name("check-service-clients.py"))
clients = importlib.util.module_from_spec(fixture_spec)
fixture_spec.loader.exec_module(clients)


CASES = [
    {"name": "code", "query": "revoke_session", "mode": "lexical", "expected": "code/auth.rs"},
    {"name": "prose", "query": "recovery", "mode": "hybrid", "path_prefix": "docs/", "expected": "docs/recovery.md"},
    {"name": "synonym", "query": "login", "mode": "vector", "path_prefix": "code/", "expected": "code/auth.rs"},
    {"name": "minified", "query": "compacttoken", "mode": "lexical", "expected": "code/minified.js"},
    {"name": "long-line", "query": "tailmarker", "mode": "hybrid", "expected": "long.txt", "expected_text": "tailmarker"},
    {"name": "spanish", "query": "recuperar", "mode": "hybrid", "path_prefix": "es/", "expected": "es/access.txt"},
    {"name": "french", "query": "connexion", "mode": "hybrid", "path_prefix": "fr/", "expected": "fr/access.txt"},
    {"name": "alias", "query": "requirement", "mode": "hybrid", "path_prefix": "@specs/", "expected": "@specs/limits.md"},
    {"name": "sparse-scope", "query": "needle", "mode": "vector", "path_prefix": "wanted/", "expected": "wanted/only.txt"},
]


def digest(path):
    value = hashlib.sha256()
    with path.open("rb") as source:
        for chunk in iter(lambda: source.read(1024 * 1024), b""):
            value.update(chunk)
    return value.hexdigest()


def source_identity():
    repository = Path(__file__).resolve().parents[1]
    command = ["git", "-c", f"safe.directory={repository.as_posix()}", "-C", str(repository)]
    commit = subprocess.check_output(command + ["rev-parse", "HEAD"], text=True).strip()
    names = subprocess.check_output(command + ["ls-files", "-co", "--exclude-standard", "-z"]).decode().split("\0")
    source = hashlib.sha256()
    for name in sorted(set(names)):
        if not name or name.startswith(("temp/", "artifacts/", "target/", ".airk/")):
            continue
        path = repository / name
        if path.is_file() and not path.is_symlink():
            source.update(name.encode() + b"\0" + bytes.fromhex(digest(path)))
    dirty = bool(subprocess.check_output(command + ["status", "--porcelain", "--untracked-files=normal"]))
    return {"commit": commit, "dirty": dirty, "public_tree_sha256": source.hexdigest()}


class Session:
    def __init__(self, command):
        environment = dict(os.environ)
        environment.pop("AIRK_USAGE_MODE", None)
        environment["AIRK_MCP_RESPONSE_MODE"] = "full"
        self.process = subprocess.Popen(command + ["mcp"], stdin=subprocess.PIPE,
                                        stdout=subprocess.PIPE, stderr=subprocess.DEVNULL, env=environment)
        self.messages = queue.Queue(maxsize=64)
        self.sequence = 0
        self.publication = None
        def read():
            try:
                while True:
                    line = self.process.stdout.readline(1_048_577)
                    if not line or len(line) > 1_048_576:
                        raise RuntimeError("MCP closed or exceeded response bound")
                    self.messages.put(json.loads(line), timeout=1)
            except (OSError, ValueError, RuntimeError, queue.Full) as error:
                try:
                    self.messages.put(error, timeout=1)
                except queue.Full:
                    pass
        self.reader = threading.Thread(target=read, daemon=True)
        self.reader.start()
        try:
            self.request("initialize", {"protocolVersion": "2025-11-25", "capabilities": {},
                                       "clientInfo": {"name": "airk-qualification", "version": "1"}})
            self.send({"jsonrpc": "2.0", "method": "notifications/initialized"})
        except BaseException:
            self.close()
            raise

    def send(self, value):
        self.process.stdin.write(json.dumps(value).encode() + b"\n")
        self.process.stdin.flush()

    def request(self, method, arguments, deadline=None):
        deadline = min(deadline if deadline is not None else float("inf"), time.monotonic() + 120)
        clients.remaining(deadline, 120)
        self.sequence += 1
        self.send({"jsonrpc": "2.0", "id": self.sequence, "method": method, "params": arguments})
        while True:
            try:
                message = self.messages.get(timeout=clients.remaining(deadline, 120))
            except queue.Empty as error:
                raise TimeoutError("MCP qualification deadline") from error
            if isinstance(message, BaseException):
                raise RuntimeError("MCP response reader failed") from message
            if not isinstance(message, dict):
                raise RuntimeError("MCP response is not an object")
            if message.get("id") == self.sequence:
                if "error" in message:
                    raise RuntimeError(str(message["error"]))
                return message["result"]
            if time.monotonic() > deadline:
                raise TimeoutError("MCP qualification deadline")

    def search(self, case):
        arguments = {key: case[key] for key in ("query", "mode", "path_prefix") if key in case}
        arguments.update(limit=5, max_bytes=65536, response_mode="full")
        start = time.perf_counter_ns()
        retry_deadline = time.monotonic() + 15
        params = {"name": "airk_search", "arguments": arguments}
        result, attempt = clients.read_envelope(lambda until: self.request("tools/call", params, until), retry_deadline, 601)
        elapsed = (time.perf_counter_ns() - start) / 1_000_000
        if result.get("isError"):
            raise RuntimeError(str(result["content"]))
        value = json.loads(result["content"][0]["text"])
        publication = value["publication"]["token"]
        if self.publication is not None and publication != self.publication:
            raise AssertionError("fixed fixture publication changed during session")
        self.publication = publication
        paths = [hit["citation"]["path"] for hit in value["hits"]]
        rank = next((index + 1 for index, hit in enumerate(value["hits"])
                     if hit["citation"]["path"] == case["expected"]
                     and case.get("expected_text", "") in hit["text"]), None)
        if rank is None:
            raise AssertionError(f"fixed retrieval judgment failed: {case['name']}; returned {paths}")
        if value["hits"][rank - 1]["citation"]["content_hash"] != case["source_sha256"]:
            raise AssertionError(f"fixture citation hash differs: {case['name']}")
        return {"case": case["name"], "latency_ms": elapsed, "admission_retries": attempt, "recall_at_5": 1,
                "reciprocal_rank_at_5": 1 / rank, "publication": publication}

    def close(self):
        try:
            if not self.process.stdin.closed:
                self.process.stdin.close()
            if self.process.wait(timeout=15) != 0:
                raise RuntimeError("MCP exited unsuccessfully during qualification shutdown")
        except (OSError, subprocess.TimeoutExpired) as error:
            self.process.kill()
            self.process.wait(timeout=15)
            raise RuntimeError("MCP did not shut down cleanly; child was terminated") from error
        finally:
            self.process.stdout.close()
            self.reader.join(timeout=2)


def stats(records):
    values = sorted(record["latency_ms"] for record in records)
    return {"samples": len(values), "p50_ms": statistics.median(values),
            "p95_ms": values[math.ceil(len(values) * 0.95) - 1],
            "recall_at_5": statistics.mean(record["recall_at_5"] for record in records),
            "mrr_at_5": statistics.mean(record["reciprocal_rank_at_5"] for record in records)}


def fixture(binary, base, count, trials, metrics_module, detail="balanced"):
    root, state, alias = (base / name for name in ("root", "state", "alias"))
    root.mkdir(parents=True)
    alias.mkdir()
    sources = {
        "code/auth.rs": "fn revoke_session() {} // authenticate\n",
        "docs/recovery.md": "# Account access\nUse a recovery link after forgetting a password.\n",
        "code/minified.js": "const compacttoken=()=>{return 'authenticate'};" * 250,
        "long.txt": "banana " * 1500 + "tailmarker authenticate",
        "es/access.txt": "Puede recuperar su cuenta con una clave temporal.\n",
        "fr/access.txt": "La connexion utilise un code temporaire.\n",
        "wanted/only.txt": "needle banana\n",
    }
    for name, text in sources.items():
        path = root / name
        path.parent.mkdir(exist_ok=True)
        path.write_text(text, encoding="utf-8")
    (alias / "limits.md").write_text("The requirement sets a timeout of 30 seconds.\n", encoding="utf-8")
    vocabulary = {word: [1.0, 0.0] for word in ("authenticate", "login", "recovery", "password", "needle", "tailmarker", "recuperar", "connexion", "requirement")}
    vocabulary["banana"] = [0.0, 1.0]
    (root / "distractors").mkdir()
    for index in range(count):
        word = f"word{index}"
        vocabulary[word] = [1.0, (index + 1) / (count * 10)]
        (root / "distractors" / f"{index:05d}.txt").write_text(word + "\n", encoding="utf-8")
    state.mkdir()
    model = base / "model-input"
    model.mkdir(parents=True)
    (model / "vocabulary.json").write_text(json.dumps(vocabulary), encoding="utf-8")
    (model / "model.json").write_text(json.dumps({"schema": 1, "name": "original-qualification-fixture",
        "license": "Apache-2.0", "dimensions": 2, "sha256": digest(model / "vocabulary.json")}), encoding="utf-8")
    service = clients.Fixture(binary.parent, base)
    assert service.binary == binary
    with service:
        service.register("qualification", root, state)
        service.configure("qualification", {"operation": "add_root", "alias": "specs", "path": str(alias)})
        service.configure("qualification", {"operation": "detail", "level": detail})
        service.configure("qualification", {"operation": "install_model", "directory": str(model)})
        service.job("qualification", "index")
        deadline = time.monotonic() + 300
        while True:
            status = service.published_status("qualification", deadline)
            readiness = clients.semantic_readiness(status)
            assert readiness["state"] != "failed", "Semantic work explicitly failed"
            if readiness["state"] == "ready" and readiness["completed_artifacts"] == count + 8:
                break
            assert time.monotonic() < deadline, readiness
            time.sleep(0.05)
        command = [str(binary), *service.arguments("qualification")]
        cases = [dict(case, source_sha256=digest(
            alias / case["expected"].removeprefix("@specs/")
            if case["expected"].startswith("@specs/") else root / case["expected"])) for case in CASES]
        modes = {}
        publications = set()
        for disabled in (True, False):
            service.configure("qualification", {"operation": "usage", "mode": "disabled" if disabled else "aggregates_only"})
            session = Session(command)
            try:
                cold = [session.search(cases[0])]
                warm = [session.search(case) for _ in range(trials) for case in cases]
                publications.add(session.publication)
                resources = metrics_module.process_metrics(session.process, binary)
            finally:
                session.close()
            modes["usage_disabled" if disabled else "usage_enabled"] = {"cold": stats(cold), "warm": stats(warm),
                "process_counters": resources, "judgments": warm[:len(CASES)]}
        def concurrent(_):
            session = Session(command)
            try:
                return session.search(cases[-1])
            finally:
                session.close()
        with ThreadPoolExecutor(max_workers=4) as pool:
            parallel = list(pool.map(concurrent, range(8)))
        publications.update(record["publication"] for record in parallel)
        if len(publications) != 1:
            raise AssertionError("usage modes/processes observed different fixture publications")
        modes["four_processes"] = stats(parallel)
        return {"distractors": count, "publication": publications.pop(),
                "source_hashes": {case["expected"]: case["source_sha256"] for case in cases},
                "modes": modes, "usage": json.loads(service.run(*service.arguments("qualification"), "usage", "report").stdout)["report"]}


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("--binary", type=Path, required=True)
    parser.add_argument("--output", type=Path, required=True)
    parser.add_argument("--trials", type=int, default=3)
    parser.add_argument("--detail", choices=("balanced", "agent_lean", "essential", "detailed"),
                        default="balanced", help="Explicit evidence mode under test")
    args = parser.parse_args()
    if not 2 <= args.trials <= 30:
        parser.error("trials must be 2..30")
    binary = args.binary.resolve(strict=True)
    spec = importlib.util.spec_from_file_location("scale_metrics", Path(__file__).with_name("benchmark-scale.py"))
    metrics = importlib.util.module_from_spec(spec)
    spec.loader.exec_module(metrics)
    report = {"schema": "airk-retrieval-qualification/1", "source": source_identity(), "index_detail": args.detail,
        "binary_sha256": digest(binary), "binary_version": subprocess.check_output([str(binary), "--version"], text=True, timeout=15).strip(),
        "platform": platform.platform(), "architecture": platform.machine(),
        "judgments_sha256": hashlib.sha256(json.dumps(CASES, sort_keys=True).encode()).hexdigest(),
        "cache_conditions": "cold means first query in a new MCP process; warm reuses its direct project connection and shared worker; OS caches uncontrolled",
        "source_binding": "Observed checkout and binary digests must stay stable during this run. Local dirty-tree receipts do not prove binary build provenance; CI associates a clean checkout with its build.",
        "ranking_units": "Top five returned chunks, with each fixed judgment requiring its expected path and any expected text. Repeated paths retain their chunk rank.",
        "process_cleanup": "Owned MCP children must exit successfully after stdin closes; each owned service must drain. Static vectors use the shared helper. No external provider or source deletion.",
        "scope": "Original synthetic static vectors and fixed judgments outside indexed sources. No production semantic-quality, larger-than-memory, physical-I/O, installed-editor or power-loss qualification.",
        "workloads": []}
    with tempfile.TemporaryDirectory(prefix="airk-retrieval-qualification-") as temporary:
        base = Path(temporary).resolve()
        for count in (32, 256):
            report["workloads"].append(fixture(binary, base / str(count), count, args.trials, metrics, args.detail))
    report["source_after"] = source_identity()
    report["binary_sha256_after"] = digest(binary)
    report["passed"] = (report["source"] == report["source_after"]
                        and report["binary_sha256"] == report["binary_sha256_after"])
    args.output.parent.mkdir(parents=True, exist_ok=True)
    args.output.write_text(json.dumps(report, indent=2) + "\n", encoding="utf-8")
    print(json.dumps({"passed": report["passed"], "receipt": str(args.output), "workloads": 2, "cases": len(CASES)}))
    if not report["passed"]:
        raise SystemExit("Source or binary changed during qualification; rebuild and rerun on a stable checkout")


if __name__ == "__main__":
    main()
