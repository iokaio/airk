# SPDX-License-Identifier: Apache-2.0
"""Ignored P6f native handler/transport fixture; NOT installer qualification.

No production switch is changed. A stopped, disposable native schema-11 registry
is backed up, then given the exact compiled schema-12/authority-1 DDL. The native
registry's strict physical-shape validator must accept it before any test runs.
This proves neither fresh installation nor predecessor upgrade/readiness.

Run only in the lead's serialized native qualification queue. No credential is
written to a cases/report file. Private worker grants travel on child stdin.
"""
import argparse
import copy
from contextlib import closing
import hashlib
import importlib.util
import json
import os
from pathlib import Path
import queue
import re
import socket
import sqlite3
import struct
import subprocess
import sys
import tempfile
import threading
import time
import uuid

REPO = Path(__file__).resolve().parents[2]
SPEC = importlib.util.spec_from_file_location("scoped_clients", REPO / "scripts/check-service-clients.py")
CLIENTS = importlib.util.module_from_spec(SPEC)
SPEC.loader.exec_module(CLIENTS)
PROJECTION_SPEC = importlib.util.spec_from_file_location("p6f_catalog_projection", Path(__file__).with_name("p6f_catalog_projection.py"))
CATALOG_PROJECTION = importlib.util.module_from_spec(PROJECTION_SPEC)
PROJECTION_SPEC.loader.exec_module(CATALOG_PROJECTION)
PRIVATE_SPEC = importlib.util.spec_from_file_location("p6f_private_probe", Path(__file__).with_name("p6f_private_probe.py"))
PRIVATE_PROBE = importlib.util.module_from_spec(PRIVATE_SPEC)
PRIVATE_SPEC.loader.exec_module(PRIVATE_PROBE)
KIND = "compiled_schema12_authority1_handler_transport_only"
MAX_FRAME = 4 * 1024 * 1024
MAX_CALLS = 240
VOLATILE = {
    ("request_id",),
    ("observed_unix_ms",),
    ("cached",),
    ("cache_age_ms",),
    ("freshness", "observed_unix_ms"),
    ("discovery_readiness", "observed_unix_ms"),
    ("semantic_readiness", "observed_unix_ms"),
    ("discovery_observation", "observed_unix_ms"),
    ("indexing", "observed_unix_ms"),
    ("status_observation", "observed_unix_ms"),
    ("status_observation", "cache_age_ms"),
    ("status_observation", "cached"),
}
BASE_POINTERS = ["/" + "/".join(path) for path in sorted(VOLATILE)]
WORKBENCH_POINTERS = BASE_POINTERS + ["/steps/0/elapsed_ms"] + ["/steps/0/result" + pointer for pointer in BASE_POINTERS]


class FixtureFailure(AssertionError):
    def __init__(self, report):
        self.report = report
        super().__init__(report["failure"]["message"])


def encoded(value):
    return json.dumps(value, ensure_ascii=False, separators=(",", ":")).encode("utf-8")


def first_json_difference(native, mcp, pointer=""):
    """Strict JSON difference. No transport, documentation or schema rewrites."""
    if type(native) is not type(mcp):
        return {"pointer": pointer or "/", "native": native, "mcp": mcp, "kind": "type"}
    if isinstance(native, dict):
        for key in sorted(set(native) | set(mcp)):
            child = pointer + "/" + key.replace("~", "~0").replace("/", "~1")
            if key not in native:
                return {"pointer": child, "mcp": mcp[key], "kind": "missing_native_field"}
            if key not in mcp:
                return {"pointer": child, "native": native[key], "kind": "missing_mcp_field"}
            found = first_json_difference(native[key], mcp[key], child)
            if found is not None:
                return found
    elif isinstance(native, list):
        if len(native) != len(mcp):
            return {"pointer": pointer, "native_length": len(native), "mcp_length": len(mcp), "kind": "array_length"}
        for number, (left, right) in enumerate(zip(native, mcp)):
            found = first_json_difference(left, right, pointer + "/" + str(number))
            if found is not None:
                return found
    elif native != mcp:
        return {"pointer": pointer or "/", "native": native, "mcp": mcp, "kind": "value"}
    return None


def input_catalog_differences(native_result, mcp_result):
    """Exact schema diagnostics; full raw catalogs are retained separately.

    Only the existing native airk_ tool-name prefix maps to its public alias.
    Project selectors, public text and extra help inputs are NOT normalized.
    """
    native_tools, mcp_tools = native_result["tools"], mcp_result["tools"]
    assert len(native_tools) <= 128 and len(mcp_tools) <= 128, "Input catalog tool bound exceeded"
    native_names = [tool["name"].removeprefix("airk_") for tool in native_tools]
    mcp_names = [tool["name"] for tool in mcp_tools]
    assert len(set(native_names)) == len(native_names) and len(set(mcp_names)) == len(mcp_names), "Ambiguous duplicate catalog tool identity"
    native_map = dict(zip(native_names, native_tools))
    mcp_map = dict(zip(mcp_names, mcp_tools))
    differences = []
    for name in sorted(set(native_map) | set(mcp_map)):
        if name not in native_map or name not in mcp_map:
            difference = {"tool": name, "pointer": "/tools", "kind": "missing_native_tool" if name not in native_map else "missing_mcp_tool"}
        else:
            found = first_json_difference(native_map[name]["inputSchema"], mcp_map[name]["inputSchema"], "/inputSchema")
            if found is None:
                continue
            difference = {"tool": name, "native_tool": native_map[name]["name"], "mcp_tool": mcp_map[name]["name"], **found}
        differences.append(difference)
        if len(differences) == 64:
            break
    return differences


def digest(path):
    with Path(path).open("rb") as stream:
        return hashlib.file_digest(stream, "sha256").hexdigest()


def framed(domain, *fields):
    value = hashlib.sha256(b"AIRK-DIGEST-1\0")
    for field in (domain, *fields):
        value.update(len(field).to_bytes(8, "big"))
        value.update(field)
    return value.hexdigest()


def normalize(value, path=()):
    """Exact complete bodies; only the enumerated transport/time/cache fields.

    Scope, publication, sources, proof, errors, budgets, continuations and all
    unknown fields are retained. A step result is another complete native body.
    """
    if isinstance(value, list):
        return [normalize(item, (*path, str(n))) for n, item in enumerate(value)]
    if not isinstance(value, dict):
        return value
    if "scope_observation" in path:
        return copy.deepcopy(value)
    result = {}
    for key, item in value.items():
        child = (*path, key)
        if child in VOLATILE:
            continue
        if len(child) == 3 and child[0] == "steps" and child[1].isdigit() and key == "elapsed_ms":
            continue
        if len(child) == 3 and child[0] == "steps" and child[1].isdigit() and key == "result":
            result[key] = normalize(item)
        else:
            result[key] = normalize(item, child)
    return result


def body(envelope):
    return CLIENTS.tool_value(envelope)


def comparable_envelope(envelope):
    result = copy.deepcopy(envelope)
    for block in result.get("content", []):
        if block.get("type") == "text":
            block["text"] = normalize(json.loads(block["text"]))
    if isinstance(result.get("structuredContent"), dict):
        result["structuredContent"] = normalize(result["structuredContent"])
    return result


def retain_read_comparison(report, label, actual, expected=None, mode="body"):
    """Retain public read payloads BEFORE comparison, never IPC grant replies.

    Complete raw payloads have a 96-entry/16MiB aggregate budget. Only the
    existing explicit volatile paths are excluded for equality; Scope, unknown
    native fields, errors and provenance remain exact. Oversized diagnostics
    retain their exact pointer; complete values remain in the raw payloads.
    scope_only explicitly tests just the mandatory observation and still
    retains both complete bodies; it does not claim whole-result equivalence.
    """
    capture = report.setdefault("read_comparisons", {"max_comparisons": 96, "max_raw_payload_bytes": 16 * 1024 * 1024,
                                                     "raw_payload_bytes": 0, "entries": []})
    raw_bytes = len(encoded(actual)) + (len(encoded(expected)) if mode != "observe" else 0)
    assert len(capture["entries"]) < capture["max_comparisons"], "Full read comparison count budget exceeded"
    assert capture["raw_payload_bytes"] + raw_bytes <= capture["max_raw_payload_bytes"], "Full raw read comparison byte budget exceeded"
    entry = {"id": label, "comparison": mode, "actual": copy.deepcopy(actual), "raw_payload_bytes": raw_bytes}
    if mode != "observe":
        entry["expected"] = copy.deepcopy(expected)
    capture["entries"].append(entry)
    capture["raw_payload_bytes"] += raw_bytes
    if mode == "observe":
        entry["observed"] = True
        return
    if mode == "envelope":
        left, right = comparable_envelope(expected), comparable_envelope(actual)
    elif mode == "body":
        left, right = normalize(expected), normalize(actual)
    elif mode == "raw":
        left, right = expected, actual
    elif mode == "scope_only":
        left = {"scope_observation": expected["scope_observation"]} if "scope_observation" in expected else {}
        right = {"scope_observation": actual["scope_observation"]} if "scope_observation" in actual else {}
    else:
        raise AssertionError("Unknown read comparison mode")
    found = first_json_difference(left, right)
    entry["matched"] = found is None
    if found is not None:
        diagnostic = {key.replace("native", "expected").replace("mcp", "actual"): value for key, value in found.items()}
        if len(encoded(diagnostic)) > 2048:
            diagnostic = {"pointer": found["pointer"], "kind": found["kind"], "diagnostic_values_omitted": True, "complete_raw_payloads_retained": True}
        entry["first_difference"] = diagnostic
        raise AssertionError(label + ": complete read comparison failed: " + json.dumps(diagnostic, ensure_ascii=False))


def require_scope(value, selection):
    observation = value.get("scope_observation")
    assert isinstance(observation, dict), "Missing mandatory scope observation"
    assert observation["schema"] == 1 and observation["qualifier"] == "selected_subset"
    assert observation["grant"] == {"id": selection["grant_id"], "revision": selection["expected_grant_revision"]}
    assert len(observation["members"]) == 1, "P6f must retain exactly one selected index"
    member = observation["members"][0]
    assert member["index_id"] == selection["index_id"]
    assert sorted(item["project_id"] for item in member["memberships"]) == sorted(selection["project_ids"])
    assert len({item["project_id"] for item in member["memberships"]}) == len(member["memberships"])
    return observation


def safe_json(stdout):
    assert len(stdout.encode("utf-8")) <= MAX_FRAME, "Fixture response exceeds bound"
    return json.loads(stdout)


def json_documents(stdout):
    """Read actual tools-shell JSON following Result:/MCP error:, no summaries."""
    decoder, values = json.JSONDecoder(), []
    for match in re.finditer(r"(?:Result:|MCP error:)\s*", stdout):
        value, _ = decoder.raw_decode(stdout[match.end():])
        values.append(value)
    return values


def schema_objects(db):
    rows = db.execute("SELECT type,name,tbl_name,coalesce(sql,'') FROM sqlite_schema ORDER BY type,name").fetchall()
    assert len(rows) <= 512
    result = [{"kind": kind, "name": name, "table": table, "sql": sql.replace("\r\n", "\n")}
              for kind, name, table, sql in rows]
    assert sum(len(item["sql"].encode()) for item in result) <= 512 * 1024
    return result


def seed_handler_catalog(fixture):
    assert fixture.child is None, "Never mutate a running native registry"
    database = fixture.control / "service-1.4.sqlite3"
    assert database.resolve(strict=True).parent == fixture.control.resolve(strict=True)
    assert not database.is_symlink()
    schema_dir = REPO / "crates/airk-store/src"
    inputs = [schema_dir / "service_catalog.rs", schema_dir / "service_catalog_schema.sql",
              schema_dir / "service_catalog_authority_v1.sql", schema_dir / "service_registry_schema11.sql"]
    before = {str(path.relative_to(REPO)): digest(path) for path in inputs}
    with closing(sqlite3.connect(database)) as db, db:
        assert db.execute("PRAGMA user_version").fetchone()[0] == 11
        assert db.execute("SELECT count(*) FROM projects").fetchone()[0] == 0
        namespace = db.execute("SELECT incarnation FROM service_meta WHERE id=1").fetchone()[0]
        original_ddl = framed(b"airk/registry-ddl/1", encoded(schema_objects(db)))
        backup = fixture.owner / "actual-empty-schema11-backup.sqlite3"
        with closing(sqlite3.connect(backup)) as target, target:
            db.backup(target)
        actual_backup = digest(backup)
        db.executescript((schema_dir / "service_catalog_schema.sql").read_text(encoding="utf-8"))
        for table in ("index_definitions", "query_projects", "index_contexts", "index_access_grants",
                      "index_context_imports", "index_operation_receipts"):
            db.execute(f"CREATE TRIGGER {table}_capacity BEFORE INSERT ON {table} WHEN (SELECT count(*) FROM {table})>=1024 BEGIN SELECT RAISE(ABORT,'index catalog capacity exceeded'); END;")
        for table, condition, maximum in (
            ("index_sources", "index_id=NEW.index_id", 17),
            ("index_source_aliases", "source_id=NEW.source_id", 16),
            ("project_index_memberships", "project_id=NEW.project_id", 32),
            ("project_index_scopes", "project_id=NEW.project_id AND index_id=NEW.index_id", 128),
            ("context_projects", "context_id=NEW.context_id", 32),
            ("context_default_indexes", "context_id=NEW.context_id", 32),
            ("index_access_memberships", "grant_id=NEW.grant_id", 1024),
            ("index_access_scopes", "grant_id=NEW.grant_id AND project_id=NEW.project_id AND index_id=NEW.index_id", 128),
        ):
            db.execute(f"CREATE TRIGGER {table}_capacity BEFORE INSERT ON {table} WHEN (SELECT count(*) FROM {table} WHERE {condition})>={maximum} BEGIN SELECT RAISE(ABORT,'index catalog group capacity exceeded'); END;")
        for column in ("project_id", "index_id"):
            db.execute(f"CREATE TRIGGER index_access_{column}_capacity BEFORE INSERT ON index_access_memberships WHEN NOT EXISTS(SELECT 1 FROM index_access_memberships WHERE grant_id=NEW.grant_id AND {column}=NEW.{column}) AND (SELECT count(DISTINCT {column}) FROM index_access_memberships WHERE grant_id=NEW.grant_id)>=32 BEGIN SELECT RAISE(ABORT,'grant selection capacity exceeded'); END;")
        for (table,) in db.execute("SELECT name FROM sqlite_schema WHERE type='table' AND name<>'index_catalog_meta' AND name NOT LIKE 'sqlite_%'").fetchall():
            assert re.fullmatch("[a-z_]+", table)
            for action in ("INSERT", "UPDATE", "DELETE"):
                db.execute(f"CREATE TRIGGER catalog_write_{table}_{action.lower()} AFTER {action} ON {table} WHEN EXISTS(SELECT 1 FROM index_catalog_meta WHERE id=1 AND activated=1 AND accepted_new_writes=0) BEGIN UPDATE index_catalog_meta SET accepted_new_writes=1 WHERE id=1; END;")
        db.execute("INSERT INTO index_catalog_meta VALUES(1,1,0,0,0,0,?,?,?)",
                   (uuid.uuid4().hex, original_ddl, actual_backup))
        db.executescript((schema_dir / "service_catalog_authority_v1.sql").read_text(encoding="utf-8"))
        db.execute("UPDATE index_catalog_meta SET activated=1,authority_contract=1 WHERE id=1")
        db.execute("PRAGMA user_version=12")
        assert db.execute("PRAGMA integrity_check").fetchall() == [("ok",)]
    return {"schema_inputs": before, "namespace": namespace, "real_before_schema_digest": original_ddl,
            "real_before_backup_sha256": actual_backup, "qualification": KIND,
            "not_qualified": ["installer", "fresh_genesis", "P5_predecessor_upgrade", "native_readiness", "installed_editor", "cross_platform"]}


class ScopedFixture(CLIENTS.Fixture):
    def __init__(self, *args, **kwargs):
        super().__init__(*args, **kwargs)
        self.calls = 0
        self.deadline = time.monotonic() + 420
        self.environment.update(HTTP_PROXY="http://127.0.0.1:1", HTTPS_PROXY="http://127.0.0.1:1",
                                ALL_PROXY="http://127.0.0.1:1")

    def run(self, *args, **kwargs):
        self.calls += 1
        assert self.calls <= MAX_CALLS, "Native fixture call budget exceeded"
        kwargs["timeout"] = CLIENTS.remaining(self.deadline, kwargs.get("timeout") or self.command_timeout)
        return super().run(*args, **kwargs)

    def stop_owned(self):
        assert self.child is not None
        self.run("service", "shutdown", "--runtime-dir", self.control)
        self.child.wait(timeout=45)
        assert self.child.returncode == 0, "Owned service failed shutdown"
        self.child = None

    def close(self):
        # Cleanup has its own finite budget even after the observation budget
        # expires. Only the Popen child created by this fixture may be killed.
        try:
            if self.child is not None:
                if self.child.poll() is None:
                    try:
                        CLIENTS.Fixture.run(self, "service", "shutdown", "--runtime-dir", self.control,
                                            check=False, timeout=10)
                    except (subprocess.TimeoutExpired, OSError):
                        pass
                try:
                    self.child.wait(timeout=45)
                except subprocess.TimeoutExpired:
                    self.child.kill()
                    self.child.wait(timeout=15)
                    raise AssertionError("Owned service required forced cleanup; drain qualification failed")
        finally:
            self.log.close()

    def exact_rpc(self, method):
        # No uncertain mutation is resubmitted. Successful response or fail the
        # fixture and retain the original request in this process for diagnosis.
        return safe_json(self.run("service", "rpc", "--runtime-dir", self.control,
                                  "--stdin", input=json.dumps(method)).stdout)

    def epoch(self):
        value = self.exact_rpc("request_epoch")
        return {"namespace": value["namespace"], "epoch": value["epoch"], "nonce": uuid.uuid4().hex}

    def host(self, context_key):
        result = self.exact_rpc({"catalog_host_context": {"key": context_key, "expected_catalog_revision": None}})
        assert result["status"] == "catalog_host_context", result
        return result["descriptor"]

    def apply(self, operation, caller, context_key):
        descriptor = self.host(context_key)
        request = {"caller": caller, "key": self.epoch(), "expected_catalog_revision": descriptor["catalog_revision"], "operation": operation}
        preview = self.exact_rpc({"catalog_preview": {"request": request}})
        assert preview["status"] == "catalog_preview", preview
        result = self.exact_rpc({"catalog_apply": {"request": request, "preview_digest": preview["preview_digest"]}})
        assert result["status"] == "catalog_applied", result
        return result["receipt"]

    def detail(self, entity):
        result = self.exact_rpc({"catalog_detail": {"entity": entity, "expected_catalog_revision": None}})
        assert result["status"] == "catalog_detail", result
        item = result["detail"]["item"]
        assert item is not None and item["kind"] == entity["kind"], result
        return item["value"]

    def setup(self, root, extra, data, context_key):
        descriptor = self.host(context_key)
        selection = {"key": self.epoch(), "expected_catalog_revision": descriptor["catalog_revision"],
                     "name": "Shared indexed library", "primary_root": str(root), "data_root": str(data),
                     "collection": "scoped-shared", "additional_roots": [{"alias": "external", "path": str(extra)}],
                     "setup_watch": False}
        result = self.exact_rpc({"catalog_index_setup_draft": {"selection": selection}})
        assert result["status"] == "catalog_index_setup_draft", result
        draft = result["draft"]
        result = self.exact_rpc({"catalog_index_setup_apply": {"draft": draft}})
        assert result["status"] == "catalog_index_setup_applied", result
        receipt = result["receipt"]
        assert receipt["stage"] == "ready", receipt
        registration = receipt["request"]["registration"]
        self.projects["shared"] = registration
        return draft["request"]["caller"], receipt

    def selection_args(self, selection):
        return ["--service-runtime-dir", str(self.control), "--catalog-session", json.dumps(selection)]

    def call_selected(self, selection, tool, arguments):
        result = self.run(*self.selection_args(selection), "tools", "call", tool,
                          "--args", json.dumps(arguments), check=False)
        assert result.returncode in (0, 1), "Native tools-call process failed"
        value = safe_json(result.stdout)
        assert result.returncode == 0 or value.get("isError") is True
        return value

    def alias_selected(self, selection, alias):
        result = self.run(*self.selection_args(selection), *alias, check=False)
        assert result.returncode in (0, 1), "Native CLI alias process failed"
        return safe_json(result.stdout)

    def shell_selected(self, selection, tool, arguments):
        result = self.run(*self.selection_args(selection), "tools", "shell",
                          input="call " + tool + " " + json.dumps(arguments) + "\nexit\n", check=False)
        assert result.returncode == 0, result.stderr[-2048:]
        values = json_documents(result.stdout)
        assert len(values) == 1, "tools-shell did not preserve one full result"
        return values[0]


class Mcp:
    def __init__(self, fixture, selection, workbench=False):
        self.fixture, self.selection, self.next_id = fixture, selection, 10
        self.child = subprocess.Popen([str(fixture.binary), *fixture.selection_args(selection), "mcp"],
                                      stdin=subprocess.PIPE, stdout=subprocess.PIPE, stderr=subprocess.PIPE,
                                      text=True, encoding="utf-8", cwd=fixture.owner / "decoy-cwd", env=fixture.environment,
                                      creationflags=getattr(subprocess, "CREATE_NO_WINDOW", 0))
        self.messages = queue.Queue()
        threading.Thread(target=lambda: [self.messages.put(line) for line in self.child.stdout], daemon=True).start()
        try:
            self.send({"jsonrpc": "2.0", "id": 1, "method": "initialize", "params": {
                "protocolVersion": "2025-11-25", "capabilities": {},
                "clientInfo": {"name": "airk-workbench" if workbench else "airk-native-check", "version": "fixture"}}})
            assert "result" in self.receive(1)
            self.send({"jsonrpc": "2.0", "method": "notifications/initialized"})
        except BaseException:
            self.child.stdin.close()
            try:
                self.child.wait(timeout=5)
            except subprocess.TimeoutExpired:
                self.child.kill()
                self.child.wait(timeout=5)
            raise

    def send(self, value):
        self.child.stdin.write(json.dumps(value) + "\n")
        self.child.stdin.flush()

    def receive(self, identifier):
        deadline = min(self.fixture.deadline, time.monotonic() + 45)
        for _ in range(32):
            item = safe_json(self.messages.get(timeout=CLIENTS.remaining(deadline, 45)))
            if item.get("id") == identifier:
                return item
        raise AssertionError("MCP response bound exceeded")

    def request(self, method, params):
        self.fixture.calls += 1
        assert self.fixture.calls <= MAX_CALLS
        self.next_id += 1
        self.send({"jsonrpc": "2.0", "id": self.next_id, "method": method, "params": params})
        response = self.receive(self.next_id)
        assert "error" not in response, response
        return response["result"]

    def call(self, tool, arguments):
        return self.request("tools/call", {"name": tool.removeprefix("airk_"), "arguments": arguments})

    def close(self):
        if self.child.poll() is None:
            self.child.stdin.close()
            try:
                self.child.wait(timeout=15)
            except subprocess.TimeoutExpired:
                self.child.kill()
                self.child.wait(timeout=5)
                raise AssertionError("Owned MCP client did not exit")
        assert self.child.returncode == 0, "Owned MCP client failed"


def current_sid():
    return PRIVATE_PROBE.process_sid()


def worker_endpoint(control, role):
    assert role == "control" or re.fullmatch(r"q-[0-9a-f]{32}", role)
    root = str(Path(control).resolve(strict=True))
    if os.name == "nt":
        if not root.startswith("\\\\?\\"):
            root = "\\\\?\\UNC\\" + root[2:] if root.startswith("\\\\") else "\\\\?\\" + root
        identity = hashlib.sha256(current_sid().encode() + root.encode() + role.encode()).hexdigest()
        return "\\\\.\\pipe\\airk-1.4-" + identity
    identity = hashlib.sha256(str(os.geteuid()).encode() + b"\0" + os.fsencode(root)).hexdigest()
    candidates = []
    if os.environ.get("XDG_RUNTIME_DIR"):
        candidates.append(Path(os.environ["XDG_RUNTIME_DIR"]) / ("ak-" + identity[:16]))
    candidates.append(Path("/tmp") / ("ak-" + identity[:16]))
    for candidate in candidates:
        receipt = candidate / "identity"
        if candidate.is_dir() and not candidate.is_symlink() and receipt.is_file() and not receipt.is_symlink():
            assert candidate.stat().st_uid == os.geteuid() and candidate.stat().st_mode & 0o077 == 0
            metadata = receipt.stat()
            assert metadata.st_uid == os.geteuid() and metadata.st_nlink == 1 and metadata.st_size == 64 and metadata.st_mode & 0o077 == 0
            with receipt.open("rb") as stream:
                if stream.read(65) == identity.encode():
                    return str(candidate / (role + ".sock"))
    raise AssertionError("Exact owned native worker endpoint absent")


def worker_exchange(payload):
    """Actual credential issuance/redemption stays wholly in this child."""
    return PRIVATE_PROBE.exchange(payload, worker_endpoint)


def private_query(fixture, selection, queries, *, forge_purpose=False, expire=False):
    if not hasattr(fixture, "private_broker_health"):
        health = fixture.exact_rpc("health")
        assert health["status"] == "health"
        expected = health["observation"]
        assert expected["process_id"] == fixture.child.pid and expected["executable_sha256"] == digest(fixture.binary)
        fixture.private_broker_health = {key: expected[key] for key in ("version", "protocol", "instance", "process_id", "executable_sha256")}
    # Health + ConnectScoped + worker query/ACK observation are still counted,
    # even though issuance now correctly remains on the private native channel.
    fixture.calls += 2 + max(1, len(queries))
    assert fixture.calls <= MAX_CALLS, "Native fixture call budget exceeded"
    reply = subprocess.run([sys.executable, str(Path(__file__).resolve()), "--worker-exchange"],
                           input=json.dumps({"control": str(fixture.control), "selection": selection, "queries": queries,
                                             "expected_health": fixture.private_broker_health, "expected_backing": fixture.projects["shared"]["binding"],
                                             "forge_purpose": forge_purpose, "expire": expire}),
                           text=True, encoding="utf-8", capture_output=True, timeout=CLIENTS.remaining(fixture.deadline, 55),
                           creationflags=getattr(subprocess, "CREATE_NO_WINDOW", 0))
    assert reply.returncode == 0, "Private framing probe failed (credentials deliberately omitted)"
    value = safe_json(reply.stdout)
    capture = fixture.probe_report.setdefault("private_probe_results", {"max_probes": 32, "max_bytes": 4 * 1024 * 1024, "bytes": 0, "entries": []})
    entry = {"requested_index": selection["index_id"], "queries": queries, "forged_wire_purpose": forge_purpose,
             "expired_unredeemed": expire, "counted_native_observations": 2 + max(1, len(queries)), "result": value}
    needed = len(encoded(entry))
    assert len(capture["entries"]) < capture["max_probes"] and capture["bytes"] + needed <= capture["max_bytes"], "Sanitized private probe capture budget exceeded"
    capture["entries"].append(entry)
    capture["bytes"] += needed
    assert not value.get("probe_failed"), "Private native probe failed; static stage/class diagnostics retained without credentials"
    assert type(value["ttl_ms"]) is int and 1 <= value["ttl_ms"] <= 30000
    if not expire and not forge_purpose:
        assert value["accepted"] is True, "Private read had no actual identity-matching worker ACK"
    return value, value["ttl_ms"]


def query_frame(tool="", arguments=None, native=None):
    return {"tool": tool, "arguments": arguments or {}, "native": native}


def grant_selection(fixture, context_key, projects, index):
    host = fixture.host(context_key)
    draft = fixture.exact_rpc({"catalog_grant_draft": {"context_key": context_key, "selection": {
        "key": fixture.epoch(), "expected_catalog_revision": host["catalog_revision"],
        "project_ids": projects, "index_ids": [index]}}})
    assert draft["status"] == "catalog_grant_draft", draft
    draft = draft["draft"]
    result = fixture.exact_rpc({"catalog_apply": {"request": draft["request"], "preview_digest": draft["preview_digest"]}})
    assert result["status"] == "catalog_applied", result
    native_grant = draft["request"]["operation"]["grant"]
    return {"schema": 1, "context_key": context_key, "namespace": host["namespace"],
            "expected_catalog_revision": result["receipt"]["catalog_revision"],
            "grant_id": native_grant["id"], "expected_grant_revision": native_grant["revision"],
            "index_id": index, "project_ids": projects}


def evidence_families():
    """Explicit matching inputs, including rich representation and budgets."""
    common = {"response_mode": "full", "reply_detail": "rich", "verification": "off"}
    tail = ["--reply-detail", "rich", "--verification", "off"]
    budget = ["--limit", "8", "--max-bytes", "32768"]
    path = "public/memo0.py"
    return [
        ("files", "airk_list_files", {**common, "prefix": "", "limit": 8, "max_bytes": 32768}, ["files", "", *budget, *tail]),
        ("read", "airk_read_file", {**common, "path": path, "start_line": 1, "end_line": 3, "max_bytes": 32768},
         ["read", path, "--start-line", "1", "--end-line", "3", "--max-bytes", "32768", *tail]),
        ("info", "airk_file_info", {**common, "path": path}, ["info", path, *tail]),
        ("outline", "airk_outline", {**common, "path": path, "compact": False, "limit": 8, "max_bytes": 32768},
         ["outline", path, "--compact", "false", *budget, *tail]),
        ("symbols", "airk_symbols", {**common, "name": "beacon_public_0", "path_prefix": "public", "compact": False, "limit": 8, "max_bytes": 32768},
         ["symbol", "beacon_public_0", "--path-prefix", "public", "--compact", "false", *budget, *tail]),
        ("imports", "airk_imports", {**common, "path_prefix": "public", "compact": False}, ["imports", "public", "--compact", "false", *tail]),
        ("document_facts", "airk_document_facts", {**common, "path_prefix": "public", "compact": False, "limit": 8, "max_bytes": 32768},
         ["facts", "public", "--compact", "false", *budget, *tail]),
        ("code_facts", "airk_code_facts", {**common, "path_prefix": "public", "compact": False, "limit": 8, "max_bytes": 32768},
         ["code-facts", "--path-prefix", "public", "--compact", "false", *budget, *tail]),
        ("references", "airk_references", {**common, "name": "beacon_public_0", "path_prefix": "public", "limit": 8, "max_bytes": 32768},
         ["refs", "beacon_public_0", "--path-prefix", "public", *budget, *tail]),
        ("relationships", "airk_relationships", {**common, "symbol": "file:public/memo0.py", "direction": "outgoing", "path_prefix": "public", "hops": 1, "limit": 8, "max_bytes": 32768},
         ["relationships", "file:public/memo0.py", "--direction", "outgoing", "--path-prefix", "public", "--hops", "1", *budget, *tail]),
        ("changes", "airk_changes", {**common, "path_prefix": "public", "selectors": [{"kind": "path", "path": path}], "limit": 8, "max_bytes": 32768},
         ["changes", "--path", path, "--path-prefix", "public", *budget, *tail]),
        ("context", "airk_context", {**common, "path": path, "line": 1, "before": 0, "after": 2, "max_bytes": 32768},
         ["context", path, "1", "--before", "0", "--after", "2", "--max-bytes", "32768", *tail]),
    ]


def native_parse_requests(search_arguments):
    """Primitive plan inputs exclude the MCP presentation envelope field.

    The owning workbench reader always supplies response_mode=full and status
    when it invokes primitives. Native query/reply/verification controls remain
    unchanged; this is a supported-input distinction, not result normalization.
    The real Workbench UI builds only schema/mode/plan; its native baseline uses
    that exact wrapper and inherits full presentation from the fixture's native
    environment plus verification=off from the identical semantic plan.
    """
    step_arguments = copy.deepcopy(search_arguments)
    step_arguments.pop("response_mode", None)
    plan = {"schema": "airk.parse.plan/1", "steps": [{"id": "parity", "tool": "search", "args": step_arguments}],
            # Native execution spends one call pinning publication and one on
            # the sole search primitive; this is not a two-step/fanout plan.
            "verification": "off", "budgets": {"calls": 2, "fanout": 1, "seconds": 15, "max_bytes": 65536},
            "output": {"view": "raw", "max_bytes": 65536}}
    workbench_request = {"schema": "airk.parse.request/1", "mode": "execute", "plan": plan}
    native_request = {**copy.deepcopy(workbench_request), "response_mode": "full", "verification": "off"}
    return native_request, workbench_request


def qualify_selected_native_help(fixture, selection, mcp, native_catalog, report):
    """Native selectors only; UI query/topic_id help is not equivalent evidence."""
    schemas = {tool["name"]: tool for tool in native_catalog["tools"]}
    cases = [
        ("tools_search", {"topic": "TOOLS", "tool": "search"}, False),
        ("tools_compatibility_search", {"topic": "TOOLS", "tool": "airk_search"}, False),
        ("commands_tools_list", {"topic": "COMMANDS", "command": "tools list"}, False),
        ("conflicting_native_topic_tool", {"topic": "overview", "tool": "search"}, True),
    ]
    first_tool_help = None
    for name, selectors, is_error in cases:
        arguments = {**selectors, "max_bytes": 32768, "response_mode": "full"}
        native = fixture.call_selected(selection, "airk_help", arguments)
        retain_read_comparison(report, "native_help_" + name + "_baseline", native, mode="observe")
        assert (native.get("isError") is True) == is_error, "Native help selector returned the wrong success/error result"
        value = body(native)
        require_scope(value, selection)
        if is_error:
            assert value["error"]["code"] == "conflicting_help_selectors", "Expected a genuine native selector conflict"
        else:
            assert value["truncated"] is False and value["next_line"] is None, "Native help did not return its full untruncated document"
            if "tool" in selectors:
                assert value["tool"] == "airk_search", "Native help alias did not disclose its resolved native identity"
                schema = json.loads(value["content"])
                retain_read_comparison(report, "native_help_" + name + "_full_schema", schema, schemas["airk_search"], mode="raw")
                if first_tool_help is not None:
                    retain_read_comparison(report, "native_help_compatibility_alias_full_body", value, first_tool_help)
                first_tool_help = value
        public_reply = mcp.call("help", arguments)
        retain_read_comparison(report, "native_help_" + name + "_public_mcp", public_reply, native, mode="envelope")
        shell_reply = fixture.shell_selected(selection, "airk_help", arguments)
        retain_read_comparison(report, "native_help_" + name + "_tools_shell", shell_reply, value)
        report["cases"].append({"id": "selected_native_help_" + name, "passed": True, "expected_is_error": is_error,
                                "ui_only_help_equivalence": False, "scope": value["scope_observation"]})


def source_draft_metadata_census(fixture, index, binding):
    """Bounded read-only diagnostic census for this exact native index/backing."""
    with closing(sqlite3.connect((fixture.control / "service-1.4.sqlite3").as_uri() + "?mode=ro", uri=True)) as db:
        with db:
            sources = db.execute("SELECT id,canonical_path,owner,enabled FROM index_sources WHERE index_id=? ORDER BY id LIMIT 18", (index,)).fetchall()
            aliases = db.execute("SELECT source_id,alias FROM index_source_aliases WHERE index_id=? ORDER BY source_id,alias LIMIT 17", (index,)).fetchall()
            jobs = db.execute("SELECT id,kind,state,epoch,attempts FROM service_jobs WHERE project=? ORDER BY seq LIMIT 1025", (binding,)).fetchall()
            assert len(sources) <= 17 and len(aliases) <= 16 and len(jobs) <= 1024, "Source draft metadata census exceeded native bounds"
            return {"sources": sources, "aliases": aliases, "jobs": jobs}


def settle_accepted_source_work(fixture, index, binding, report):
    """Observe accepted work to native success before deliberately pausing it.

    No enqueue, retry, cancellation, reordering or database write is permitted.
    Status observations have their own cap inside the existing campaign budget.
    """
    deadline = min(fixture.deadline, time.monotonic() + 60)
    settlement = report["source_draft_settlement"] = {"status": "running", "index_id": index, "project": binding,
                                                      "native_observations": [], "metadata_censuses": []}
    def observe(method):
        assert len(settlement["native_observations"]) < 40, "Accepted work observation budget exceeded"
        result = safe_json(fixture.run("service", "rpc", "--runtime-dir", fixture.control, "--stdin",
                                       input=json.dumps(method), timeout=CLIENTS.remaining(deadline, 20)).stdout)
        settlement["native_observations"].append({"input": method, "response": result})
        return result
    try:
        while True:
            CLIENTS.remaining(deadline, 20)
            progress = observe({"progress": {"project": binding}})
            assert progress["status"] == "progress" and progress["project"] == binding
            assert progress["queue_truncated"] is False and progress["project_paused"] is False, "Accepted work is truncated or already paused"
            census = source_draft_metadata_census(fixture, index, binding)
            settlement["metadata_censuses"].append(census)
            candidates = {row[0] for row in census["jobs"] if row[2] != "succeeded"}
            native_jobs = list(progress["queued_jobs"])
            if progress.get("pending_job") is not None:
                native_jobs.append(progress["pending_job"])
            if progress.get("observation") is not None:
                native_jobs.append(progress["observation"]["job"])
            for job in native_jobs:
                assert job["project"] == binding, "Native progress crossed the selected backing"
                if job["state"] != "succeeded":
                    candidates.add(job["id"])
            if not candidates:
                settlement.update(status="passed", native_observation_count=len(settlement["native_observations"]),
                                  accepted_work_settled=True, fixture_job_mutations=0)
                return
            for job_id in sorted(candidates):
                observed = observe({"job": {"id": job_id}})
                assert observed["status"] == "job" and observed["job"]["id"] == job_id and observed["job"]["project"] == binding
                assert observed["job"]["state"] in ("pending", "running", "succeeded"), "Accepted native work failed or was canceled; full job outcome is retained"
            time.sleep(min(1.0, CLIENTS.remaining(deadline, 1.0)))
    except BaseException:
        settlement.update(status="failed", native_observation_count=len(settlement["native_observations"]),
                          accepted_work_settled=False, fixture_job_mutations=0)
        raise


def qualify_source_draft_validation(fixture, args, owner, source_probe, context_key, index, setup, manifest_sha, report):
    """Actual native read-only review -> unchanged capture -> compiled validator.

    This exercises no Apply/configuration/index mutation, Console transport or
    installed-editor behavior. The optional driver starts no native process.
    """
    assert next(source_probe.iterdir(), None) is None, "Owned draft root is not empty"
    host = fixture.host(context_key)
    selection = {"key": fixture.epoch(), "expected_catalog_revision": host["catalog_revision"], "index_id": index,
                 "change": {"operation": "add_root", "alias": "native_draft_probe", "path": str(source_probe)}}
    method = {"catalog_source_mutation_draft": {"selection": selection}}
    registration = setup["request"]["registration"]
    before_index = fixture.detail({"kind": "index", "id": index})
    before_census = source_draft_metadata_census(fixture, index, registration["binding"])
    bridge = report["source_draft_validation"] = {"status": "running", "coverage": "actual_native_readonly_capture_to_compiled_service_client",
                                               "actual_native_input": method, "metadata_before": before_census,
                                               "persistent_console_qualified": False, "installed_editor_qualified": False, "native_apply_qualified": False}
    response = fixture.exact_rpc(method)
    bridge["actual_native_response"] = response
    assert response["status"] == "catalog_source_mutation_draft", "Actual native source review refused; no mutation or retry is attempted"
    draft = response["draft"]
    plan = draft["request"]["operation"]["plan"]
    identities = {"registry_namespace": selection["key"]["namespace"], "native_caller": draft["request"]["caller"],
                  "backing_binding": plan["registration"]["binding"], "collection_incarnation": plan["configuration"]["incarnation"],
                  "collection_settings_digest": plan["configuration"]["settings"]}
    bridge["native_identity_observations"] = identities
    # These are nonvacuity observations about this disposable fixture, NEVER
    # product rules that reject legitimately coinciding independent identities.
    assert identities["collection_incarnation"] != identities["registry_namespace"], "Fixture did not exercise independent native collection incarnation"
    assert identities["collection_settings_digest"] != identities["backing_binding"], "Fixture did not exercise independent native settings digest"
    assert draft["request"]["key"] == selection["key"] and draft["request"]["operation"]["id"] == index
    assert not draft["preview"]["file_data_changes"]
    assert fixture.detail({"kind": "index", "id": index}) == before_index, "Read-only source review changed the native index definition"
    assert fixture.host(context_key)["catalog_revision"] == host["catalog_revision"], "Read-only source review advanced the native catalog clock"
    assert source_draft_metadata_census(fixture, index, registration["binding"]) == before_census, "Read-only source review changed sources, aliases or accepted jobs"
    assert next(source_probe.iterdir(), None) is None, "Read-only source review wrote into the selected empty root"
    capture = {"schema": 1, "fixture_kind": KIND, "coverage": bridge["coverage"], "runtime_manifest_sha256": manifest_sha,
               "input": method, "response": response, "native_identity_observations": identities}
    capture_path, result_path = owner / "source-draft-native-capture.json", owner / "source-draft-validation-results.json"
    capture_path.write_text(json.dumps(capture, ensure_ascii=False, indent=2), encoding="utf-8")
    process = subprocess.run([args.node, str(args.source_draft_driver), "--cases", str(capture_path), "--report", str(result_path)],
                             cwd=REPO, env=fixture.environment, capture_output=True, text=True,
                             timeout=CLIENTS.remaining(fixture.deadline, 20), creationflags=getattr(subprocess, "CREATE_NO_WINDOW", 0))
    if result_path.is_file():
        assert result_path.stat().st_size <= 1024 * 1024, "Source draft validator report exceeds1MiB"
        bridge["compiled_validator"] = json.loads(result_path.read_text(encoding="utf-8"))
    assert process.returncode == 0, "Actual native source capture/compiled validator bridge failed: " + process.stderr[-2048:]
    observed = bridge["compiled_validator"]
    assert observed["status"] == "passed" and observed["fixture_kind"] == KIND and observed["input_sha256"] == digest(capture_path)
    assert observed["actual_native_input"] == method and observed["actual_native_response"] == response, "Compiled validator changed the complete native capture"
    assert observed["cases"][0]["review"]["response"] == response and observed["cases"][0]["review"]["request"] == draft["request"]
    assert len(observed["cases"]) == 6 and all(case["passed"] for case in observed["cases"])
    assert observed["native_processes_started"] == 0 and observed["capture_transport_calls"] == observed["disposed_capture_transports"] == 6
    bridge.update(status="passed", coverage_qualified=True, metadata_unchanged=True, source_root_preserved_empty=True)


def run_campaign(args):
    started = time.monotonic()
    runtime = CLIENTS.runtime_root(args.runtime_dir)
    manifest = runtime / "airk-install.json"
    manifest_sha = digest(manifest)
    assert manifest_sha == args.expected_runtime_sha256, "Runtime inventory identity differs from the lead's pin"
    inventory = json.loads(manifest.read_text(encoding="utf-8"))
    assert 1 <= len(inventory["files"]) <= 4096
    for name, expected in inventory["files"].items():
        path = runtime / name
        assert path.resolve(strict=True).is_relative_to(runtime) and not path.is_symlink()
        assert digest(path) == expected, "Runtime pair/inventory bytes changed"
    report = {"schema": 1, "fixture_kind": KIND, "runtime_manifest_sha256": manifest_sha,
              "fixture_script_sha256": digest(Path(__file__)),
              "catalog_projection_source_sha256": digest(Path(CATALOG_PROJECTION.__file__)),
              "private_probe_source_sha256": digest(Path(PRIVATE_PROBE.__file__)),
              "normalization_excluded_paths": WORKBENCH_POINTERS,
              "cases": [], "workbench": {"status": "not_run"},
              "source_draft_validation": {"status": "not_run", "reason": "native source draft capture not reached" if args.source_draft_driver else "optional --source-draft-driver omitted", "coverage_qualified": False},
              "installed_editor_qualified": False,
              "installer_qualified": False, "cross_platform_qualified": False}
    with tempfile.TemporaryDirectory(prefix="airk-scoped-parity-", dir=args.fixture_parent) as temporary:
        owner = Path(temporary).resolve(strict=True)
        assert owner.parent == args.fixture_parent.resolve(strict=True) and owner.name.startswith("airk-scoped-parity-")
        root, extra, data, decoy = (owner / name for name in ("source", "external", "data", "decoy-cwd"))
        # Create the sole new source fixture before taking source baselines.
        # It remains empty and is reviewed only; no source mutation is applied.
        source_probe = owner / "draft-additional-root"
        if args.source_draft_driver:
            source_probe.mkdir()
        originals = {}
        for directory in (root, extra, decoy):
            for subset in ("public", "private"):
                (directory / subset).mkdir(parents=True)
                for number in range(2):
                    path = directory / subset / f"memo{number}.py"
                    marker = "decoyonly" if directory == decoy else "publiconly" if subset == "public" else "privateonly"
                    path.write_text(f"def beacon_{subset}_{number}():\n    # sharedneedle {marker}\n    return {number}\n", encoding="utf-8")
                    originals[path] = digest(path)
        fixture = ScopedFixture(runtime, owner, command_timeout=90, job_timeout=180)
        fixture.probe_report = report
        try:
            fixture.start()
            fixture.stop_owned()
            report["seed"] = seed_handler_catalog(fixture)
            fixture.start()  # Strict native compiled full-shape validation happens here.
            context_key = hashlib.sha256(b"owned-scoped-parity-context/1").hexdigest()
            native_context = fixture.host(context_key)
            assert native_context["namespace"] == report["seed"]["namespace"] and native_context["catalog_revision"] == 1
            report["seed"]["strict_native_catalog_open_verified"] = True
            caller, setup = fixture.setup(root, extra, data, context_key)
            index = setup["index_id"]
            roots_by_id = list(zip(setup["request"]["sources"], setup["source_ids"]))
            primary_id = next(identifier for source, identifier in roots_by_id if source["owner"])
            external_id = next(identifier for source, identifier in roots_by_id if source["aliases"] == ["external"])
            source_ids = [primary_id, external_id]
            fixture.job("shared", "index")
            if args.source_draft_driver:
                settle_accepted_source_work(fixture, index, setup["request"]["registration"]["binding"], report)
            fixture.exact_rpc({"pause": {"project": fixture.projects["shared"]["binding"], "paused": True}})
            if args.source_draft_driver:
                paused_census = source_draft_metadata_census(fixture, index, setup["request"]["registration"]["binding"])
                report["source_draft_settlement"]["paused_metadata_census"] = paused_census
                if not all(row[2] == "succeeded" for row in paused_census["jobs"]):
                    report["source_draft_settlement"].update(status="failed", pause_boundary_jobs_pending=True)
                    raise AssertionError("Accepted work appeared before pause; do not resubmit or suppress source-change Busy")
                qualify_source_draft_validation(fixture, args, owner, source_probe, context_key, index, setup, manifest_sha, report)
            public, private, full = ("prj_" + uuid.uuid4().hex for _ in range(3))
            for identifier, name in ((public, "Public subset"), (private, "Primary public and private overlap"), (full, "All source metadata")):
                fixture.apply({"operation": "create_project", "id": identifier, "name": name}, caller, context_key)
            def atom(source, path):
                return {"source_root_id": source, "kind": "subtree", "path": path}
            memberships = {
                public: [atom(source_ids[0], "public"), atom(source_ids[1], "public")],
                private: [atom(source_ids[0], "public"), atom(source_ids[0], "private")],
                full: [atom(source, "") for source in source_ids],
            }
            for project, scope in memberships.items():
                native_project = fixture.detail({"kind": "project", "id": project})
                fixture.apply({"operation": "set_membership", "membership": {"project_id": project, "index_id": index, "revision": 1, "scope": scope},
                               "expected_scope_revision": native_project["scope_revision"], "expected_membership_revision": None}, caller, context_key)
            selection = grant_selection(fixture, context_key, [public, private, full], index)
            selected = {name: {**selection, "project_ids": projects} for name, projects in
                        (("public", [public]), ("private", [private]), ("union", [public, private]), ("full", [full]))}
            # Disable the separately qualified source-verification pass so its
            # dated observations do not confuse transport equality. Off, rich,
            # lexical and all output budgets are explicit on every transport.
            search_args = {"query": "sharedneedle", "mode": "lexical", "limit": 2, "max_bytes": 32768,
                           "response_mode": "full", "reply_detail": "rich", "verification": "off"}
            cases = []
            for name in ("public", "private", "union"):
                current = selected[name]
                mcp = Mcp(fixture, current)
                try:
                    envelope = fixture.call_selected(current, "airk_search", search_args)
                    retain_read_comparison(report, name + "_search_baseline", envelope, mode="observe")
                    assert envelope.get("isError") is not True, envelope
                    expected = body(envelope)
                    require_scope(expected, current)
                    via_mcp = mcp.call("airk_search", search_args)
                    retain_read_comparison(report, name + "_search_mcp", via_mcp, envelope, mode="envelope")
                    alias = fixture.alias_selected(current, ["search", "sharedneedle", "--mode", "lexical", "--limit", "2", "--max-bytes", "32768", "--reply-detail", "rich", "--verification", "off"])
                    retain_read_comparison(report, name + "_search_cli_alias", alias, envelope, mode="envelope")
                    retain_read_comparison(report, name + "_search_tools_shell", fixture.shell_selected(current, "airk_search", search_args), expected)
                    paths = [item["citation"]["path"] for item in expected["hits"]]
                    assert len(paths) == 2, "Authorized results disappeared when scope filtering was applied after the limit"
                    assert len(paths) == len(set(paths)), "Shared index results were duplicated by Project membership"
                    assert not any("decoy" in json.dumps(item) for item in expected["hits"])
                    if name == "public":
                        assert all("/private/" not in "/" + path for path in paths), "Private prefix leaked before limit"
                    if name == "private":
                        assert all(path.startswith(("public/", "private/")) for path in paths), "External source escaped primary-only scope"
                    catalog = mcp.request("tools/list", {})
                    native_catalog = safe_json(fixture.run(*fixture.selection_args(current), "tools", "list", "--json").stdout)
                    captured_catalogs = {"selection_name": name, "cli": native_catalog, "mcp": catalog}
                    report.setdefault("input_catalogs", []).append(captured_catalogs)
                    differences = input_catalog_differences(native_catalog, catalog)
                    captured_catalogs["strict_schema_differences"] = differences
                    projection = CATALOG_PROJECTION.compare(native_catalog, catalog)
                    captured_catalogs["declared_projection"] = projection
                    assert projection["passed"], "Native/public declared catalog projection mismatch: " + json.dumps(projection["differences"][0], ensure_ascii=False)
                    report["cases"].append({"id": name + "_transport_full", "scope": expected["scope_observation"], "paths": paths, "passed": True})
                    if name == "public":
                        terse = body(fixture.call_selected(current, "airk_search", {**search_args, "response_mode": "terse"}))
                        retain_read_comparison(report, "terse_full_mandatory_scope", terse, expected, mode="scope_only")
                        require_scope(terse, current)
                        request, workbench_request = native_parse_requests(search_args)
                        report["parse_supported_inputs"] = {"native_transport_request": request, "actual_workbench_request": workbench_request,
                                                            "primitive_presentation": "Owning workbench_read injects response_mode=full/include_status=true; response_mode is not a plan-step argument.",
                                                            "workbench_presentation": "AIRK_MCP_RESPONSE_MODE=full in the fixture environment; identical plan.verification=off.",
                                                            "native_query_controls_unchanged": True}
                        workbench_mcp = Mcp(fixture, current, workbench=True)
                        try:
                            parse_envelope = workbench_mcp.call("airk_parse", request)
                            retain_read_comparison(report, "parse_mcp_baseline", parse_envelope, mode="observe")
                            assert parse_envelope.get("isError") is not True, "Native parse failed"
                            parsed = body(parse_envelope)
                            require_scope(parsed, current)
                            assert parsed.get("executed") is True, "Native parse did not execute its primitive"
                            assert isinstance(parsed.get("steps"), list) and len(parsed["steps"]) == 1, "Native parse did not complete the exact one-step plan"
                            assert parsed["steps"][0].get("status") == "complete", "Native parse skipped or failed its primitive"
                            assert isinstance(parsed["steps"][0].get("result"), dict), "Native parse did not retain the complete primitive result object"
                            retain_read_comparison(report, "parse_cli_tools_call", fixture.call_selected(current, "airk_parse", request), parse_envelope, mode="envelope")
                            retain_read_comparison(report, "parse_cli_tools_shell", fixture.shell_selected(current, "airk_parse", request), parsed)
                            retain_read_comparison(report, "parse_step_complete_native_body", parsed["steps"][0]["result"], expected)
                            actual_ui_parse = workbench_mcp.call("airk_parse", workbench_request)
                            retain_read_comparison(report, "parse_actual_ui_wrapper_full_native_result", actual_ui_parse, parse_envelope, mode="envelope")
                            workbench_expected = body(actual_ui_parse)
                        finally:
                            workbench_mcp.close()
                        cases.append({"id": "public_search", "selection": current, "tool": "airk_search", "arguments": search_args,
                                      "expected": expected, "workbench_request": workbench_request, "workbench_expected": workbench_expected,
                                      "native_transport_request": request,
                                      "direct_normalization": {"excluded_paths": BASE_POINTERS},
                                      "scopeSpec": {"command": str(fixture.binary), "root": fixture.projects["shared"]["root"],
                                                    "dataDir": fixture.projects["shared"]["data_root"], "collection": "scoped-shared", "name": "Scoped parity",
                                                    "catalogSession": current,
                                                    "service": {"installDir": str(fixture.control), "project": fixture.projects["shared"]["binding"]}},
                                      "verify": {"index_id": index, "project_ids": current["project_ids"]}})
                finally:
                    mcp.close()
            family_mcp = Mcp(fixture, selected["public"])
            try:
                for name, tool, arguments, alias in evidence_families():
                    envelope = fixture.call_selected(selected["public"], tool, arguments)
                    retain_read_comparison(report, name + "_family_baseline", envelope, mode="observe")
                    assert envelope.get("isError") is not True, (name, envelope)
                    expected = body(envelope)
                    require_scope(expected, selected["public"])
                    retain_read_comparison(report, name + "_family_mcp", family_mcp.call(tool, arguments), envelope, mode="envelope")
                    retain_read_comparison(report, name + "_family_cli_alias", fixture.alias_selected(selected["public"], alias), envelope, mode="envelope")
                    retain_read_comparison(report, name + "_family_tools_shell", fixture.shell_selected(selected["public"], tool, arguments), expected)
                    report["cases"].append({"id": name + "_canonical_read_family", "passed": True,
                                            "canonical_sha256": hashlib.sha256(encoded(normalize(expected))).hexdigest(),
                                            "scope": expected["scope_observation"]})
                qualify_selected_native_help(fixture, selected["public"], family_mcp, report["input_catalogs"][0]["cli"], report)
            finally:
                family_mcp.close()
            # Obtain a real native refusal before giving the case to the actual
            # Workbench handler driver. A protocol/transport failure has no
            # CallToolResult and must fail this preparation, never fabricate one.
            unknown_workbench = copy.deepcopy(cases[0])
            unknown_workbench["id"] = "unknown_project_native_refusal"
            unknown_workbench["selection"]["project_ids"] = ["prj_" + uuid.uuid4().hex]
            unknown_workbench["scopeSpec"]["catalogSession"] = unknown_workbench["selection"]
            unknown_workbench["verify"]["project_ids"] = unknown_workbench["selection"]["project_ids"]
            refused_mcp = Mcp(fixture, unknown_workbench["selection"], workbench=True)
            try:
                actual_refusal = refused_mcp.call("airk_parse", unknown_workbench["workbench_request"])
                retain_read_comparison(report, "workbench_unknown_project_native_refusal", actual_refusal, mode="observe")
                assert actual_refusal.get("isError") is True, "Unknown Project did not produce a native MCP refusal"
                unknown_workbench.pop("expected")
                unknown_workbench.pop("workbench_expected")
                unknown_workbench["workbench_expected_native_failure"] = actual_refusal
                cases.append(unknown_workbench)
            finally:
                refused_mcp.close()
            # Before-limits isolation across original roots, using exact source locations.
            all_public = body(fixture.call_selected(selected["public"], "airk_list_files", {"prefix": "", "limit": 16, "max_bytes": 32768, "response_mode": "full", "verification": "off"}))
            require_scope(all_public, selected["public"])
            serialized = json.dumps(all_public)
            assert "@external/public/" in serialized and "private/memo" not in serialized
            union_files = body(fixture.call_selected(selected["union"], "airk_list_files", {"prefix": "", "limit": 16, "max_bytes": 32768, "response_mode": "full", "verification": "off"}))
            require_scope(union_files, selected["union"])
            assert len(union_files["paths"]) == len(set(union_files["paths"])) == 6, "Overlapping Projects duplicated or broadened the shared index"
            private_hits = body(fixture.call_selected(selected["private"], "airk_search", {**search_args, "query": "privateonly"}))
            require_scope(private_hits, selected["private"])
            assert len(private_hits["hits"]) == 2 and all(item["citation"]["path"].startswith("private/") for item in private_hits["hits"])
            # Safe metadata retains installed scope; partial scope must refuse aggregates.
            for view in ("roots", "vector_status", "vector_selection", "extraction_policy", "scan_limits"):
                request = query_frame(native={"settings": {"view": view, "cursor": None, "max_bytes": 32768}})
                response, _ = private_query(fixture, selected["full"], [request])
                assert not response["closed"] and len(response["replies"]) == 1
                native = response["replies"][0]["result"]
                require_scope(native, selected["full"])
                assert "error" not in native, (view, native)
                refused, _ = private_query(fixture, selected["public"], [request])
                assert not refused["closed"] and len(refused["replies"]) == 1
                partial_error = refused["replies"][0]["result"]
                require_scope(partial_error, selected["public"])
                assert partial_error["error"]["code"] == "scoped_read_unavailable", (view, partial_error)
            attack = query_frame(native="usage_policy")
            scoped_admin, _ = private_query(fixture, selected["full"], [attack])
            assert not scoped_admin["closed"] and len(scoped_admin["replies"]) == 1
            require_scope(scoped_admin["replies"][0]["result"], selected["full"])
            assert scoped_admin["replies"][0]["result"]["error"]["code"] == "scoped_read_unavailable"
            forged, _ = private_query(fixture, selected["full"], [attack], forge_purpose=True)
            assert forged["closed"] and forged["accepted"] is False and not forged["replies"], "Wire purpose forged native administration"
            administrative = fixture.exact_rpc({"native_read": {"project": fixture.projects["shared"]["binding"], "request": "usage_policy"}})
            assert administrative["status"] == "native_read" and "grant" not in administrative and "token" not in administrative
            expired, ttl = private_query(fixture, selected["public"], [query_frame("airk_search", search_args)], expire=True)
            assert expired["closed"] and expired["accepted"] is False and not expired["replies"], "Expired unredeemed bearer was accepted or produced evidence"
            report["credential_expiry"] = {"ttl_ms": ttl, "native_unredeemed_grant_refused": True,
                                            "installed_scope_lease_expiry_qualified": False,
                                            "limitation": "Native installed scopes live up to300s; separate focused native expiry tests are required."}
            # Real WorkbenchView native handlers/clipboard/export driver, separate from MCP parse.
            # This precedes ALL membership changes and reviewed grant revocation
            # below. The earlier unredeemed IPC credential-expiry probe does not
            # revoke the persisted reviewed grant used by the positive case.
            if args.workbench_driver:
                manifest_path, result_path = owner / "workbench-cases.json", owner / "workbench-results.json"
                manifest_path.write_text(json.dumps({"schema": 1, "fixture_kind": KIND, "control": str(fixture.control), "runtime": str(runtime), "binary": str(fixture.binary),
                                                     "runtime_manifest_sha256": manifest_sha, "source_root": str(decoy), "cases": cases,
                                                     "normalization": {"excluded_paths": report["normalization_excluded_paths"]}}, indent=2), encoding="utf-8")
                environment = {**fixture.environment, "AIRK_TEST_BINARY": str(fixture.binary)}
                process = subprocess.run([args.node, str(args.workbench_driver), "--cases", str(manifest_path), "--report", str(result_path)],
                                         cwd=REPO, env=environment, capture_output=True, text=True, timeout=120,
                                         creationflags=getattr(subprocess, "CREATE_NO_WINDOW", 0))
                if result_path.is_file():
                    assert result_path.stat().st_size <= 64 * 1024 * 1024, "Workbench report exceeds bounded native driver budget"
                    report["workbench"] = json.loads(result_path.read_text(encoding="utf-8"))
                assert process.returncode == 0, "Actual Workbench handler driver failed: " + process.stderr[-2048:]
                workbench = report["workbench"]
                assert workbench["fixture_kind"] == KIND and len(workbench["cases"]) == len(cases)
                for expected_case, observed in zip(cases, workbench["cases"]):
                    assert observed["id"] == expected_case["id"] and not observed.get("errors")
                    if "workbench_expected_native_failure" in expected_case:
                        assert observed.get("native_body") is None
                        retain_read_comparison(report, "workbench_full_native_refusal", observed["native_failure"], expected_case["workbench_expected_native_failure"], mode="envelope")
                    else:
                        retain_read_comparison(report, "workbench_full_native_parse_body", observed["native_body"], expected_case["workbench_expected"])
                report["workbench"] = {"status": "passed", **workbench}
            # A stale metadata clock alone must not invalidate the exact saved grant.
            unrelated = "prj_" + uuid.uuid4().hex
            fixture.apply({"operation": "create_project", "id": unrelated, "name": "Unrelated metadata"}, caller, context_key)
            unrelated_definition = fixture.detail({"kind": "project", "id": unrelated})
            fixture.apply({"operation": "rename_project", "id": unrelated, "name": "Unrelated renamed", "expected_definition_revision": unrelated_definition["definition_revision"]}, caller, context_key)
            cold = body(fixture.call_selected(selected["public"], "airk_search", search_args))
            retain_read_comparison(report, "old_catalog_clock_mandatory_scope", cold, cases[0]["expected"], mode="scope_only")
            require_scope(cold, selected["public"])
            report["old_catalog_clock_cold_channel"] = True
            unknown = {**selected["public"], "project_ids": ["prj_" + uuid.uuid4().hex]}
            unknown_control = fixture.run("service", "rpc", "--runtime-dir", fixture.control, "--stdin",
                                          input=json.dumps({"connect_scoped": {"request": {**unknown, "expected_catalog_revision": fixture.host(context_key)["catalog_revision"]}}}), check=False)
            # The public metadata CLI intentionally cannot export credentials.
            # Unexpected output might be a bearer; withhold it rather than
            # writing a credential into a diagnostic report on guard failure.
            report["public_metadata_credential_surface_guard"] = {
                "exit_code": unknown_control.returncode,
                "stdout_bytes": len(unknown_control.stdout.encode("utf-8")),
                "stdout": unknown_control.stdout[:8192] if not unknown_control.stdout.strip() else None,
                "stdout_truncated": len(unknown_control.stdout) > 8192,
                "unexpected_stdout_withheld": bool(unknown_control.stdout.strip()),
                "stderr": unknown_control.stderr[-8192:], "unknown_project_proof": False,
            }
            assert unknown_control.returncode != 0 and not unknown_control.stdout.strip(), "Public metadata credential-surface guard did not refuse without output"
            assert "metadata RPC cannot expose evidence credentials" in unknown_control.stderr, "Expected the actual public metadata credential guard"
            unknown_reply = fixture.run(*fixture.selection_args(unknown), "tools", "call", "airk_search", "--args", json.dumps(search_args), check=False)
            assert unknown_reply.returncode != 0, "Unknown Project became an empty successful query"
            unknown_envelope = safe_json(unknown_reply.stdout)
            retain_read_comparison(report, "unknown_project_selected_native_refusal", unknown_envelope, mode="observe")
            assert unknown_envelope.get("isError") is True
            unknown_value = body(unknown_envelope)
            assert unknown_value["error"]["code"] == "scoped_connection_unavailable"
            requested = unknown_value["requested_scope"]
            assert requested["confirmed"] is False
            for field in ("schema", "context_key", "namespace", "grant_id", "expected_grant_revision", "index_id", "project_ids"):
                assert requested["selection"][field] == unknown[field], "Unknown-project refusal crossed the actual requested scope"
            assert not unknown_value.get("hits") and not unknown_value.get("evidence") and not unknown_value.get("paths"), "Unknown Project returned source evidence"
            project_before = fixture.detail({"kind": "project", "id": public})
            membership_before = fixture.detail({"kind": "membership", "project_id": public, "index_id": index})
            fixture.apply({"operation": "set_membership", "membership": {"project_id": public, "index_id": index, "revision": membership_before["revision"] + 1, "scope": [atom(source_ids[0], "public")]},
                           "expected_scope_revision": project_before["scope_revision"], "expected_membership_revision": membership_before["revision"]}, caller, context_key)
            stale = fixture.run(*fixture.selection_args(selected["public"]), "tools", "call", "airk_search", "--args", json.dumps(search_args), check=False)
            assert stale.returncode != 0, "Old grant silently adopted changed membership revision"
            fresh = grant_selection(fixture, context_key, [public], index)
            live = Mcp(fixture, fresh)
            try:
                assert live.call("airk_search", search_args).get("isError") is not True
                fixture.apply({"operation": "revoke_grant", "id": fresh["grant_id"], "expected_revision": fresh["expected_grant_revision"]}, caller, context_key)
                after_revoke = live.call("airk_search", search_args)
                assert after_revoke.get("isError") is True and not body(after_revoke).get("hits"), "Existing MCP session escaped grant revocation"
                report["revoked_existing_session"] = body(after_revoke)
            finally:
                live.close()
            revoked = fixture.run(*fixture.selection_args(fresh), "tools", "call", "airk_search", "--args", json.dumps(search_args), check=False)
            assert revoked.returncode != 0, "Revoked grant retained evidence authority"
            report["membership_revision_refused"] = report["revoked_grant_refused"] = report["unknown_project_refused"] = True
            assert all(digest(path) == before for path, before in originals.items()), "Source fixture bytes changed"
            if args.source_draft_driver:
                assert next(source_probe.iterdir(), None) is None, "Reviewed empty source fixture was changed"
            assert all(digest(REPO / path) == before for path, before in report["seed"]["schema_inputs"].items()), "Fixture's schema source inputs changed during run"
            assert digest(manifest) == manifest_sha
            assert digest(Path(__file__)) == report["fixture_script_sha256"], "Fixture source changed during qualification"
            assert digest(Path(CATALOG_PROJECTION.__file__)) == report["catalog_projection_source_sha256"], "Catalog projection comparator changed during qualification"
            assert digest(Path(PRIVATE_PROBE.__file__)) == report["private_probe_source_sha256"], "Private native probe changed during qualification"
            report.update(source_bytes_preserved=True, native_calls=fixture.calls, elapsed_seconds=round(time.monotonic() - started, 3),
                          passed=True, workbench_handler_qualified=report["workbench"]["status"] == "passed",
                          source_draft_validator_qualified=report["source_draft_validation"]["status"] == "passed")
        except BaseException as error:
            if report["source_draft_validation"]["status"] == "running":
                report["source_draft_validation"].update(status="failed", coverage_qualified=False)
            report.update(passed=False, native_calls=fixture.calls,
                          elapsed_seconds=round(time.monotonic() - started, 3),
                          failure={"class": type(error).__name__, "message": str(error)[:2048]},
                          source_bytes_preserved=all(path.is_file() and digest(path) == before for path, before in originals.items()))
            raise FixtureFailure(report) from error
        finally:
            fixture.close()
    return report


def main():
    if sys.argv[1:] == ["--worker-exchange"]:
        print(json.dumps(worker_exchange(json.load(sys.stdin))))
        return
    if sys.argv[1:] == ["--self-test"]:
        import unittest

        class Contracts(unittest.TestCase):
            def test_normalization_never_discards_scope_or_unknown_source_fields(self):
                left = {"request_id": "first", "freshness": {"observed_unix_ms": 1, "state": "idle"},
                        "scope_observation": {"fingerprint": "first", "observed_unix_ms": 1},
                        "source": {"request_id": "preserved"}, "error": {"retryable": False}, "budget": 9}
                right = copy.deepcopy(left)
                right["request_id"] = "second"
                right["freshness"]["observed_unix_ms"] = 2
                self.assertEqual(normalize(left), normalize(right))
                right["scope_observation"]["fingerprint"] = "second"
                self.assertNotEqual(normalize(left), normalize(right))
                self.assertEqual(normalize(left)["source"]["request_id"], "preserved")

            def test_outer_step_timing_is_distinct_from_the_complete_nested_native_body(self):
                value = {"steps": [{"elapsed_ms": 1, "result": {"request_id": "transient", "scope_observation": {"grant": "retained"}, "coverage": {"elapsed_ms": 7}}}]}
                normalized = normalize(value)
                self.assertNotIn("elapsed_ms", normalized["steps"][0])
                self.assertEqual(normalized["steps"][0]["result"], {"scope_observation": {"grant": "retained"}, "coverage": {"elapsed_ms": 7}})

            def test_tools_shell_parser_keeps_full_error_and_success_objects(self):
                output = 'AIRK tools\nairk> Result:\n{"scope_observation":{"schema":1},"proof":{"all":true}}\nairk> MCP error:\n{"error":{"code":"refused"},"scope_observation":{"schema":1}}\nairk> '
                values = json_documents(output)
                self.assertEqual(len(values), 2)
                self.assertEqual(values[0]["proof"], {"all": True})
                self.assertEqual(values[1]["error"], {"code": "refused"})

            def test_full_envelope_retains_errors_and_structured_content(self):
                value = {"content": [{"type": "text", "text": '{"request_id":"transient","scope_observation":{"id":"original"}}'}],
                         "isError": True, "structuredContent": {"scope_observation": {"id": "original"}, "budget": 17}}
                actual = comparable_envelope(value)
                self.assertTrue(actual["isError"])
                self.assertEqual(actual["structuredContent"]["budget"], 17)
                self.assertEqual(actual["content"][0]["text"]["scope_observation"], {"id": "original"})

            def test_failed_read_comparison_retains_full_scope_and_unknown_fields_before_raise(self):
                expected = {"scope_observation": {"grant": {"revision": 1}}, "unknown": {"payload": [1, 2]}}
                actual = copy.deepcopy(expected)
                actual["scope_observation"]["grant"]["revision"] = 2
                report = {}
                with self.assertRaises(AssertionError):
                    retain_read_comparison(report, "scope_drift", actual, expected)
                captured = report["read_comparisons"]["entries"][0]
                self.assertEqual(captured["actual"], actual)
                self.assertEqual(captured["expected"], expected)
                self.assertEqual(captured["first_difference"]["pointer"], "/scope_observation/grant/revision")
                self.assertFalse(captured["matched"])

            def test_read_capture_count_budget_refuses_without_discarding_prior_payloads(self):
                report = {}
                for number in range(96):
                    retain_read_comparison(report, str(number), {"unknown": number}, mode="observe")
                with self.assertRaises(AssertionError):
                    retain_read_comparison(report, "overflow", {"unknown": "preserve_budget"}, mode="observe")
                self.assertEqual(len(report["read_comparisons"]["entries"]), 96)
                self.assertEqual(report["read_comparisons"]["entries"][-1]["actual"], {"unknown": 95})

            def test_parse_plan_omits_only_transport_presentation_and_preserves_query_controls(self):
                arguments = {"query": "sharedneedle", "mode": "lexical", "limit": 2, "max_bytes": 32768,
                             "reply_detail": "rich", "verification": "off", "response_mode": "full"}
                original = copy.deepcopy(arguments)
                native, workbench = native_parse_requests(arguments)
                self.assertEqual(arguments, original)
                self.assertEqual(native["response_mode"], "full")
                self.assertEqual(native["verification"], "off")
                self.assertEqual(native["plan"]["verification"], "off")
                # One native publication pin plus one primitive execution.
                self.assertEqual(native["plan"]["budgets"], {"calls": 2, "fanout": 1, "seconds": 15, "max_bytes": 65536})
                self.assertEqual(workbench["plan"], native["plan"])
                self.assertEqual(set(workbench), {"schema", "mode", "plan"})
                self.assertEqual(native["plan"]["steps"][0]["args"], {key: value for key, value in original.items() if key != "response_mode"})

            def test_private_peer_decisions_refuse_wrong_native_user_or_owned_server_pid(self):
                self.assertTrue(PRIVATE_PROBE.require_peer("native-user", "native-user", 42, 42)["expected_pid_verified"])
                for actual_user, actual_pid in (("foreign-user", 42), ("native-user", 43)):
                    with self.assertRaises(PRIVATE_PROBE.PeerRejected):
                        PRIVATE_PROBE.require_peer(actual_user, "native-user", actual_pid, 42)

            def test_private_probe_output_never_serializes_bearer_or_calls_timeout_a_closure(self):
                token = "a" * 64
                refused = PRIVATE_PROBE.safe_result({"accepted": True, "replies": [{"unexpected": token}]}, [token], True, "worker_query")
                self.assertTrue(refused["probe_failed"])
                self.assertFalse(refused["closed"])
                self.assertTrue(refused["accepted"])
                self.assertNotIn(token, json.dumps(refused))
                stream = object.__new__(PRIVATE_PROBE.NativeStream)
                class TimeoutConnection:
                    def read(self, _length):
                        raise TimeoutError()
                    def recv(self, _length):
                        raise TimeoutError()
                stream.connection = TimeoutConnection()
                with self.assertRaises(TimeoutError):
                    stream.read()
                class TruncatedConnection:
                    parts = [b"\x00\x00", b""]
                    def read(self, _length):
                        return self.parts.pop(0)
                    def recv(self, _length):
                        return self.parts.pop(0)
                stream.connection = TruncatedConnection()
                with self.assertRaises(PRIVATE_PROBE.FrameInvalid):
                    stream.read()

        suite = unittest.defaultTestLoader.loadTestsFromTestCase(Contracts)
        if not unittest.TextTestRunner(verbosity=2).run(suite).wasSuccessful():
            raise SystemExit(1)
        return
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("--runtime-dir", type=Path, required=True)
    parser.add_argument("--expected-runtime-sha256", required=True)
    parser.add_argument("--fixture-parent", type=Path, default=REPO / "target")
    parser.add_argument("--workbench-driver", type=Path)
    parser.add_argument("--source-draft-driver", type=Path)
    parser.add_argument("--node", default="node")
    parser.add_argument("--report", type=Path, required=True)
    args = parser.parse_args()
    assert re.fullmatch("[0-9a-f]{64}", args.expected_runtime_sha256)
    assert args.fixture_parent.resolve(strict=True).is_dir()
    try:
        report = run_campaign(args)
    except FixtureFailure as error:
        report = error.report
    except Exception as error:
        report = {"schema": 1, "fixture_kind": KIND, "passed": False, "native_calls": None,
                  "failure": {"class": type(error).__name__, "message": str(error)[:2048]},
                  "source_bytes_preserved": None, "installer_qualified": False, "installed_editor_qualified": False,
                  "cross_platform_qualified": False}
    args.report.write_text(json.dumps(report, indent=2) + "\n", encoding="utf-8")
    print(json.dumps({"passed": report["passed"], "fixture_kind": KIND, "report": str(args.report), "native_calls": report["native_calls"]}))
    if not report["passed"]:
        raise SystemExit(1)


if __name__ == "__main__":
    main()
