# SPDX-License-Identifier: Apache-2.0
"""Replay bounded identifier discovery on an explicitly prepared isolated project.

This harness never registers/indexes projects, installs runtimes or stops a shared
service. Fixture creation and read-only replay are separate commands. Prepare the
fixture using a reviewed native installation before running. Use two configurations
for approved baseline/candidate launchers, bound to the same frozen source bytes.
Reports contain counts, hashes, timings and response classifications, not source
text, absolute paths, command arguments or arbitrary server error strings.
Fresh status-first, fresh unprimed and primed reused connections are separate
cohorts. Unprimed startup performs only initialization/catalog discovery before
the identical nine-tool batch; its separate initial-status timing is null. The
reused cohort retains one serial warmup batch separately from measured samples.
"""
import argparse
from concurrent.futures import ThreadPoolExecutor
import hashlib
import json
import math
import os
from pathlib import Path
import queue
import re
import statistics
import subprocess
import threading
import time

SCHEMA = "airk.mcp-investigation/1"
MAX_LINE = 2 * 1024 * 1024
MAX_READ_ATTEMPTS = 64
TASK_IDS = ("definitions", "references", "literal-search", "code-facts", "outline",
            "excerpt", "context", "batch-status", "identifier-question")
COHORTS = (("fresh-frontend-status-first", True, False),
           ("fresh-frontend-unprimed", False, False),
           ("reused-connection-status-first", True, True))
ALLOWED = frozenset({"status", "symbols", "references", "search", "code_facts",
                     "read_file", "context", "outline", "file_info"})


def digest_bytes(value):
    return hashlib.sha256(value).hexdigest()


def canonical(value):
    return json.dumps(value, ensure_ascii=False, sort_keys=True, separators=(",", ":")).encode("utf-8")


def reconcile_record(path):
    """Count retained attempts without rewriting or treating prose as a timing."""
    raw = Path(path).read_bytes()
    text = raw.decode("utf-8")
    rows = [line for line in text.splitlines() if re.match(r"^\|\s*\d+\s*\|", line)]
    summary = re.search(r"cost was (\d+) MCP calls", text, re.I)
    refused = re.search(r"(\d+|Five) calls were rejected as busy", text, re.I)
    reported_calls = int(summary.group(1)) if summary else None
    reported_busy = (5 if refused.group(1).lower() == "five" else int(refused.group(1))) if refused else None
    busy = sum("Busy:" in line for line in rows)
    return {"record_sha256": digest_bytes(raw), "table_call_rows": len(rows), "table_busy_rows": busy,
        "summary_calls": reported_calls, "summary_busy": reported_busy,
        "counts_agree": reported_calls == len(rows) and reported_busy == busy,
        "note": "Table rows are attempts; retries remain separate. Matching lines, occurrences and facts are distinct units."}


def source_inventory(root):
    """Read exact fixture bytes only; the manifest lives outside the source root."""
    rows = []
    for path in sorted(Path(root).rglob("*")):
        if path.is_symlink():
            raise ValueError("fixture_link_refused")
        if path.is_file():
            rows.append({"path": path.relative_to(root).as_posix(), "bytes": path.stat().st_size,
                         "sha256": digest_bytes(path.read_bytes())})
    return rows


def create_fixture(destination):
    destination = Path(destination).resolve()
    destination.mkdir(parents=True, exist_ok=False)
    root = destination / "source"
    root.mkdir()
    contents = {}
    for module in ("alpha", "beta", "gamma", "delta", "epsilon", "zeta"):
        contents[f"src/{module}.rs"] = (
            "// SPDX-License-Identifier: Apache-2.0\n"
            f"//! Fictional {module} connection handle.\n"
            "pub struct Session { pub id: u64 }\n"
            "impl Session {\n"
            "    pub fn spawn(id: u64) -> Session { Session { id } }\n"
            "    pub fn request(&self) -> u64 { self.id }\n"
            "}\n"
            "pub fn connect() -> Session { Session::spawn(7) }\n")
    contents["src/owners.rs"] = (
        "// SPDX-License-Identifier: Apache-2.0\n"
        "pub struct OwnerSession;\n"
        "pub struct NativeOwnerSession;\n"
        "pub trait RemoteSession { fn request(&self); }\n")
    contents["client/session.ts"] = (
        "// SPDX-License-Identifier: Apache-2.0\n"
        "export interface WorkbenchSession { id: string }\n"
        "export class McpStdioSession { constructor(readonly id: string) {} }\n")
    contents["docs/session.md"] = (
        "# Fictional Session design\n\n"
        "Session denotes a local handle; unrelated modules can share that name.\n"
        "A prose mention of Session does not establish a resolved code reference.\n")
    for name, content in contents.items():
        path = root / name
        path.parent.mkdir(parents=True, exist_ok=True)
        path.write_text(content, encoding="utf-8", newline="\n")
    matching_lines, occurrences = [], 0
    for name, content in contents.items():
        if not name.startswith("src/"):
            continue
        for number, line in enumerate(content.splitlines(), 1):
            count = len(re.findall(r"\bSession\b", line))
            if count:
                matching_lines.append([name, number])
                occurrences += count
    oracle = {"identifier": "Session", "scope": "src/", "count_unit": "matching_source_lines",
              "definition_paths": sorted(name for name in contents if name.endswith(".rs") and name != "src/owners.rs"),
              "matching_lines": matching_lines, "identifier_occurrences": occurrences}
    manifest = {"schema": SCHEMA, "source_subdirectory": "source", "inventory": source_inventory(root), "oracle": oracle}
    manifest["fixture_sha256"] = digest_bytes(canonical(manifest))
    (destination / "fixture.json").write_text(json.dumps(manifest, indent=2) + "\n", encoding="utf-8")
    return manifest


def load_fixture(path):
    manifest = json.loads((Path(path) / "fixture.json").read_text(encoding="utf-8"))
    if manifest.get("source_subdirectory") != "source" or (Path(path) / "source").is_symlink():
        raise ValueError("fixture_source_redirected")
    root = (Path(path) / manifest["source_subdirectory"]).resolve()
    if manifest.get("schema") != SCHEMA or source_inventory(root) != manifest["inventory"]:
        raise ValueError("fixture_identity_mismatch")
    without_digest = {key: value for key, value in manifest.items() if key != "fixture_sha256"}
    if digest_bytes(canonical(without_digest)) != manifest["fixture_sha256"]:
        raise ValueError("fixture_manifest_mismatch")
    return root, manifest


def body(envelope):
    if isinstance(envelope.get("structuredContent"), dict):
        return envelope["structuredContent"]
    for item in envelope.get("content", []):
        if item.get("type") == "text":
            try:
                value = json.loads(item["text"])
                if isinstance(value, dict):
                    return value
            except (ValueError, KeyError):
                pass
    return {}


def typed_retry(envelope):
    """Only an explicit unsubmitted receipt permits automatic replay here."""
    error = body(envelope).get("error", {})
    if (envelope.get("isError") is True and isinstance(error, dict)
            and error.get("query_submitted") is False and error.get("retryable") is True):
        delay = error.get("retry_after_ms")
        if isinstance(delay, int) and not isinstance(delay, bool) and 0 <= delay <= 500:
            return delay / 1000
    return None


def citations(value):
    if isinstance(value, dict):
        citation = value.get("citation")
        if isinstance(citation, dict) and isinstance(citation.get("path"), str):
            yield citation
        for key, child in value.items():
            if key != "citation":
                yield from citations(child)
    elif isinstance(value, list):
        for child in value:
            yield from citations(child)


def has_evidence(value):
    """Cited records or compact fact rows bound to their own file table."""
    if any(citations(value)):
        return True
    for candidate in (value, value.get("minimal", {})):
        if isinstance(candidate, dict) and candidate.get("items") and candidate.get("files"):
            return True
    return False


def observation(envelope):
    """Never copy returned prose, paths, arbitrary reason text or payloads."""
    value = body(envelope)
    evidence = value.get("evidence", value)
    publication = evidence.get("publication", {}) if isinstance(evidence, dict) else {}
    error = value.get("error", {})
    code = error.get("code") if isinstance(error, dict) else None
    if not isinstance(code, str) or not re.fullmatch(r"[a-zA-Z0-9_]{1,80}", code):
        code = "tool_error" if envelope.get("isError") is True else None
    def state(section):
        row = value.get(section)
        return row.get("state") if isinstance(row, dict) else None
    freshness = value.get("freshness")
    hints = value.get("definitions", [])
    return {"is_error": envelope.get("isError") is True, "code": code,
            "publication_token_sha256": digest_bytes(publication["token"].encode()) if isinstance(publication.get("token"), str) else None,
            "truncated": bool(evidence.get("truncated", False)) if isinstance(evidence, dict) else None,
            "index_state": value.get("index_state"), "health": value.get("health"),
            "definition_hint_groups": len(hints) if isinstance(hints, list) else 0,
            "definition_hint_pointers": sum(len(hint.get("matches", [])) for hint in hints if isinstance(hint, dict) and isinstance(hint.get("matches"), list)) if isinstance(hints, list) else 0,
            "discovery_state": state("discovery_readiness"),
            "semantic_state": state("semantic_readiness"), "freshness_state": state("freshness"),
            "live_source_verified": freshness.get("live_source_verified") if isinstance(freshness, dict) else None}


class Session:
    def __init__(self, configuration, timeout, *, prime_status=True):
        self.configuration, self.timeout = configuration, timeout
        self.prime_status = prime_status
        self.first_status, self.initialize_through_status_ms = None, None
        self.pending, self.lock, self.write_lock = {}, threading.Lock(), threading.Lock()
        self.next_id, self.tx_bytes, self.rx_bytes = 0, 0, 0
        self.child = None

    def __enter__(self):
        started = time.perf_counter()
        self.started = started
        configuration = self.configuration
        self.child = subprocess.Popen([configuration["command"], *configuration["args"]],
            cwd=configuration.get("cwd"), env={**os.environ, **configuration.get("env", {})},
            stdin=subprocess.PIPE, stdout=subprocess.PIPE, stderr=subprocess.DEVNULL,
            creationflags=subprocess.CREATE_NO_WINDOW if os.name == "nt" else 0)
        def pump():
            try:
                while True:
                    line = self.child.stdout.readline(MAX_LINE + 1)
                    if not line or len(line) > MAX_LINE:
                        break
                    message = json.loads(line)
                    with self.lock:
                        self.rx_bytes += len(line)
                        target = self.pending.get(message.get("id"))
                    if target is not None:
                        target.put((message, len(line), time.perf_counter()))
            except (OSError, ValueError):
                pass
            finally:
                with self.lock:
                    for target in self.pending.values():
                        target.put(None)
        self.reader = threading.Thread(target=pump, daemon=True)
        self.reader.start()
        try:
            initialized, _ = self.request("initialize", {"protocolVersion": "2025-11-25", "capabilities": {},
                "clientInfo": {"name": "airk-mcp-investigation", "version": "1"}})
            self.version = initialized.get("serverInfo", {}).get("version")
            if self.version != configuration["expected_version"]:
                raise ValueError("runtime_version_mismatch")
            self.send({"jsonrpc": "2.0", "method": "notifications/initialized"})
            self.initialize_ms = (time.perf_counter() - started) * 1000
            catalog, metadata = self.request("tools/list", {})
            self.catalog_sha256 = digest_bytes(canonical(catalog))
            self.discovery_bytes = metadata["response_bytes"]
            if self.prime_status:
                envelope, self.first_status = self.tool(("status", {"response_mode": "full"}))
                self.first_status["observation"] = observation(envelope)
                self.initialize_through_status_ms = (time.perf_counter()-started)*1000
            self.startup_ms = (time.perf_counter()-started)*1000
            self.startup_wire = {"request_bytes": self.tx_bytes, "response_bytes": self.rx_bytes}
            return self
        except BaseException:
            self.__exit__(None, None, None)
            raise

    def send(self, message):
        data = canonical(message) + b"\n"
        with self.write_lock:
            self.child.stdin.write(data)
            self.child.stdin.flush()
            self.tx_bytes += len(data)
        return len(data)

    def request(self, method, params, deadline=None):
        started = time.perf_counter()
        deadline = min(deadline or math.inf, time.monotonic() + self.timeout)
        with self.lock:
            self.next_id += 1
            identifier = self.next_id
            target = queue.Queue(maxsize=2)
            self.pending[identifier] = target
        try:
            if deadline <= time.monotonic():
                raise TimeoutError("request_not_submitted")
            sent = self.send({"jsonrpc": "2.0", "id": identifier, "method": method, "params": params})
            try:
                item = target.get(timeout=max(.001, deadline - time.monotonic()))
            except queue.Empty:
                raise TimeoutError("submitted_request_timeout") from None
            if item is None:
                raise RuntimeError("transport_closed")
            message, size, received = item
            if "error" in message or not isinstance(message.get("result"), dict):
                raise RuntimeError("protocol_error")
            return message["result"], {"request_bytes": sent, "response_bytes": size,
                "elapsed_ms": (received-started)*1000, "completed_at": received}
        finally:
            with self.lock:
                self.pending.pop(identifier, None)

    def tool(self, task):
        name, arguments = task
        if name not in ALLOWED:
            raise ValueError("non_read_tool_refused")
        started = time.perf_counter()
        deadline = time.monotonic() + self.timeout
        attempts = []
        while True:
            envelope, metadata = self.request("tools/call", {"name": name, "arguments": arguments}, deadline)
            attempts.append({**metadata, **observation(envelope)})
            delay = typed_retry(envelope)
            if delay is None or time.monotonic() + delay >= deadline or len(attempts) >= MAX_READ_ATTEMPTS:
                break
            time.sleep(delay)
        return envelope, {"tool": name, "attempts": attempts, "retries": len(attempts)-1,
            "retry_limit_reached": len(attempts) >= MAX_READ_ATTEMPTS and delay is not None,
            "elapsed_ms": (time.perf_counter()-started)*1000, "completed_at": time.perf_counter()}

    def __exit__(self, *_):
        if self.child is None:
            return
        self.cleanup = {"graceful_eof": False, "forced_owned_frontend_reap": False, "exit_code": None}
        try:
            self.child.stdin.close()
            self.child.wait(timeout=10)
            self.cleanup["graceful_eof"] = self.child.returncode == 0
        except (OSError, subprocess.TimeoutExpired):
            if self.child.poll() is None:
                self.cleanup["forced_owned_frontend_reap"] = True
                self.child.kill()  # Only the frontend child created by this Session.
                self.child.wait(timeout=10)
        finally:
            self.cleanup["exit_code"] = self.child.returncode
            self.child.stdout.close()
            self.reader.join(timeout=1)


def workload():
    return [
        ("symbols", {"name": "Session", "path_prefix": "src/", "limit": 100, "max_bytes": 65536}),
        ("references", {"name": "Session", "path_prefix": "src/", "limit": 100, "max_bytes": 65536}),
        ("search", {"query": "Session", "mode": "lexical", "path_prefix": "src/", "limit": 20}),
        ("code_facts", {"name": "Session", "path_prefix": "src/", "limit": 100, "max_bytes": 65536}),
        ("outline", {"path": "src/alpha.rs", "limit": 100, "max_bytes": 65536}),
        ("read_file", {"path": "src/alpha.rs", "start_line": 1, "end_line": 8}),
        ("context", {"path": "src/beta.rs", "line": 5, "before": 2, "after": 2}),
        ("status", {}),
        ("search", {"query": "how the code uses Session", "mode": "lexical", "path_prefix": "src/", "limit": 20}),
    ]


def task_workload():
    tasks = workload()
    assert len(tasks) == len(TASK_IDS)
    return tuple(zip(TASK_IDS, tasks))


def coverage(results, oracle, expected_hashes):
    references = body(results["references"][0])
    reference_value = references.get("evidence", references)
    observed_lines = {(c["path"], c["start_line"]) for c in citations(reference_value) if "start_line" in c}
    expected_lines = {tuple(line) for line in oracle["matching_lines"]}
    # Minimal symbol replies share file identities in their own table.
    symbols = body(results["definitions"][0])
    paths = {c["path"] for c in citations(symbols)}
    minimal = symbols.get("minimal", {})
    if isinstance(minimal, dict):
        paths.update(f["path"] for f in minimal.get("files", []) if isinstance(f, dict) and "path" in f)
    paths.update(f["path"] for f in symbols.get("files", []) if isinstance(f, dict) and "path" in f)
    expected_paths = set(oracle["definition_paths"])
    referenced = list(citations(reference_value))
    symbol_citations = list(citations(symbols))
    files = symbols.get("files", [])
    if isinstance(minimal, dict):
        files = [*files, *minimal.get("files", [])]
    identities = [*referenced, *symbol_citations, *[file for file in files if isinstance(file, dict)]]
    mismatched_hashes = sum(record.get("content_hash") != expected_hashes.get(record.get("path")) for record in identities)
    return {"matching_line_unit": "unique_path_start_line", "expected_matching_lines": len(expected_lines),
        "returned_matching_lines": len(observed_lines), "missing_matching_lines": len(expected_lines-observed_lines),
        "unexpected_matching_lines": len(observed_lines-expected_lines), "expected_definition_files": len(expected_paths),
        "returned_definition_files": len(paths), "missing_definition_files": len(expected_paths-paths),
        "source_hash_mismatches": mismatched_hashes,
        "references_truncated": bool(reference_value.get("truncated", False)),
        "complete": not reference_value.get("truncated", False) and observed_lines == expected_lines and paths == expected_paths and mismatched_hashes == 0}


def run_trial(session, concurrency, oracle, expected_hashes):
    started = time.perf_counter()
    tasks = task_workload()
    with ThreadPoolExecutor(max_workers=concurrency) as executor:
        futures = [executor.submit(session.tool, task) for _, task in tasks]
        results = {}
        for (task_id, task), future in zip(tasks, futures):
            try:
                envelope, metadata = future.result()
                results[task_id] = (envelope, {**metadata, "task_id": task_id})
            except (OSError, ValueError, RuntimeError, TimeoutError) as error:
                # Preserve every failed sample without serializing error text.
                results[task_id] = ({"isError": True}, {"tool": task[0], "task_id": task_id, "failure_class": type(error).__name__,
                    "submission": "uncertain", "attempts": [], "retries": 0,
                    "elapsed_ms": (time.perf_counter()-started)*1000, "completed_at": time.perf_counter()})
    successful = [metadata["completed_at"] for envelope, metadata in results.values() if not envelope.get("isError")
                  and has_evidence(body(envelope))]
    publications = {attempt["publication_token_sha256"] for _, metadata in results.values()
                    for attempt in metadata["attempts"] if attempt["publication_token_sha256"]}
    batch_status = results["batch-status"][1]
    return {"concurrency": concurrency, "calls": [metadata for _, metadata in results.values()],
        "batch_status": batch_status,
        "batch_status_from_session_start_ms": (batch_status["completed_at"]-session.started)*1000,
        "first_useful_ms": (min(successful)-started)*1000 if successful else None,
        "task_ms": (time.perf_counter()-started)*1000, "coverage": coverage(results, oracle, expected_hashes),
        "publication_consistent": len(publications) == 1, "distinct_publications": len(publications)}


def connection_metadata(session, *, opened):
    first = session.first_status
    return {"connection_opened": opened, "status_primed": session.prime_status,
        "initialize_ms": session.initialize_ms, "startup_ms": session.startup_ms,
        "initialize_through_status_ms": session.initialize_through_status_ms,
        "first_status": first,
        "first_status_available": (not first["observation"]["is_error"] and first["observation"]["index_state"] == "available") if first is not None else None,
        "startup_wire": session.startup_wire, "discovery_bytes": session.discovery_bytes,
        "catalog_sha256": session.catalog_sha256}


def distribution(values):
    values = sorted(values)
    return {"samples": len(values), "median": statistics.median(values) if values else None,
            "p95": values[max(0, math.ceil(len(values)*.95)-1)] if values else None}


def load_configuration(path, source):
    raw = Path(path).read_bytes()
    config = json.loads(raw)
    command = Path(config["command"])
    if not command.is_absolute() or not command.is_file() or command.is_symlink():
        raise ValueError("absolute_regular_launcher_required")
    if digest_bytes(command.read_bytes()) != config.get("launcher_sha256"):
        raise ValueError("launcher_identity_mismatch")
    if config.get("route") not in ("installed-launcher", "isolated-approved-runtime"):
        raise ValueError("explicit_route_required")
    args = config.get("args", [])
    if not isinstance(args, list) or not args or args[-1] != "mcp" or not all(isinstance(v, str) for v in args):
        raise ValueError("explicit_mcp_arguments_required")
    # No re-use of a normal project by accident. A fresh fixture registration is
    # prepared externally; this script only proves its requested source binding.
    if args.count("--root") != 1 or Path(args[args.index("--root")+1]).resolve() != source:
        raise ValueError("configuration_fixture_scope_mismatch")
    if any(option in args for option in ("--additional-root", "--additional-project", "--explorer-descriptor")):
        raise ValueError("single_native_fixture_only")
    return config, digest_bytes(raw)


def run(args):
    source, manifest = load_fixture(args.fixture)
    configuration, config_digest = load_configuration(args.configuration, source)
    output = Path(args.output).resolve()
    if output == source or source in output.parents or output in source.parents:
        raise ValueError("output_source_overlap")
    output.mkdir(parents=True, exist_ok=False)
    report = {"schema": SCHEMA, "label": args.label, "route": configuration["route"],
        "configuration_sha256": config_digest, "launcher_sha256": configuration["launcher_sha256"],
        "expected_version": configuration["expected_version"], "fixture_sha256": manifest["fixture_sha256"],
        "cohorts": [], "limitations": ["Cold means frontend connection only; service/model/filesystem caches are not flushed.",
            "This deterministic read-only replay does not measure a model's reasoning or proprietary client UI.",
            "Setup/queue/execution split requires separately correlated content-free native diagnostics; end-to-end times are measured here.",
            "A launcher hash and protocol version do not authenticate selected running image/source identity; the caller must independently verify and retain approved runtime provenance.",
            "No runtime is installed and no project is registered/indexed by this harness."]}
    if args.record:
        report["retained_record_reconciliation"] = reconcile_record(args.record)
    report["workload_task_ids"] = list(TASK_IDS)
    expected_hashes = {row["path"]:row["sha256"] for row in manifest["inventory"]}
    try:
        for concurrency in args.concurrency:
            for cohort, primed, reuse in COHORTS:
                trials = []
                group = {"concurrency": concurrency, "connection": cohort, "status_primed": primed, "trials": trials}
                report["cohorts"].append(group)
                reusable = None
                try:
                    if reuse:
                        reusable = Session(configuration, args.timeout, prime_status=True).__enter__()
                        group["connection_start"] = connection_metadata(reusable, opened=True)
                        # Retain this serial warm-up separately; the measured
                        # reused samples never masquerade as fresh connections.
                        group["warmup"] = run_trial(reusable, 1, manifest["oracle"], expected_hashes)
                    for sample in range(args.samples):
                        session = reusable or Session(configuration, args.timeout, prime_status=primed).__enter__()
                        before_tx, before_rx = session.tx_bytes, session.rx_bytes
                        trial = None
                        try:
                            trial = run_trial(session, concurrency, manifest["oracle"], expected_hashes)
                            trial.update(sample=sample, **connection_metadata(session, opened=not reuse),
                                request_wire_bytes=session.tx_bytes-before_tx, response_wire_bytes=session.rx_bytes-before_rx)
                            startup = 0 if reuse else session.startup_ms
                            trial["task_with_connection_ms"] = trial["task_ms"] + startup
                            trial["first_useful_with_connection_ms"] = trial["first_useful_ms"] + startup if trial["first_useful_ms"] is not None else None
                            trials.append(trial)
                        finally:
                            if not reuse:
                                session.__exit__(None, None, None)
                                if trial is not None:
                                    trial["frontend_cleanup"] = session.cleanup
                finally:
                    if reusable:
                        reusable.__exit__(None, None, None)
                        group["frontend_cleanup"] = reusable.cleanup
                starts = [group["connection_start"]] if reuse else [trial for trial in trials if trial["connection_opened"]]
                warmup = group.get("warmup")
                group.update(task_ms=distribution([trial["task_ms"] for trial in trials]),
                    task_with_connection_ms=distribution([trial["task_with_connection_ms"] for trial in trials]),
                    connections_started=len(starts),
                    first_status_connections_available=sum(start["first_status_available"] is True for start in starts),
                    first_status_connection_count=sum(start["first_status"] is not None for start in starts),
                    first_status_observation_unit="Separate initial status per primed connection only; unprimed status is measured inside the fixed task batch.",
                    coverage_complete=all(trial["coverage"]["complete"] and trial["publication_consistent"] for trial in trials)
                        and (warmup is None or (warmup["coverage"]["complete"] and warmup["publication_consistent"])),
                    cleanup_clean=all(trial.get("frontend_cleanup", group.get("frontend_cleanup", {})).get("graceful_eof", False) for trial in trials))
        report["status"] = "complete"
    except (OSError, ValueError, RuntimeError, TimeoutError) as error:
        # Class name only: arbitrary exception messages can contain local paths.
        report.update(status="failed", failure_class=type(error).__name__)
    finally:
        report["source_unchanged"] = source_inventory(source) == manifest["inventory"]
        (output / "report.json").write_text(json.dumps(report, indent=2) + "\n", encoding="utf-8")
    return report["status"] == "complete" and report["source_unchanged"] and all(c.get("coverage_complete", False) and c.get("cleanup_clean", False) for c in report["cohorts"])


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    commands = parser.add_subparsers(dest="operation", required=True)
    fixture = commands.add_parser("fixture")
    fixture.add_argument("--destination", type=Path, required=True)
    audit = commands.add_parser("audit-record")
    audit.add_argument("--record", type=Path, required=True)
    replay = commands.add_parser("run")
    replay.add_argument("--configuration", type=Path, required=True)
    replay.add_argument("--fixture", type=Path, required=True)
    replay.add_argument("--output", type=Path, required=True)
    replay.add_argument("--label", choices=("baseline", "candidate"), required=True)
    replay.add_argument("--samples", type=int, default=30)
    replay.add_argument("--concurrency", type=int, nargs="+", choices=(1, 4, 6, 8), default=[1, 4, 6, 8])
    replay.add_argument("--timeout", type=float, default=30)
    replay.add_argument("--record", type=Path, help="Optional retained investigation to reconcile without editing it")
    args = parser.parse_args()
    if args.operation == "fixture":
        result = create_fixture(args.destination)
        print(json.dumps({"fixture_sha256": result["fixture_sha256"], "files": len(result["inventory"])}))
    elif args.operation == "audit-record":
        print(json.dumps(reconcile_record(args.record), indent=2))
    else:
        if not 1 <= args.samples <= 1000 or not 0 < args.timeout <= 120:
            parser.error("samples must be 1..1000 and timeout >0..120 seconds")
        raise SystemExit(0 if run(args) else 1)


if __name__ == "__main__":
    main()
