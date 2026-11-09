# SPDX-License-Identifier: Apache-2.0
"""Exercise explicit content-free usage inputs through the built public CLI."""
import argparse
from contextlib import ExitStack, contextmanager, closing
from concurrent.futures import ThreadPoolExecutor
import hashlib
import importlib.util
import json
import os
import queue
import sqlite3
from pathlib import Path
import subprocess
import threading
import time
import uuid

spec = importlib.util.spec_from_file_location("airk_usage_fixture", Path(__file__).with_name("check-service-clients.py"))
clients = importlib.util.module_from_spec(spec)
spec.loader.exec_module(clients)


@contextmanager
def usage_mcp(fixture, name, base):
    """Exercise real transport with immutable native Codex attribution."""
    if fixture.single_image:
        spec = importlib.util.spec_from_file_location("usage_https_fixture", Path(__file__).with_name("http_fixture.py"))
        transport = importlib.util.module_from_spec(spec)
        spec.loader.exec_module(transport)
        selected = fixture.scopes[name]
        connection = fixture.enroll_http({"kind": "scoped", "selection": selected}, uuid.uuid4().hex, "codex")
        health = fixture.rpc("health")["observation"]
        fixture.assert_alive()
        assert health["instance"] == fixture.managed_health["instance"]
        transport.validate_native_identity(connection, health)
        assert connection["selection"]["kind"] == "scoped"
        actual, expected = connection["selection"]["selection"].copy(), selected.copy()
        assert actual.pop("expected_catalog_revision") >= expected.pop("expected_catalog_revision")
        assert actual == expected, "Usage HTTP enrollment changed reviewed scope"
        network = transport.McpClient(connection, fixture.control)
        try:
            yield network.send, network.receive
        finally:
            network.close()
        return
    # Exact predecessor compatibility: modern images must never reach stdio.
    with (base / "mcp-errors.log").open("w", encoding="utf-8") as errors:
        child = subprocess.Popen([str(fixture.binary), *fixture.arguments(name), "mcp"],
            stdin=subprocess.PIPE, stdout=subprocess.PIPE, stderr=errors, text=True,
            encoding="utf-8", env=dict(fixture.environment, AIRK_CLIENT="codex"),
            creationflags=subprocess.CREATE_NO_WINDOW if os.name == "nt" else 0)
        responses = queue.Queue()
        reader = threading.Thread(target=lambda: [responses.put(line) for line in child.stdout], daemon=True)
        reader.start()
        def send(value):
            child.stdin.write(json.dumps(value) + "\n")
            child.stdin.flush()
        def receive(identifier):
            deadline = time.monotonic() + 30
            while True:
                response = json.loads(responses.get(timeout=clients.remaining(deadline, 30)))
                if response.get("id") == identifier:
                    assert "error" not in response, response
                    return response
        try:
            yield send, receive
        finally:
            child.stdin.close()
            try:
                child.wait(timeout=10)
            except subprocess.TimeoutExpired:
                child.kill()
                child.wait(timeout=10)
            reader.join(timeout=5)
            child.stdout.close()


# Diagnostics and setup only. The measured workload below remains unchanged.
class UsageFixture(clients.Fixture):
    def __init__(self, *args, **kwargs):
        super().__init__(*args, **kwargs)
        self.failed_read_local = threading.local()
        self.failed_read_lock = threading.Lock()
        self.failed_read_count = 0

    def run(self, *args, **kwargs):
        result = super().run(*args, **kwargs)
        if getattr(self.failed_read_local, "active", False):
            self.failed_read_local.last = (args, result)
        return result

    def query(self, *args, **kwargs):
        self.failed_read_local.active = True
        self.failed_read_local.last = None
        try:
            return super().query(*args, **kwargs)
        except BaseException as error:
            try:
                with self.failed_read_lock:
                    assert self.failed_read_count < 32, "Failed-read diagnostic cap reached"
                    self.failed_read_count += 1
                    sequence = self.failed_read_count
                last = self.failed_read_local.last
                command, result = last if last is not None else ((), None)
                def bounded(raw):
                    raw = raw or b""
                    if isinstance(raw, str):
                        raw = raw.encode("utf-8", errors="replace")
                    return {"utf8": raw[:65536].decode("utf-8", errors="replace"),
                            "total_bytes": len(raw), "truncated": len(raw) > 65536}
                path = self.owner / f"failed-read-{sequence:02d}.json"
                with path.open("x", encoding="utf-8") as output:
                    json.dump({"command": [str(self.binary), *map(str, command)],
                               "query": [str(value) for value in args], "options": kwargs,
                               "returncode": result.returncode if result is not None else None,
                               "stdout": bounded(result.stdout if result is not None else getattr(error, "stdout", None)),
                               "stderr": bounded(result.stderr if result is not None else getattr(error, "stderr", None)),
                               "failure": str(error)[:4096]}, output, indent=2)
                error.add_note(f"Bounded exact failed-read evidence: {path}")
            except BaseException as diagnostic_error:
                error.add_note(f"Diagnostic retention failed: {diagnostic_error}")
            raise
        finally:
            self.failed_read_local.active = False


@contextmanager
def retained_usage_fixture():
    with clients.owned_client_fixture() as temporary:
        try:
            yield temporary
        except BaseException as error:
            error.add_note(f"Owned fictional usage fixture retained: {temporary}")
            print(json.dumps({"status": "failed", "retained_fixture": temporary}), flush=True)
            raise


def usage_quiescence(fixture, project):
    """One atomic read-only metadata observation; never changes audit state."""
    binding = fixture.projects[project]["binding"]
    registry = (fixture.control / "service-1.4.sqlite3").resolve(strict=True)
    assert registry.is_relative_to(fixture.owner), "Usage registry escaped fictional owner"
    with closing(sqlite3.connect(registry.as_uri() + "?mode=ro", uri=True, timeout=2)) as connection:
        row = connection.execute(
            "SELECT paused,"
            "(SELECT count(*) FROM service_jobs WHERE project=p.binding AND state IN('pending','running')),"
            "(SELECT count(*) FROM service_recovery_budget WHERE scope=?) "
            "FROM projects p WHERE binding=?", ("integrity:" + binding, binding)).fetchone()
    assert row is not None, "Usage fixture registration absent"
    assert row[0] in (0, 1) and row[1] >= 0 and row[2] in (0, 1), row
    return {"paused": bool(row[0]), "active_jobs": row[1], "audit_veto": row[2] != 0}


def usage_plain(path, *, directory=False):
    info = path.lstat()
    assert not path.is_symlink() and not getattr(info, "st_file_attributes", 0) & 0x400, "Linked usage fixture path"
    assert path.is_dir() if directory else path.is_file(), "Unexpected usage fixture entry"
    if not directory:
        assert info.st_nlink == 1, "Aliased usage fixture file"
    return info.st_dev, info.st_ino


def usage_head_token(raw, head):
    # Compare the actual native token supplied by published_status. Hash the
    # exact saved canonical bytes, never a Python reserialization of CurrentHead.
    digest = hashlib.sha256(b"AIRK-DIGEST-1\x00")
    for part in (b"current-index-head/2", raw):
        digest.update(len(part).to_bytes(8, "big"))
        digest.update(part)
    return f"airk-r1:{head['scope']['incarnation']}:{head['sequence']}:{digest.hexdigest()}"


@contextmanager
def hold_usage_reader(fixture, project, expected_publication):
    """Fixture-only real WAL reader continuity inside the held native writer.

    This does not issue a native evidence query or modify its accounting. Pinning
    starts with BEGIN plus an actual current_state read, not connection creation.
    It does not replace native ownership, private integrity or source authority.
    """
    registration = fixture.projects[project]
    # This driver's source and state layout is explicit and fictional. Normalize
    # the registration's Windows extended prefix by same-file identity, rather
    # than treating lexical drive-prefix spellings as an ownership proof.
    expected = fixture.owner / "index-data"
    if fixture.catalog_authority:
        expected /= registration["collection"]
    data = expected.resolve(strict=True)
    assert data.samefile(Path(registration["data_root"])), "Usage registration changed the exact fixture storage layout"
    assert data.is_relative_to(fixture.owner.resolve(strict=True))
    logical = data / "collections-1.4" / registration["collection"]
    for path in (data, logical.parent, logical):
        usage_plain(path, directory=True)
    stable_owner_path = logical / "service-owner.json"
    stable_owner_id = usage_plain(stable_owner_path)
    with stable_owner_path.open("rb") as stream:
        stable_owner_raw = stream.read(65537)
    assert len(stable_owner_raw) <= 65536
    stable_owner = json.loads(stable_owner_raw)
    assert stable_owner["version"] == 140
    assert all(stable_owner[field] == registration[field] for field in ("root", "data_root", "collection"))
    manifest_path = logical / "active.json"
    manifest_id = usage_plain(manifest_path)
    with manifest_path.open("rb") as stream:
        manifest_raw = stream.read(65537)
    assert len(manifest_raw) <= 65536, "Usage manifest exceeds bound"
    manifest = json.loads(manifest_raw)
    active = manifest["active"]
    assert manifest["version"] == 140 and isinstance(active, str) and len(active) == 32
    assert all(c in "0123456789abcdef" for c in active)
    incarnations = logical / "incarnations"
    selected = incarnations / active
    for path in (incarnations, selected):
        usage_plain(path, directory=True)
    selected_id = usage_plain(selected, directory=True)
    owner_path = selected / "current-owner.json"
    owner_id = usage_plain(owner_path)
    with owner_path.open("rb") as stream:
        owner_raw = stream.read(65537)
    assert len(owner_raw) <= 65536
    owner = json.loads(owner_raw)
    expected_scope = {"root": registration["root"], "collection": registration["collection"], "incarnation": active}
    assert owner["version"] == 140 and owner["scope"] == expected_scope
    database = selected / "current.sqlite3"
    database_id = usage_plain(database)
    # Native service readers retain these owned names. Do not create/recover a
    # missing database or sidecar, and do not borrow a different selected slot.
    for suffix in ("-wal", "-shm"):
        usage_plain(Path(str(database) + suffix))
    assert not Path(str(database) + "-journal").exists()
    with closing(sqlite3.connect(database.as_uri() + "?mode=ro", uri=True, timeout=2)) as connection:
        connection.execute("PRAGMA query_only=ON")
        assert connection.execute("PRAGMA journal_mode").fetchone()[0] == "wal"
        connection.execute("BEGIN")
        phase, raw = connection.execute("SELECT phase,head FROM current_state WHERE id=1 AND length(head)<=65536").fetchone()
        head = json.loads(raw)
        assert phase == "ready" and head["version"] == 2 and head["scope"] == expected_scope
        assert isinstance(head["sequence"], int) and not isinstance(head["sequence"], bool) and head["sequence"] > 0
        token = usage_head_token(raw, head)
        assert token == expected_publication["token"] and head["sequence"] == expected_publication["generation"]
        def recheck():
            assert usage_plain(stable_owner_path) == stable_owner_id and stable_owner_path.read_bytes() == stable_owner_raw
            assert usage_plain(manifest_path) == manifest_id and manifest_path.read_bytes() == manifest_raw
            assert usage_plain(selected, directory=True) == selected_id
            assert usage_plain(owner_path) == owner_id and owner_path.read_bytes() == owner_raw
            assert usage_plain(database) == database_id
        recheck()
        primary = None
        try:
            yield
        except BaseException as error:
            primary = error
            raise
        finally:
            try:
                recheck()
            except BaseException as error:
                if primary is None:
                    raise
                primary.add_note(f"Usage reader identity recheck failed: {error}")
            finally:
                connection.rollback()  # Close reader before writer release/pause restoration.



@contextmanager
def stable_usage_publication(fixture, project, expected_publication=None):
    """Real audited, paused and quiescent owner before fixture writer exclusion.

    Pause is only a temporary desired-state setting of this fictional fixture.
    A claimed audit must drain while unpaused; a veto is never reset or ignored.
    No measured search or any whole-read replay is submitted by this preflight.
    """
    binding = fixture.projects[project]["binding"]
    prior = usage_quiescence(fixture, project)["paused"]
    primary = None
    try:
        if prior:
            fixture.rpc({"pause": {"project": binding, "paused": False}})
        # Native Verify succeeds only with actual complete physical proof. This
        # explicit setup job is outside the later 41-search accounting window.
        fixture.wait(fixture.enqueue(project, "verify"))
        deadline = time.monotonic() + 90
        for attempt in range(4):
            while True:
                observed = usage_quiescence(fixture, project)
                assert not observed["paused"], "Unexpected pause during audit drain"
                if observed["active_jobs"] == 0:
                    assert not observed["audit_veto"], ("Unresolved audit before usage measurement", observed)
                    break
                assert time.monotonic() < deadline, ("Usage preflight drain deadline", observed)
                time.sleep(min(0.025, clients.remaining(deadline, 90)))
            fixture.rpc({"pause": {"project": binding, "paused": True}})
            observed = usage_quiescence(fixture, project)
            assert observed["paused"], ("Pause was not retained", observed)
            if observed["active_jobs"] == 0 and not observed["audit_veto"]:
                break
            # A job can be claimed between the prior observation and Pause.
            # Restore execution and drain that actual job; never leave a live
            # Verify suspended behind the writer lock or clear its veto.
            fixture.rpc({"pause": {"project": binding, "paused": False}})
            assert attempt < 3 and time.monotonic() < deadline, ("Usage preflight raced repeatedly", observed)
        with fixture.hold_publication(project), hold_usage_reader(fixture, project, expected_publication):
            yield
    except BaseException as error:
        primary = error
        raise
    finally:
        try:
            fixture.rpc({"pause": {"project": binding, "paused": prior}})
        except BaseException as error:
            if primary is None:
                raise
            primary.add_note(f"Failed to restore original fixture pause state: {error}")



def usage_arguments(fixture, project, command):
    """Accounting/policy uses owned management; measured evidence keeps its grant."""
    assert command == ("index",) or command[:2] in (("usage", "report"), ("usage", "policy"),
        ("usage", "import"), ("usage", "calibrate"), ("usage", "configure"), ("usage", "clear")), \
        "Unsupported usage fixture administrative command"
    return fixture.management_arguments(project)


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("--binary", type=Path, required=True)
    binary = parser.parse_args().binary.resolve(strict=True)
    with retained_usage_fixture() as temporary, ExitStack() as stack:
        base = Path(temporary).resolve(strict=True)
        root = base / "sources"
        root.mkdir()
        (root / "fixture.txt").write_text("Original bounded usage fixture.\n", encoding="utf-8")
        state = base / "index-data"
        state.mkdir()
        fixture = UsageFixture(binary.parent, base)
        assert fixture.binary == binary
        fixture.environment.pop("AIRK_USAGE_MODE", None)
        stack.enter_context(fixture)
        fixture.register("usage-fixture", root, state)
        fixture.configure("usage-fixture", {"operation": "vectors", "enabled": False})
        fixture.configure("usage-fixture", {"operation": "usage", "mode": "aggregates_only"})
        environment = fixture.environment

        def run(*command, fails=False):
            if command[0] not in ("index", "usage"):
                assert not fails
                return fixture.query("usage-fixture", *command, retry=False, admission_only=True)
            result = fixture.run(*usage_arguments(fixture, "usage-fixture", command), *command, check=False)
            if result.returncode:
                assert fails, result.stderr
                return None
            value = json.loads(result.stdout)
            if "job" in value:
                job = fixture.wait(value, "failed" if fails else "succeeded")
                if fails:
                    return None
                if command[0] == "index":
                    return fixture.published_status("usage-fixture")
                value = fixture.rpc({"configuration_result": {"id": job["id"]}})["result"]
            else:
                assert not fails, (command, value)
            return value

        def write_json(name, value):
            path = base / name
            path.write_text(json.dumps(value), encoding="utf-8")
            return path

        indexed = run("index")
        # Each measured query is submitted once; only explicitly unsubmitted admission may wait.
        # Hold only this fixture's actual native writer advisory file; release before mutations.
        with stable_usage_publication(fixture, "usage-fixture", indexed["publication"]):
            request = run("search", "--mode", "lexical", "fixture")
            native_calls = run("usage", "report")["report"]["calls"]
            with ThreadPoolExecutor(max_workers=4) as workers:
                replies = list(workers.map(lambda _: run("search", "--mode", "lexical", "fixture"), range(40)))
            assert all(reply["hits"] for reply in replies)
            assert len({reply["request_id"] for reply in replies}) == 40
            native_calls += 40
            concurrent = run("usage", "report")
            assert concurrent["report"]["calls"] == native_calls, "concurrent CLI observations were lost"
            assert concurrent["lifetime"]["calls"] == native_calls
            comparisons = concurrent["report"]["comparisons"]["full_returned_sources"]
            assert comparisons["compared_calls"] == 41
            assert concurrent["report"]["eligible_evidence_calls"] == 41
            assert concurrent["report"]["missing_baseline_calls"] == 0
            assert concurrent["report"]["baseline_unavailable"] == {}
            assert int(comparisons["returned_utf8_bytes"]) > 0
            assert int(comparisons["token_estimate"]["nominal_reduction"]) < 0, "Tiny complete-file results must preserve expansion from response overhead"
            # Exercise actual HTTPS for modern images (stdio for predecessors),
            # including conditional evidence and native Codex usage attribution.
            with usage_mcp(fixture, "usage-fixture", base) as (send, receive):
                def rpc(identifier, method, params):
                    send({"jsonrpc": "2.0", "id": identifier, "method": method, "params": params})
                    response = receive(identifier)
                    assert "error" not in response, response
                    return response["result"]
                rpc(1, "initialize", {"protocolVersion": "2025-11-25", "capabilities": {},
                                      "clientInfo": {"name": "codex", "version": "airk-native-fixture-1"}})
                send({"jsonrpc": "2.0", "method": "notifications/initialized"})
                arguments = {"path": "fixture.txt", "start_line": 1, "end_line": 1, "max_bytes": 200, "response_mode": "full"}
                reply = rpc(2, "tools/call", {"name": "airk_read_file", "arguments": arguments})
                assert not reply.get("isError"), reply
                read = json.loads(reply["content"][0]["text"])
                reply = rpc(3, "tools/call", {"name": "airk_read_file", "arguments": {**arguments, "if_evidence": read["read"]["validator"]}})
                assert not reply.get("isError"), reply
                assert json.loads(reply["content"][0]["text"])["read"]["status"] == "not_modified"
                reply = rpc(4, "tools/call", {"name": "airk_search", "arguments": {"query": "fixture", "mode": "lexical"}})
                assert not reply.get("isError"), reply
            native_calls += 3
            mcp_report = run("usage", "report")["report"]
            assert mcp_report["calls"] == native_calls
            assert mcp_report["eligible_evidence_calls"] == 43
            assert mcp_report["comparisons"]["full_returned_sources"]["compared_calls"] == 43
            assert mcp_report["missing_baseline_calls"] == 0
            assert mcp_report["clients"]["codex"] >= 3
        measurement = write_json("measurement.json", {
            "schema_version": 1, "event_id": uuid.uuid4().hex,
            "unix_ms": int(time.time() * 1000), "client": "codex", "content_kind": "code",
            "provenance": "client_reported",
            "context": {
                "baseline_kind": "observed_alternative", "generation": indexed["generation"],
                "representation": "logical_result_json", "baseline_utf8_bytes": 80,
                "returned_utf8_bytes": 400, "tokens": None,
            },
            "session": {
                "model_revision_sha256": "a" * 64, "requests": 2,
                "input_tokens": 120, "cached_input_tokens": None, "output_tokens": 12,
                "elapsed_ms": 1000, "tool_round_trips": None,
            },
            "paired_task": None,
            "airk_request_ids": [request["request_id"]],
        })
        preview = run("usage", "import", measurement, "--preview")
        assert preview["recorded"] is False
        assert run("usage", "report")["supplemental"] == []
        assert run("usage", "import", measurement)["recorded"] is True
        assert run("usage", "import", measurement)["duplicate"] is True
        tokenizer = write_json("tokenizer.json", {
            "schema": 1, "model_revision_sha256": "a" * 64,
            "merges": [["68", "65"], ["6865", "6c"], ["68656c", "6c"], ["68656c6c", "6f"]],
        })
        baseline = write_json("baseline.json", "hellohello")
        returned = write_json("returned.json", "hello")
        calibration = ["usage", "calibrate", "--tokenizer", tokenizer, "--baseline", baseline,
                       "--returned", returned, "--generation", indexed["generation"], "--content-kind", "code"]
        assert run(*calibration)["summary"]["token_reduction"] == "1"
        assert run(*calibration, "--record")["recording"]["recorded"] is True
        prices = write_json("prices.json", {
            "model": "fictional-model", "currency": "USD", "rate_date": "2026-10-14",
            "input_price_per_million": "2.50", "cached_input_treatment": "unknown",
            "output_price_per_million": "10.00",
            "cached_input_price_per_million": None, "context_assumes_uncached": True,
        })
        report = run("usage", "report", "--price-scenario", prices)
        assert report["report"]["calls"] == native_calls
        assert report["report"]["provider_token_usage_observed"] is False
        assert report["report"]["estimated_bill_savings"] is None
        assert report["health"]["supplemental_duplicate_observations"] == 1
        assert report["lifetime"]["calls"] == native_calls
        assert sum(s["linked_airk_requests"] for b in report["supplemental"] for s in b["summaries"]) == 1
        scenario = report["price_scenario"]
        observed = next(row for row in scenario["context_equivalents"]
                        if row.get("stratum", {}).get("provenance") == "client_reported")
        assert observed["nominal_input_cost_equivalent"] == "-0.000200000000"
        assert scenario["session_input_equivalents"][0]["unpriced_input_tokens_unknown_cache_split"] == "120"
        assert scenario["session_input_equivalents"][0]["uncached_input_equivalent"] is None
        assert scenario["session_input_equivalents"][0]["output_equivalent"] == "0.000120000000"
        ledger = Path(fixture.projects["usage-fixture"]["data_root"]) / "collections-1.4" / "usage-fixture" / "usage.sqlite3"
        assert b"hellohello" not in ledger.read_bytes()
        run("usage", "configure", "--mode", "disabled")
        before = hashlib.sha256(ledger.read_bytes()).digest()
        run("usage", "import", measurement, fails=True)
        run(*calibration, "--record", fails=True)
        assert run(*calibration)["recording"] is None
        assert hashlib.sha256(ledger.read_bytes()).digest() == before
        run("usage", "clear")
        assert run("usage", "report")["supplemental"] == []
        assert run("usage", "report")["lifetime"] is None
        assert (root / "fixture.txt").read_text(encoding="utf-8") == "Original bounded usage fixture.\n"
    print("Usage input CLI smoke passed: concurrent process accounting, separate imports, replay protection, exact local byte-BPE counts, signed price scenarios, unknown cache splits, disabled no-writes and clear.")


if __name__ == "__main__":
    main()
