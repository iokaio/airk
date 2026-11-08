# SPDX-License-Identifier: Apache-2.0
"""Replay bounded identifier discovery on an explicitly prepared isolated project.

The run command never registers/indexes projects or installs/stops a shared
service. The owned command prepares and drains only disposable original fixtures
with an exact staged pair. Use two configurations
for approved baseline/candidate launchers, bound to the same frozen source bytes.
Reports contain counts, hashes, timings and response classifications, not source
text, absolute paths, command arguments or arbitrary server error strings.
Fresh status-first, fresh unprimed and primed reused connections are separate
cohorts. Unprimed startup performs only initialization/catalog discovery before
the identical nine-tool batch; its separate initial-status timing is null. The
reused cohort retains one serial warmup batch separately from measured samples.
An explicit AIRK_COMBINED_COMPARE_LEGACY=1 owned-fixture comparison additionally
retains full query replies in its ignored output directory; its summary remains
content-free and never claims whole-body or multi-index legacy equivalence.
AIRK_COMBINED_RETAIN_RAW=1 separately retains complete received combined query
attempts for bounded diagnostics, including failed responses; neither is default.
"""
import argparse
from concurrent.futures import ThreadPoolExecutor
from contextlib import nullcontext
import hashlib
import importlib.util
import json
import math
import os
from pathlib import Path
import queue
import re
import statistics
import subprocess
import tempfile
import threading
import time
import uuid

evidence_spec = importlib.util.spec_from_file_location("combined_evidence", Path(__file__).with_name("combined_evidence.py"))
combined_evidence = importlib.util.module_from_spec(evidence_spec)
evidence_spec.loader.exec_module(combined_evidence)
http_spec = importlib.util.spec_from_file_location("benchmark_https_fixture", Path(__file__).with_name("http_fixture.py"))
http_fixture = importlib.util.module_from_spec(http_spec)
http_spec.loader.exec_module(http_fixture)

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
COMBINED_SCHEMA = "airk.combined-investigation/1"
LEGACY_COMPARE_ENV = "AIRK_COMBINED_COMPARE_LEGACY"
COMBINED_RAW_ENV = "AIRK_COMBINED_RETAIN_RAW"


class LegacyComparisonError(ValueError):
    """Fixed content-free outcome for the explicitly opted-in owned comparison."""
    def __init__(self, code):
        super().__init__(code)
        self.code = code


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


def combined_source_inventory(root):
    """Bound snapshots of the deliberately small original fan-out fixtures."""
    paths, entries, total = [], 0, 0
    for path in Path(root).rglob("*"):
        entries += 1
        if entries > 512 or path.is_symlink():
            raise ValueError("combined_fixture_entries_or_link")
        if path.is_file():
            size = path.stat().st_size
            total += size
            if len(paths) >= 128 or total > 16 * 1024 * 1024:
                raise ValueError("combined_fixture_size_limit")
            paths.append((path, size))
    rows = []
    for path, size in sorted(paths):
        with path.open("rb") as source:
            raw = source.read(size + 1)
        if len(raw) != size:
            raise ValueError("combined_fixture_changed_during_snapshot")
        rows.append({"path": path.relative_to(root).as_posix(), "bytes": size, "sha256": digest_bytes(raw)})
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
    receipt = value.get("source_verification")
    verification = None
    if isinstance(receipt, dict) and receipt.get("schema") == "airk.saved-source-verification/1":
        counts = receipt.get("counts", {})
        if not isinstance(counts, dict):
            counts = {}
        verification = {
            "mode": receipt.get("mode") if receipt.get("mode") in ("off", "report", "require") else "unknown",
            "status": receipt.get("status") if receipt.get("status") in ("verified", "partial", "unverified", "not_applicable", "requirements_unmet") else "unknown",
            "counts": {key: count if type(count) is int and count >= 0 else None
                       for key in ("references", "requested_sources", "checked_sources", "matched_sources", "mismatched_sources", "unchecked_sources", "bytes_read")
                       for count in [counts.get(key)]}}
    return {"is_error": envelope.get("isError") is True, "code": code,
            "publication_token_sha256": digest_bytes(publication["token"].encode()) if isinstance(publication.get("token"), str) else None,
            "truncated": bool(evidence.get("truncated", False)) if isinstance(evidence, dict) else None,
            "index_state": value.get("index_state"), "health": value.get("health"),
            "definition_hint_groups": len(hints) if isinstance(hints, list) else 0,
            "definition_hint_pointers": sum(len(hint.get("matches", [])) for hint in hints if isinstance(hint, dict) and isinstance(hint.get("matches"), list)) if isinstance(hints, list) else 0,
            "discovery_state": state("discovery_readiness"),
            "semantic_state": state("semantic_readiness"), "freshness_state": state("freshness"),
            "live_source_verified": freshness.get("live_source_verified") if isinstance(freshness, dict) else None,
            "source_verification": verification}


def modern_configuration(config, *, combined):
    allowed = {"schema", "transport", "command", "args", "launcher_sha256", "route", "expected_version", "control", "selection", "connections_file", "cwd", "source_root", "verification_override"}
    if (set(config) - allowed or type(config.get("schema")) is not int or config["schema"] != 2
            or config.get("transport") != "https" or config.get("args") != []
            or config.get("route") != "isolated-approved-runtime"):
        raise ValueError("explicit_https_configuration_required")
    control = Path(config["control"])
    if not control.is_absolute() or control.is_symlink() or not control.is_dir():
        raise ValueError("explicit_owned_control_required")
    selected = config.get("selection", {})
    if (set(selected) != {"kind", "selection"} or selected["kind"] != ("combined" if combined else "scoped")
            or not isinstance(selected["selection"], dict) or len(canonical(selected)) > 8192):
        raise ValueError("explicit_https_selection_required")
    if not isinstance(config.get("connections_file"), str):
        raise ValueError("native_pre_enrollment_required")
    # The native fixture owner enrolls each exact selector before measurement.
    # JSON selectors never manufacture authority or cause console enrollment.
    return config


def selection_identity(value):
    if isinstance(value, list):
        return [selection_identity(item) for item in value]
    if isinstance(value, dict):
        return {key:selection_identity(item) for key,item in value.items() if key != "expected_catalog_revision"}
    return value


def enroll_https(configuration, timeout):
    deadline = time.monotonic() + timeout
    def rpc(method):
        result = subprocess.run([configuration["command"], "service", "rpc", "--runtime-dir", configuration["control"], "--stdin"],
            input=canonical(method), stdout=subprocess.PIPE, stderr=subprocess.DEVNULL,
            cwd=configuration.get("cwd"), timeout=max(.001, deadline-time.monotonic()),
            creationflags=subprocess.CREATE_NO_WINDOW if os.name == "nt" else 0)
        if result.returncode or len(result.stdout) > 65536:
            raise RuntimeError("native_https_metadata_refused")
        return json.loads(result.stdout)
    health = rpc("health")
    if health.get("status") != "health":
        raise RuntimeError("native_https_health_unavailable")
    roster = json.loads(http_fixture.private_bytes(configuration["connections_file"], configuration["control"], 262144))
    if (not isinstance(roster, dict) or set(roster) != {"schema", "client", "connections"}
            or roster["schema"] != 1 or roster["client"] != "unknown"
            or not isinstance(roster["connections"], list) or not 1 <= len(roster["connections"]) <= 32):
        raise ValueError("native_pre_enrollment_roster_invalid")
    matches = [connection for connection in roster["connections"]
               if selection_identity(connection.get("selection")) == selection_identity(configuration["selection"])]
    if len(matches) != 1:
        raise ValueError("native_pre_enrollment_exact_selection_required")
    connection = matches[0]
    http_fixture.validate_enrollment(connection, configuration["control"], configuration["selection"], connection["connection_id"])
    http_fixture.validate_native_identity(connection, health["observation"])
    if time.monotonic() >= deadline:
        raise TimeoutError("native HTTPS metadata deadline exhausted")
    return connection


def configuration_for_case(configuration, case):
    if configuration.get("transport") != "https":
        return configuration
    # Every narrowed selector must match one prior native enrollment exactly.
    # Query arguments never reuse a broader credential.
    selected = json.loads(json.dumps(configuration["selection"]))
    frozen = selected["selection"]
    if (not case["project_ids"] or not case["index_ids"]
            or not set(case["project_ids"]).issubset(frozen["project_ids"])
            or frozen.get("index_ids") is not None and not set(case["index_ids"]).issubset(frozen["index_ids"])):
        raise ValueError("case_widens_frozen_connection_selection")
    frozen["project_ids"] = list(case["project_ids"])
    frozen["index_ids"] = list(case["index_ids"])
    return {**configuration, "selection":selected}


def require_historical_stdio(config):
    version = re.fullmatch(r"([0-9]+)\.([0-9]+)\.([0-9]+)", config.get("expected_version", ""))
    if version and tuple(map(int, version.groups())) >= (1,8,14):
        raise ValueError("modern_stdio_configuration_retired")


def cleanup_ok(cleanup):
    if cleanup.get("frontend_process_created") is False:
        return cleanup.get("http_session_delete") is True
    return cleanup.get("graceful_eof") is True


class Session:
    def __init__(self, configuration, timeout, *, prime_status=True):
        self.configuration, self.timeout = configuration, timeout
        self.prime_status = prime_status
        self.first_status, self.initialize_through_status_ms = None, None
        self.pending, self.lock, self.write_lock = {}, threading.Lock(), threading.Lock()
        self.next_id, self.tx_bytes, self.rx_bytes = 0, 0, 0
        self.child = None
        self.https = configuration.get("transport") == "https"
        self.networks, self.local = [], threading.local()

    def __enter__(self):
        started = time.perf_counter()
        self.started = started
        configuration = self.configuration
        if self.https:
            self.connection = enroll_https(configuration, self.timeout)
            self.network = http_fixture.McpClient(self.connection, configuration["control"], self.timeout)
            self.networks.append(self.network)
            self.local.network = self.network
        else:
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
        if not self.https:
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
        if self.https:
            self.network.send(message)
            size = len(canonical(message))
            with self.lock:
                self.tx_bytes += size
            return size
        data = canonical(message) + b"\n"
        with self.write_lock:
            self.child.stdin.write(data)
            self.child.stdin.flush()
            self.tx_bytes += len(data)
        return len(data)

    def request(self, method, params, deadline=None):
        if self.https:
            return self.https_request(method, params, deadline)
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
                error = RuntimeError("protocol_error")
                error.raw_response = message
                raise error
            return message["result"], {"request_bytes": sent, "response_bytes": size,
                "elapsed_ms": (received-started)*1000, "completed_at": received}
        finally:
            with self.lock:
                self.pending.pop(identifier, None)

    def https_request(self, method, params, deadline=None):
        started = time.perf_counter()
        deadline = min(deadline or math.inf, time.monotonic() + self.timeout)
        if deadline <= time.monotonic():
            raise TimeoutError("request_not_submitted")
        with self.lock:
            self.next_id += 1
            identifier = self.next_id
        network = getattr(self.local, "network", None)
        if network is None:
            # Each measuring thread owns one TLS connection, while all of them
            # use the exact same initialized MCP session and frozen authority.
            network = http_fixture.McpClient(self.connection, self.configuration["control"], max(.001, deadline-time.monotonic()))
            network.session, network.protocol = self.network.session, self.network.protocol
            self.local.network = network
            with self.lock:
                self.networks.append(network)
        network.connection.timeout = max(.001, deadline-time.monotonic())
        if network.connection.sock is not None:
            network.connection.sock.settimeout(network.connection.timeout)
        message = {"jsonrpc":"2.0","id":identifier,"method":method,"params":params}
        sent = len(canonical(message))
        with self.lock:
            self.tx_bytes += sent
        try:
            reply = network._request("POST", message)
        except http_fixture.HttpRefused as refused:
            with self.lock:
                self.rx_bytes += len(canonical(refused.body))
            error = RuntimeError("http_refused")
            # Actual transport evidence, never a fabricated JSON-RPC envelope.
            error.raw_response = {"http_status":refused.status,"body":refused.body}
            raise error from refused
        received = time.perf_counter()
        size = len(canonical(reply))
        with self.lock:
            self.rx_bytes += size
        if not isinstance(reply, dict) or reply.get("id") != identifier or "error" in reply or not isinstance(reply.get("result"), dict):
            error = RuntimeError("protocol_error")
            error.raw_response = reply
            raise error
        if method == "initialize":
            protocol = reply["result"].get("protocolVersion")
            if not isinstance(protocol, str) or not re.fullmatch(r"[A-Za-z0-9_.-]{1,64}", protocol):
                raise ValueError("invalid_negotiated_protocol")
            network.protocol = protocol
        return reply["result"], {"request_bytes":sent,"response_bytes":size,"elapsed_ms":(received-started)*1000,"completed_at":received}

    def tool(self, task):
        name, arguments = task
        if name not in ALLOWED:
            raise ValueError("non_read_tool_refused")
        override = self.configuration.get("verification_override")
        if override is not None and name != "status":
            arguments = {**arguments, "verification": override}
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
        if self.https:
            failures = []
            for network in self.networks[1:]:
                try:
                    network.close(delete=False)
                except Exception:
                    failures.append("owned_tls_connection_close_failed")
            try:
                if self.networks:
                    self.networks[0].close()
            except Exception:
                failures.append("owned_http_session_delete_failed")
                raise
            finally:
                self.cleanup = {"http_session_delete":not failures,"owned_tls_connections":len(self.networks),"frontend_process_created":False}
            if failures:
                raise RuntimeError(failures[0])
            return
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


class RetainedSession(Session):
    """Opt-in raw query outcomes, drained after timing and never in summaries."""
    def __init__(self, *args, **kwargs):
        self.query_replies = []
        super().__init__(*args, **kwargs)

    def request(self, method, params, deadline=None):
        try:
            result, metadata = super().request(method, params, deadline)
        except RuntimeError as error:
            if method == "tools/call" and hasattr(error, "raw_response"):
                self.query_replies.append({"jsonrpc_response": error.raw_response})
            raise
        if method == "tools/call":
            self.query_replies.append(result)
        return result, metadata


def retain_query_replies(stream, session, sample, route, additional_stream=None, selected_indexes=None):
    replies, session.query_replies = session.query_replies, []
    for attempt, envelope in enumerate(replies, 1):
        row = {"sample": sample, "route": route, "attempt": attempt, "envelope": envelope}
        if selected_indexes is not None:
            row["selected_indexes"] = selected_indexes
        encoded = json.dumps(row, ensure_ascii=False) + "\n"
        for destination in (stream, additional_stream):
            if destination is not None:
                destination.write(encoded)
    for destination in (stream, additional_stream):
        if destination is not None:
            destination.flush()


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


def combined_observation(envelope, case, source_hashes):
    """Check native combined evidence without retaining source text or locations.

    Result membership alone does not prove zero work on unselected indexes.
    That invariant needs native work counters or the actual-process tests.
    """
    value = body(envelope)
    if value.get("schema") != "airk.combined-read/1":
        raise ValueError("combined_result_required")
    selected = case["index_ids"]
    members = value.get("members", [])
    observed = [member.get("index_id") for member in members]
    if observed != sorted(selected) or len(set(observed)) != len(observed):
        raise ValueError("combined_member_selection_mismatch")
    limits = case["arguments"]
    for field, maximum in (("limit", limits["limit"]), ("max_bytes", limits["max_bytes"]),
                           ("verification_max_sources", limits.get("verification_max_sources", 8)),
                           ("verification_max_bytes", limits.get("verification_max_bytes", 16 * 1024 * 1024)),
                           ("verification_deadline_ms", limits.get("verification_deadline_ms", 2000))):
        allocations = [member.get("allocation", {}).get(field) for member in members]
        if any(type(part) is not int or part < 0 for part in allocations) or sum(allocations) > maximum:
            raise ValueError("combined_budget_multiplied")
    expected_outcomes = case.get("member_outcomes", {})
    for member in members:
        combined_evidence.member_result(value, member["index_id"])
        if member.get("outcome") != expected_outcomes.get(member["index_id"], "complete"):
            raise ValueError("combined_member_outcome_mismatch")
    expected = {(row["index_id"], row["path"]): row for row in case["expected"]}
    returned = set()
    text_bytes = 0
    vector = value.get("publication_vector", {})
    if [row.get("index_id") for row in vector.get("members", [])] != observed:
        raise ValueError("combined_publication_member_mismatch")
    member_publications = {row["index_id"]: row.get("publication") for row in vector["members"]}
    ranked_pointers = set()
    for rank in value.get("ranking", []):
        pointer = rank.get("result_pointer")
        if not isinstance(pointer, str) or not pointer.startswith("/members/") or pointer in ranked_pointers:
            raise ValueError("combined_origin_pointer_invalid")
        ranked_pointers.add(pointer)
        record = value
        try:
            parts = pointer[1:].split("/")
            if observed[int(parts[1])] != rank["index_id"]:
                raise ValueError("combined_origin_member_mismatch")
            for token in parts:
                token = token.replace("~1", "/").replace("~0", "~")
                record = record[int(token)] if isinstance(record, list) else record[token]
            citation = record["citation"]
            origin = combined_evidence.ranked_origin(value, rank)
            key = (rank["index_id"], citation["path"])
            judged = expected[key]
            if (origin["index_id"] != key[0] or origin["project_ids"] != sorted(judged["project_ids"])
                    or origin["source_root_id"] != judged["source_root_id"]
                    or member_publications[key[0]] is None or origin.get("publication") != member_publications[key[0]]
                    or citation["content_hash"] != source_hashes[key]):
                raise ValueError("combined_origin_or_hash_mismatch")
        except (KeyError, TypeError, IndexError, ValueError) as error:
            raise ValueError("combined_origin_or_hash_mismatch") from error
        returned.add(key)
        text_bytes += len(record.get("text", "").encode("utf-8"))
    if returned != set(expected) or len(value.get("ranking", [])) > limits["limit"] or text_bytes > limits["max_bytes"]:
        raise ValueError("combined_evidence_or_budget_mismatch")
    # Preserve each member token, never treat the vector as a scalar clock.
    publications = [{"index_id": row["index_id"], "publication": row.get("publication")}
                    for row in vector["members"]]
    return {"selected_indexes": len(selected), "members": len(members),
            "complete_members": sum(member["outcome"] == "complete" for member in members),
            "failed_members": sum(member["outcome"] == "failed" for member in members),
            "unqueried_members": sum(member["outcome"] == "unqueried" for member in members),
            "ranked_records": len(value.get("ranking", [])), "distinct_sources": len(returned),
            "text_bytes": text_bytes, "partial": value.get("partial"), "truncated": value.get("truncated"),
            "is_error": envelope.get("isError") is True,
            "publication_vector_sha256": digest_bytes(canonical(publications)),
            "selected_members_only": True, "budget_allocations_bounded": True,
            "source_hashes_and_attribution_match": True,
            "unselected_native_work": None}


def load_combined_configuration(path):
    with Path(path).open("rb") as source:
        raw = source.read(65537)
    if len(raw) > 65536:
        raise ValueError("configuration_byte_limit")
    config = json.loads(raw)
    command = Path(config["command"])
    if not command.is_absolute() or not command.is_file() or command.is_symlink():
        raise ValueError("absolute_regular_launcher_required")
    if digest_bytes(command.read_bytes()) != config.get("launcher_sha256"):
        raise ValueError("launcher_identity_mismatch")
    if config.get("transport") == "https":
        modern_configuration(config, combined=True)
        selected = config["selection"]["selection"]
        if not selected.get("project_ids") or len(canonical(selected)) > 4096:
            raise ValueError("invalid_combined_session")
        return config, digest_bytes(raw)
    require_historical_stdio(config)
    args = config.get("args", [])
    if (config.get("route") not in ("installed-launcher", "isolated-approved-runtime")
            or not isinstance(args, list) or not args or args[-1] != "mcp"
            or not all(isinstance(arg, str) for arg in args) or args.count("--combined-session") != 1
            or any(option in args for option in ("--root", "--catalog-session", "--service-project",
                                                "--additional-root", "--additional-project", "--explorer-descriptor"))):
        raise ValueError("explicit_combined_scope_required")
    try:
        session = json.loads(args[args.index("--combined-session") + 1])
        if not session.get("project_ids") or len(canonical(session)) > 4096:
            raise ValueError("invalid_combined_session")
    except (IndexError, TypeError, ValueError) as error:
        raise ValueError("invalid_combined_session") from error
    return config, digest_bytes(raw)


def owned_legacy_configuration(configuration, case, source_root, timeout):
    """Resolve exactly one explicit fixture root through its owned native CLI.

    This administrative metadata read is opt-in benchmark setup, never a
    fallback for a reviewed connection. It cannot discover another control root,
    register an index, start a service, open an unselected root or infer a binding.
    """
    if os.environ.get(LEGACY_COMPARE_ENV) != "1":
        raise LegacyComparisonError("legacy_comparison_not_authorized")
    if configuration.get("transport") == "https":
        raise LegacyComparisonError("genuine_predecessor_configuration_required")
    args = configuration.get("args", [])
    if (configuration.get("route") != "isolated-approved-runtime"
            or args.count("--service-runtime-dir") != 1
            or args.count("--combined-session") != 1
            or len(args) != 5 or args[-1] != "mcp"
            or len(case["index_ids"]) != 1):
        raise LegacyComparisonError("explicit_owned_control_required")
    try:
        control = Path(args[args.index("--service-runtime-dir") + 1])
        session = json.loads(args[args.index("--combined-session") + 1])
        if not control.is_absolute() or control.is_symlink() or not control.is_dir():
            raise ValueError("control")
        control = control.resolve(strict=True)
        namespace = session["namespace"]
        if not isinstance(namespace, str) or not re.fullmatch(r"[0-9a-f]{32}", namespace):
            raise ValueError("namespace")
    except (KeyError, IndexError, TypeError, OSError, ValueError) as error:
        raise LegacyComparisonError("invalid_owned_control_selection") from error
    source_root = Path(source_root)
    if not source_root.is_absolute():
        raise LegacyComparisonError("explicit_fixture_root_required")
    deadline = time.monotonic() + timeout
    observations = []

    def metadata(method):
        remaining = deadline - time.monotonic()
        if remaining <= 0:
            raise LegacyComparisonError("legacy_metadata_deadline")
        try:
            result = subprocess.run([configuration["command"], "service", "rpc", "--runtime-dir", str(control), "--stdin"],
                input=canonical(method), stdout=subprocess.PIPE, stderr=subprocess.DEVNULL,
                cwd=configuration.get("cwd"), env={**os.environ, **configuration.get("env", {})},
                timeout=remaining, creationflags=subprocess.CREATE_NO_WINDOW if os.name == "nt" else 0)
        except subprocess.TimeoutExpired as error:
            raise LegacyComparisonError("legacy_metadata_deadline") from error
        except OSError as error:
            raise LegacyComparisonError("legacy_metadata_transport_unavailable") from error
        if result.returncode or len(result.stdout) > 65536:
            raise LegacyComparisonError("legacy_metadata_transport_unavailable")
        try:
            value = json.loads(result.stdout)
        except (TypeError, ValueError) as error:
            raise LegacyComparisonError("legacy_metadata_invalid_response") from error
        if not isinstance(value, dict) or value.get("status") in ("failure", "incompatible"):
            raise LegacyComparisonError("legacy_metadata_refused")
        observations.append(digest_bytes(canonical(value)))
        return value

    epoch = metadata("request_epoch")
    if (epoch.get("status") != "request_epoch" or epoch.get("namespace") != namespace
            or type(epoch.get("epoch")) is not int or not 0 <= epoch["epoch"] <= (1 << 63) - 1):
        raise LegacyComparisonError("legacy_metadata_authority_changed")
    after, revision, seen, matching = None, None, set(), []
    for _ in range(33):
        response = metadata({"projects": {"after": after, "revision": revision, "limit": 32}})
        page = response.get("page")
        if response.get("status") != "projects" or not isinstance(page, dict):
            raise LegacyComparisonError("legacy_metadata_invalid_page")
        observed_revision = page.get("revision")
        if type(observed_revision) is not int or not 0 <= observed_revision <= (1 << 63) - 1:
            raise LegacyComparisonError("legacy_metadata_invalid_page")
        if revision is not None and observed_revision != revision:
            raise LegacyComparisonError("legacy_metadata_authority_changed")
        revision = observed_revision
        projects = page.get("projects")
        if not isinstance(projects, list) or len(projects) > 32:
            raise LegacyComparisonError("legacy_metadata_invalid_page")
        previous = after or ""
        for project in projects:
            registration = project.get("registration") if isinstance(project, dict) else None
            if not isinstance(registration, dict):
                raise LegacyComparisonError("legacy_metadata_invalid_registration")
            binding = registration.get("binding")
            root, data_root, collection = (registration.get(key) for key in ("root", "data_root", "collection"))
            if (not isinstance(binding, str) or not re.fullmatch(r"[0-9a-f]{64}", binding)
                    or binding in seen or binding <= previous
                    or any(not isinstance(path, str) or not path or len(path.encode("utf-8")) > 4096
                           or not Path(path).is_absolute() for path in (root, data_root))
                    or not isinstance(collection, str) or not re.fullmatch(r"[A-Za-z0-9_.-]{1,128}", collection)
                    or collection in (".", "..")):
                raise LegacyComparisonError("legacy_metadata_invalid_registration")
            previous = binding
            seen.add(binding)
            if len(seen) > 1024:
                raise LegacyComparisonError("legacy_metadata_registration_limit")
            # Both roots are native canonical identities from the explicit
            # scenario/registration. Never resolve another registration's path
            # or guess an alias, collection or machine-wide default.
            if root == str(source_root):
                matching.append(registration)
        cursor = page.get("after")
        if cursor is None:
            break
        if not projects or cursor != previous or cursor == after:
            raise LegacyComparisonError("legacy_metadata_invalid_cursor")
        after = cursor
    else:
        raise LegacyComparisonError("legacy_metadata_registration_limit")
    if len(matching) != 1:
        raise LegacyComparisonError("legacy_registration_ambiguous" if matching else "legacy_registration_missing")
    registration = matching[0]
    eligibility = metadata({"catalog_legacy_binding": {"project": registration["binding"], "expected_catalog_revision": None}})
    observed = eligibility.get("observation", {})
    choice = observed.get("eligibility", {}) if isinstance(observed, dict) else {}
    if (eligibility.get("status") != "catalog_legacy_binding" or observed.get("schema") != 1
            or observed.get("namespace") != namespace or observed.get("project") != registration["binding"]):
        raise LegacyComparisonError("legacy_metadata_authority_changed")
    if choice.get("kind") not in ("authority_zero", "imported_snapshot"):
        raise LegacyComparisonError("legacy_compatibility_snapshot_unavailable")
    if canonical(choice.get("registration")) != canonical(registration):
        raise LegacyComparisonError("legacy_metadata_authority_changed")
    if metadata("request_epoch") != epoch:
        raise LegacyComparisonError("legacy_metadata_authority_changed")
    result = {**configuration, "args": ["--service-runtime-dir", str(control), "--service-project", registration["binding"], "mcp"]}
    return result, {"metadata_requests": len(observations), "metadata_observations_sha256": digest_bytes(canonical(observations)),
                    "registry_revision": revision, "registration_sha256": digest_bytes(canonical(registration)),
                    "namespace_sha256": digest_bytes(namespace.encode("ascii")), "unique_explicit_root_match": True,
                    "legacy_eligibility": choice["kind"]}


def one_index_evidence(envelope, combined=None, index_id=None):
    if not isinstance(envelope, dict):
        raise LegacyComparisonError("one_index_read_invalid")
    value = body(envelope) if "structuredContent" in envelope or "content" in envelope else envelope
    if envelope.get("isError") is True or value.get("error"):
        raise LegacyComparisonError("one_index_read_failed")
    for _ in range(4):
        if isinstance(value.get("hits"), list):
            publication = value.get("publication")
            if combined is not None:
                # Hits remain inline. Only the logical publication is needed
                # for this evidence comparison; retain the full raw reply too.
                publication = combined_evidence.publication(combined, publication, index_id)
            if not isinstance(publication, dict) or not isinstance(publication.get("token"), str) or not publication["token"]:
                raise LegacyComparisonError("one_index_publication_missing")
            return value["hits"], publication
        value = value.get("evidence")
        if not isinstance(value, dict):
            break
    raise LegacyComparisonError("one_index_hit_records_missing")


def compare_one_index(combined_envelope, legacy_envelope, case):
    value = body(combined_envelope)
    members = value.get("members", [])
    if len(members) != 1 or members[0].get("index_id") != case["index_ids"][0]:
        raise LegacyComparisonError("one_index_member_required")
    combined_hits, publication = one_index_evidence(combined_evidence.member_result(value, members[0]["index_id"]), value, members[0]["index_id"])
    legacy_hits, legacy_publication = one_index_evidence(legacy_envelope)
    if canonical(combined_hits) != canonical(legacy_hits):
        raise LegacyComparisonError("one_index_hit_records_differ")
    if canonical(publication) != canonical(legacy_publication):
        raise LegacyComparisonError("one_index_publication_differ")
    return {"full_hit_records_equal": True, "exact_member_publication_equal": True,
            "hit_records_sha256": digest_bytes(canonical(combined_hits)), "hit_records": len(combined_hits)}


def combined(args):
    """Read-only replay against an explicitly prepared original fixture catalog.

    Scenario JSON is private input: indexes name source_root directories and
    their exact inventory; cases freeze selected Projects/indexes and expected
    index/path/source-root/Project attribution before candidate execution.
    This command performs no catalog mutation, registration, scan or repair.
    """
    with args.scenario.open("rb") as source:
        raw = source.read(1024 * 1024 + 1)
    if len(raw) > 1024 * 1024:
        raise ValueError("combined_scenario_byte_limit")
    scenario = json.loads(raw)
    if scenario.get("schema") != COMBINED_SCHEMA or not 1 <= len(scenario["indexes"]) <= 32:
        raise ValueError("combined_scenario_schema")
    sources, hashes = {}, {}
    for index in scenario["indexes"]:
        identity = index["index_id"]
        if identity in sources or not re.fullmatch(r"idx_[0-9a-f]{32}", identity):
            raise ValueError("combined_scenario_index")
        requested_root = Path(index["source_root"])
        if requested_root.is_symlink():
            raise ValueError("combined_source_identity_mismatch")
        root = requested_root.resolve(strict=True)
        if combined_source_inventory(root) != index["inventory"]:
            raise ValueError("combined_source_identity_mismatch")
        sources[identity] = (root, index["inventory"])
        hashes.update({(identity, row["path"]): row["sha256"] for row in index["inventory"]})
    cases = scenario["cases"]
    if not isinstance(cases, list) or not 1 <= len(cases) <= 32:
        raise ValueError("combined_scenario_case_limit")
    for case in cases:
        ids = case["index_ids"]
        if (len(ids) not in (1, 2, 4, 32) or len(set(ids)) != len(ids)
                or not set(ids) <= set(sources) or not case["project_ids"]
                or not case["expected"] or not isinstance(case["arguments"], dict)
                or case["arguments"].get("mode") != "lexical"
                or not 1 <= case["arguments"].get("limit", 0) <= 100
                or not 1 <= case["arguments"].get("max_bytes", 0) <= 65536
                or any((row["index_id"], row["path"]) not in hashes for row in case["expected"])):
            raise ValueError("combined_scenario_case")
    configuration, config_digest = load_combined_configuration(args.configuration)
    output = args.output.resolve()
    for root, _ in sources.values():
        if output == root or root in output.parents or output in root.parents:
            raise ValueError("output_source_overlap")
    output.mkdir(parents=True, exist_ok=False)
    report = {"schema": COMBINED_SCHEMA, "scenario_sha256": digest_bytes(raw),
              "wire_basis": "canonical_json_body_bytes_excluding_http_headers_tls" if configuration.get("transport") == "https" else "newline_delimited_json_bytes",
              "configuration_sha256": config_digest, "launcher_sha256": configuration["launcher_sha256"],
              "route": configuration["route"], "expected_version": configuration["expected_version"],
              "samples_per_case": args.samples, "cases": [], "passed": False,
              "limitations": ["Frontend and shared service/OS caches are not flushed.",
                              "No legacy equivalent exists for a multi-index query; absolute observed latency is reported.",
                              "No process RSS, CPU, physical I/O or unselected-work counters are available from this transport.",
                              "Selected-member output checks supplement, but do not replace, actual native work/isolation tests."]}
    group_session = None
    compare_legacy = os.environ.get(LEGACY_COMPARE_ENV) == "1"
    retain_raw = os.environ.get(COMBINED_RAW_ENV) == "1"
    raw_stream = None
    legacy_configuration, comparison_session = None, None
    if compare_legacy:
        report["legacy_one_index"] = {
            "enabled": True, "passed": False, "trials": [],
            "runner_sha256": digest_bytes(Path(__file__).read_bytes()),
            "comparison_basis": "Exact native hit records (including all citation/text/truncation fields) and member publication only; complete outcomes retained separately.",
            "limitations": [
                "The legacy connection authorizes the full fixture index; the reviewed connection authorizes only its frozen Project scope. Scope, coverage/readiness, grant/admission and transport observations retain their separate meanings in the raw replies.",
                "Query, index, literal prefix, result/text budgets and verification choice match; full response-body equality and equal authority are not asserted.",
                "Each measured combined call precedes its paired legacy call; service and OS caches are shared and unflushed. This fixed order can favor the second route.",
                "No legacy-equivalence claim applies to more than one selected index."]}
    try:
        if retain_raw:
            raw_stream = (output / "raw.jsonl").open("x", encoding="utf-8")
            report["raw_outcomes"] = "raw.jsonl"
            report["raw_basis"] = "Every received combined tools/call outcome, including retries and failures; no response is invented after transport loss. Full original-fixture source evidence may be retained only through this explicit opt-in."
        if compare_legacy:
            case = cases[0]
            if (len(case["index_ids"]) != 1 or 1 not in args.fanouts
                    or not case["arguments"].get("path_prefix")
                    or any(key in case["arguments"] for key in ("project", "project_ids", "index_ids", "member_availability", "origin_publication"))):
                raise LegacyComparisonError("explicit_one_index_case_required")
            legacy_configuration, metadata = owned_legacy_configuration(
                configuration, case, sources[case["index_ids"][0]][0], args.timeout)
            legacy_directory = output / "legacy-one-index"
            legacy_directory.mkdir()
            (legacy_directory / "configuration.json").write_text(json.dumps(legacy_configuration, indent=2) + "\n", encoding="utf-8")
            report["legacy_one_index"].update(metadata, configuration_sha256=digest_bytes(canonical(legacy_configuration)),
                                              raw_outcomes="legacy-one-index/raw.jsonl")
        for case in cases:
            if len(case["index_ids"]) not in args.fanouts:
                continue
            group = {"selected_indexes": len(case["index_ids"]), "selection_sha256": digest_bytes(canonical(case)), "trials": []}
            report["cases"].append(group)
            matched_case = compare_legacy and case is cases[0]
            group_session = (RetainedSession if matched_case or retain_raw else Session)(configuration_for_case(configuration, case), args.timeout, prime_status=False)
            comparison_session = RetainedSession(legacy_configuration, args.timeout, prime_status=False) if matched_case else None
            with group_session as session, (comparison_session if matched_case else nullcontext()) as legacy, \
                    ((output / "legacy-one-index/raw.jsonl").open("x", encoding="utf-8") if matched_case else nullcontext()) as retained:
                arguments = {**case["arguments"], "project_ids": case["project_ids"], "index_ids": case["index_ids"], "response_mode": "full"}
                for sample in range(args.samples):
                    started = time.perf_counter()
                    trial = {"sample": sample, "validated": False}
                    group["trials"].append(trial)
                    try:
                        envelope, metadata = session.tool(("search", arguments))
                        if retained is not None or raw_stream is not None:
                            retain_query_replies(retained, session, sample, "combined", raw_stream, len(case["index_ids"]))
                        trial.update(elapsed_ms=metadata["elapsed_ms"], retries=metadata["retries"],
                                     attempts=metadata["attempts"], observation=observation(envelope),
                                     response_bytes=sum(attempt["response_bytes"] for attempt in metadata["attempts"]))
                        trial.update(combined_observation(envelope, case, hashes), validated=True)
                        if legacy is not None:
                            paired = {"sample": sample, "validated": False}
                            report["legacy_one_index"]["trials"].append(paired)
                            at = time.perf_counter()
                            try:
                                legacy_envelope, legacy_metadata = legacy.tool(("search", {**case["arguments"], "response_mode": "full"}))
                                retain_query_replies(retained, legacy, sample, "legacy")
                                paired.update(elapsed_ms=legacy_metadata["elapsed_ms"], retries=legacy_metadata["retries"],
                                              attempts=legacy_metadata["attempts"], observation=observation(legacy_envelope),
                                              response_bytes=sum(attempt["response_bytes"] for attempt in legacy_metadata["attempts"]))
                                paired.update(compare_one_index(envelope, legacy_envelope, case), validated=True)
                                trial["legacy_one_index_sample"] = sample
                            except (OSError, ValueError, RuntimeError, TimeoutError) as error:
                                paired.setdefault("elapsed_ms", (time.perf_counter() - at) * 1000)
                                paired["failure_class"] = type(error).__name__
                                if isinstance(error, LegacyComparisonError):
                                    paired["failure_code"] = error.code
                                if "attempts" not in paired:
                                    paired["submission"] = "uncertain"
                                raise
                    except (OSError, ValueError, RuntimeError, TimeoutError) as error:
                        if retained is not None or raw_stream is not None:
                            retain_query_replies(retained, session, sample, "combined", raw_stream, len(case["index_ids"]))
                        if retained is not None:
                            retain_query_replies(retained, legacy, sample, "legacy")
                        trial.setdefault("elapsed_ms", (time.perf_counter() - started) * 1000)
                        trial.update(failure_class=type(error).__name__)
                        if "attempts" not in trial:
                            trial["submission"] = "uncertain"
                        raise
            group["frontend_cleanup"] = session.cleanup
            group["latency_ms"] = distribution([row["elapsed_ms"] for row in group["trials"]])
            if matched_case:
                comparison = report["legacy_one_index"]
                comparison["frontend_cleanup"] = legacy.cleanup
                comparison["combined_latency_ms"] = group["latency_ms"]
                comparison["legacy_latency_ms"] = distribution([row["elapsed_ms"] for row in comparison["trials"]])
                comparison["errors"] = sum(attempt.get("is_error") is True for row in comparison["trials"] for attempt in row.get("attempts", []))
                comparison["retries"] = sum(row.get("retries", 0) for row in comparison["trials"])
                comparison["passed"] = (len(comparison["trials"]) == args.samples and all(row["validated"] for row in comparison["trials"])
                                        and cleanup_ok(comparison["frontend_cleanup"]))
        if not report["cases"]:
            raise ValueError("combined_fanout_cases_missing")
        report["passed"] = all(cleanup_ok(group["frontend_cleanup"]) for group in report["cases"])
        if compare_legacy:
            report["passed"] = report["passed"] and report["legacy_one_index"]["passed"]
    except (OSError, ValueError, RuntimeError, TimeoutError) as error:
        report["failure_class"] = type(error).__name__
        if isinstance(error, LegacyComparisonError):
            report["failure_code"] = error.code
    finally:
        if raw_stream is not None:
            raw_stream.close()
        if report["cases"] and group_session is not None:
            report["cases"][-1].setdefault("frontend_cleanup", getattr(group_session, "cleanup", {"observed": False}))
        try:
            report["source_unchanged"] = all(combined_source_inventory(root) == inventory for root, inventory in sources.values())
        except (OSError, ValueError):
            report["source_unchanged"] = False
        report["passed"] = report["passed"] and report["source_unchanged"]
        if compare_legacy:
            report["legacy_one_index"]["source_unchanged"] = report["source_unchanged"]
            report["legacy_one_index"]["passed"] = report["legacy_one_index"]["passed"] and report["source_unchanged"]
            if comparison_session is not None:
                report["legacy_one_index"].setdefault("frontend_cleanup", getattr(comparison_session, "cleanup", {"observed": False}))
        (output / "report.json").write_text(json.dumps(report, indent=2) + "\n", encoding="utf-8")
    return report["passed"]


def phase_measurement(path, mode):
    """Admit the unchanged90-trial/810-call single-index phase workload.

    Recompute distributions from retained trials rather than trusting summary
    fields. A successful report is bounded evidence, not binary provenance.
    """
    with Path(path).open("rb") as source:
        raw = source.read(16 * 1024 * 1024 + 1)
    if len(raw) > 16 * 1024 * 1024:
        raise ValueError("phase_report_byte_limit")
    report = json.loads(raw)
    if (report.get("schema") != SCHEMA or report.get("status") != "complete"
            or report.get("source_unchanged") is not True
            or report.get("route") != "isolated-approved-runtime"
            or report.get("fixture_sha256") != "5c0ddbe2197afd930d64d6f3dbef844e3247fb52ebb2b35175ca9597397da256"):
        raise ValueError("phase_report_contract")
    groups = report.get("cohorts", [])
    if len(groups) != 3 or {group.get("connection") for group in groups} != {row[0] for row in COHORTS}:
        raise ValueError("phase_cohort_contract")
    cohorts, receipts = {}, 0
    for group in groups:
        if (group.get("concurrency") != 1 or len(group.get("trials", [])) != 30
                or group.get("coverage_complete") is not True or group.get("cleanup_clean") is not True):
            raise ValueError("phase_sample_or_coverage_contract")
        for trial in group["trials"]:
            if (trial.get("coverage", {}).get("complete") is not True
                    or trial.get("publication_consistent") is not True or len(trial.get("calls", [])) != 9):
                raise ValueError("phase_trial_contract")
            for call in trial["calls"]:
                attempts = call.get("attempts", [])
                if call.get("retries") != 0 or len(attempts) != 1 or attempts[0].get("is_error") is not False:
                    raise ValueError("phase_error_or_retry")
                receipt = attempts[0].get("source_verification")
                if mode == "off" and receipt is not None:
                    raise ValueError("phase_off_source_checks")
                if mode == "report" and receipt is not None:
                    if receipt.get("mode") != "report" or receipt.get("status") != "verified":
                        raise ValueError("phase_report_verification")
                    receipts += 1
        metrics = {}
        for metric in ("task_ms", "task_with_connection_ms"):
            values = [trial[metric] for trial in group["trials"]]
            if any(type(value) not in (int, float) or not math.isfinite(value) or value < 0 for value in values):
                raise ValueError("phase_invalid_latency")
            metrics[metric] = distribution(values)
        cohorts[group["connection"]] = metrics
    if receipts != (720 if mode == "report" else 0):
        raise ValueError("phase_verification_count")
    return cohorts, {"sha256": digest_bytes(raw), "trials": 90, "calls": 810,
                     "errors": 0, "retries": 0, "verified_receipts": receipts}


def phase_compare(candidate, reference, label):
    """Frozen P0/P4/P5 thresholds; no caller-adjustable allowances."""
    if set(candidate) != set(reference):
        raise ValueError("phase_reference_cohorts")
    gates = []
    for name, group in candidate.items():
        for metric in ("task_ms", "task_with_connection_ms"):
            for quantile, fraction, allowance in (("median", .10, 25), ("p95", .15, 50)):
                observed, expected = group[metric][quantile], reference[name][metric][quantile]
                if any(type(number) not in (int, float) or not math.isfinite(number) or number < 0
                       for number in (observed, expected)):
                    raise ValueError("phase_invalid_latency")
                bound = expected + max(expected * fraction, allowance)
                gates.append({"reference": label, "connection": name, "metric": metric,
                              "quantile": quantile, "observed": observed, "reference_value": expected,
                              "bound": bound, "passed": observed <= bound})
    return gates


def phase_gates(args):
    """One canonical evaluator for the unchanged84 legacy phase comparisons."""
    reports, evidence = {}, {}
    for label in ("candidate_off", "candidate_report", "archive_off", "archive_report",
                  "control_off", "control_report", "preceding_off", "preceding_report"):
        reports[label], evidence[label] = phase_measurement(getattr(args, label), label.rsplit("_", 1)[1])
    frozen_raw = args.baseline.read_bytes()
    frozen = json.loads(frozen_raw)
    if frozen.get("schema") != "airk.v190-baseline/1" or frozen.get("mcp_fixture_sha256") != "5c0ddbe2197afd930d64d6f3dbef844e3247fb52ebb2b35175ca9597397da256":
        raise ValueError("phase_frozen_baseline_identity")
    frozen_groups = {group["connection"]: group for group in frozen["mcp_cohorts"] if group["concurrency"] == 1}
    if len(frozen_groups) != 3 or any(group.get("samples") != 30 for group in frozen_groups.values()):
        raise ValueError("phase_frozen_sample_contract")
    gates = phase_compare(reports["candidate_off"], frozen_groups, "frozen_archived")
    for mode in ("off", "report"):
        for reference in ("archive", "control", "preceding"):
            gates.extend(phase_compare(reports[f"candidate_{mode}"], reports[f"{reference}_{mode}"], f"{reference}_{mode}"))
    if len(gates) != 84:
        raise ValueError("phase_gate_count")
    result = {"schema": "airk.legacy-phase-gates/1", "passed": all(gate["passed"] for gate in gates),
              "limits_changed": False, "baseline_sha256": digest_bytes(frozen_raw),
              "limits": "median +max(10%,25ms); p95 +max(15%,50ms)", "evidence": evidence, "gates": gates}
    args.output.parent.mkdir(parents=True, exist_ok=True)
    with args.output.open("x", encoding="utf-8") as destination:
        destination.write(json.dumps(result, indent=2) + "\n")
    print(json.dumps({"passed": result["passed"], "passed_gates": sum(gate["passed"] for gate in gates), "total_gates": len(gates)}))
    return result["passed"]


def load_configuration(path, source):
    with Path(path).open("rb") as stream:
        raw = stream.read(65537)
    if len(raw) > 65536:
        raise ValueError("configuration_byte_limit")
    config = json.loads(raw)
    command = Path(config["command"])
    if not command.is_absolute() or not command.is_file() or command.is_symlink():
        raise ValueError("absolute_regular_launcher_required")
    if digest_bytes(command.read_bytes()) != config.get("launcher_sha256"):
        raise ValueError("launcher_identity_mismatch")
    if config.get("transport") == "https":
        modern_configuration(config, combined=False)
        try:
            same_source = Path(config["source_root"]).is_absolute() and Path(config["source_root"]).resolve(strict=True).samefile(source)
        except (KeyError, OSError, ValueError):
            same_source = False
        if not same_source:
            raise ValueError("configuration_fixture_scope_mismatch")
        return config, digest_bytes(raw)
    require_historical_stdio(config)
    if config.get("route") not in ("installed-launcher", "isolated-approved-runtime"):
        raise ValueError("explicit_route_required")
    args = config.get("args", [])
    if not isinstance(args, list) or not args or args[-1] != "mcp" or not all(isinstance(v, str) for v in args):
        raise ValueError("explicit_mcp_arguments_required")
    # No re-use of a normal project by accident. A fresh fixture registration is
    # prepared externally; this script only proves its requested source binding.
    try:
        same_source = (args.count("--root") == 1
                       and Path(args[args.index("--root")+1]).resolve(strict=True).samefile(source))
    except (OSError, IndexError):
        same_source = False
    if not same_source:
        raise ValueError("configuration_fixture_scope_mismatch")
    if any(option in args for option in ("--additional-root", "--additional-project", "--explorer-descriptor")):
        raise ValueError("single_native_fixture_only")
    return config, digest_bytes(raw)


def run(args):
    source, manifest = load_fixture(args.fixture)
    configuration, config_digest = load_configuration(args.configuration, source)
    override = getattr(args, "verification", None)
    if override is not None:
        configuration = {**configuration, "verification_override": override}
    output = Path(args.output).resolve()
    if output == source or source in output.parents or output in source.parents:
        raise ValueError("output_source_overlap")
    output.mkdir(parents=True, exist_ok=False)
    report = {"schema": SCHEMA, "label": args.label, "route": configuration["route"],
        "wire_basis": "canonical_json_body_bytes_excluding_http_headers_tls" if configuration.get("transport") == "https" else "newline_delimited_json_bytes",
        "configuration_sha256": config_digest, "launcher_sha256": configuration["launcher_sha256"],
        "expected_version": configuration["expected_version"], "fixture_sha256": manifest["fixture_sha256"],
        "effective_argument_overrides": {} if override is None else {"evidence_tools": {"verification": override}},
        "cohorts": [], "limitations": ["Cold means frontend connection only; service/model/filesystem caches are not flushed.",
            "This deterministic read-only replay does not measure a model's reasoning or proprietary client UI.",
            "Setup/queue/execution split requires separately correlated content-free native diagnostics; end-to-end times are measured here.",
            "A launcher hash and protocol version do not authenticate selected running image/source identity; the caller must independently verify and retain approved runtime provenance.",
            "The replay phase installs no runtime and registers/indexes no project; owned setup, when selected, prepares only disposable fixtures."]}
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
                    cleanup_clean=all(cleanup_ok(trial.get("frontend_cleanup", group.get("frontend_cleanup", {}))) for trial in trials))
        report["status"] = "complete"
    except (OSError, ValueError, RuntimeError, TimeoutError) as error:
        # Class name only: arbitrary exception messages can contain local paths.
        report.update(status="failed", failure_class=type(error).__name__)
    finally:
        report["source_unchanged"] = source_inventory(source) == manifest["inventory"]
        (output / "report.json").write_text(json.dumps(report, indent=2) + "\n", encoding="utf-8")
    return report["status"] == "complete" and report["source_unchanged"] and all(c.get("coverage_complete", False) and c.get("cleanup_clean", False) for c in report["cohorts"])


def owned_configuration(service, name, runtime):
    """One actual owned producer; modern clients receive inert reviewed selectors."""
    config = {"command": str(service.binary),
              "launcher_sha256": digest_bytes(service.binary.read_bytes()),
              "expected_version": json.loads((runtime / "airk-runtime.json").read_text())["version"],
              "route": "isolated-approved-runtime", "cwd": str(runtime)}
    if service.single_image:
        selection = {"kind": "scoped", "selection": json.loads(json.dumps(service.scopes[name]))}
        connection = service.enroll_http(selection, uuid.uuid4().hex, "unknown")
        roster_path = service.control / "http-1" / ("benchmark-connections-" + uuid.uuid4().hex + ".json")
        descriptor = os.open(roster_path, os.O_WRONLY | os.O_CREAT | os.O_EXCL, 0o600)
        with os.fdopen(descriptor, "wb") as stream:
            stream.write(canonical({"schema": 1, "client": "unknown", "connections": [connection]}))
        config.update({"schema": 2, "transport": "https", "args": [],
                       "control": str(service.control), "source_root": service.projects[name]["root"],
                       "selection": selection, "connections_file": str(roster_path)})
        modern_configuration(config, combined=False)
    else:
        config["args"] = [*service.arguments(name), "mcp"]
        require_historical_stdio(config)
    return config


def owned(args):
    """Prepare only disposable sources/state, then reuse the unchanged replay owner."""
    spec = importlib.util.spec_from_file_location("investigation_clients", Path(__file__).with_name("check-service-clients.py"))
    clients = importlib.util.module_from_spec(spec)
    spec.loader.exec_module(clients)
    runtime = clients.runtime_root(args.runtime_dir)
    output = Path(args.output).resolve()
    output.mkdir(parents=True, exist_ok=False)
    result = False
    with clients.owned_client_fixture() as temporary:
        owner = Path(temporary).resolve()
        fixture = owner / "fixture"
        create_fixture(fixture)
        root, _ = load_fixture(fixture)
        state = owner / "index-data"
        state.mkdir()
        with clients.Fixture(runtime, owner) as service:
            service.register("investigation", root, state)
            service.configure("investigation", {"operation": "vectors", "enabled": False})
            service.configure("investigation", {"operation": "detail", "level": "agent_lean"})
            service.configure("investigation", {"operation": "usage", "mode": "disabled"})
            service.job("investigation", "index")
            service.published_status("investigation", time.monotonic() + 90)
            config = owned_configuration(service, "investigation", runtime)
            configuration = output / "configuration.json"
            configuration.write_text(json.dumps(config, indent=2) + "\n", encoding="utf-8")
            replay_args = argparse.Namespace(**vars(args))
            replay_args.fixture, replay_args.configuration = fixture, configuration
            replay_args.output = output / "replay"
            result = run(replay_args)
        # Written only after the existing service owner has drained successfully.
        (output / "lifecycle.json").write_text(json.dumps({"owned_service_drained": True,
            "normal_installation_changed": False, "replay_passed": result}) + "\n", encoding="utf-8")
    return result


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    commands = parser.add_subparsers(dest="operation", required=True)
    fixture = commands.add_parser("fixture")
    fixture.add_argument("--destination", type=Path, required=True)
    audit = commands.add_parser("audit-record")
    audit.add_argument("--record", type=Path, required=True)
    combined_parser = commands.add_parser("combined", help="Read-only selected-index replay on an explicitly prepared original catalog")
    combined_parser.add_argument("--configuration", type=Path, required=True)
    combined_parser.add_argument("--scenario", type=Path, required=True)
    combined_parser.add_argument("--output", type=Path, required=True)
    combined_parser.add_argument("--samples", type=int, default=30)
    combined_parser.add_argument("--timeout", type=float, default=30)
    combined_parser.add_argument("--fanouts", type=int, nargs="+", choices=(1, 2, 4, 32), default=[1, 2, 4, 32])
    gates_parser = commands.add_parser("phase-gates", help="Evaluate unchanged84 frozen/archive/control/preceding legacy latency gates")
    gates_parser.add_argument("--baseline", type=Path, default=Path("docs/research/baseline-v190.json"))
    for name in ("candidate-off", "candidate-report", "archive-off", "archive-report",
                 "control-off", "control-report", "preceding-off", "preceding-report"):
        gates_parser.add_argument("--" + name, type=Path, required=True)
    gates_parser.add_argument("--output", type=Path, required=True)
    for operation in ("run", "owned"):
        replay = commands.add_parser(operation)
        if operation == "run":
            replay.add_argument("--configuration", type=Path, required=True)
            replay.add_argument("--fixture", type=Path, required=True)
        else:
            replay.add_argument("--runtime-dir", type=Path, required=True,
                                help="Exact staged pair; create/drain only a private disposable service, not an installation")
        replay.add_argument("--output", type=Path, required=True)
        replay.add_argument("--label", choices=("baseline", "candidate"), required=True)
        replay.add_argument("--samples", type=int, default=30)
        replay.add_argument("--concurrency", type=int, nargs="+", choices=(1, 4, 6, 8, 16, 32, 64), default=[1, 4, 6, 8])
        replay.add_argument("--timeout", type=float, default=30)
        replay.add_argument("--verification", choices=("off", "report", "require"),
                            help="Explicit evidence-read override, recorded separately from frozen fixture identity; omission measures runtime defaults")
        replay.add_argument("--record", type=Path, help="Optional retained investigation to reconcile without editing it")
    args = parser.parse_args()
    if args.operation == "fixture":
        result = create_fixture(args.destination)
        print(json.dumps({"fixture_sha256": result["fixture_sha256"], "files": len(result["inventory"])}))
    elif args.operation == "audit-record":
        print(json.dumps(reconcile_record(args.record), indent=2))
    elif args.operation == "phase-gates":
        raise SystemExit(0 if phase_gates(args) else 1)
    else:
        if not 1 <= args.samples <= 1000 or not 0 < args.timeout <= 120:
            parser.error("samples must be 1..1000 and timeout >0..120 seconds")
        execute = combined if args.operation == "combined" else owned if args.operation == "owned" else run
        raise SystemExit(0 if execute(args) else 1)


if __name__ == "__main__":
    main()
