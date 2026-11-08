# SPDX-License-Identifier: Apache-2.0
"""Exercise real installed-extension 1.8.7 to reviewed legacy retirement.

Run only after P14 acceptance and after the recovery owners/adapters are built.
Every invocation creates its own two-index installation. No production state is
copied. Failure retains that installation and all command streams for recovery.
"""
import argparse
from contextlib import ExitStack, closing
import hashlib
import importlib.util
import json
import os
from pathlib import Path, PurePosixPath
import queue
import shutil
import sqlite3
import subprocess
import sys
import threading
import time
import uuid

OLD_MANIFEST = "b185f0980631b319968dbd209c14970020f11048c47a46412a57d6f1e7d1fb61"
OLD_MAIN = "03f7d6059c0a86f84d385811e81f0ba3da447bdbb61cb6f41abac974900a45d7"
OLD_EMBED = "584126658a83353cdf2d758b2e01f52155706db72d90ead80984580d3c940edc"
MAX_FRAME = 65536


def load(name, path):
    spec = importlib.util.spec_from_file_location(name, path)
    module = importlib.util.module_from_spec(spec)
    spec.loader.exec_module(module)
    return module


def sha(path):
    with path.open("rb") as stream:
        return hashlib.file_digest(stream, "sha256").hexdigest()


def plain(path):
    info = path.lstat()
    if path.is_symlink() or getattr(info, "st_file_attributes", 0) & 0x400:
        raise AssertionError(f"Linked/reparse fixture entry refused: {path}")


def contained(owner, path, *, exists=True):
    owner = owner.resolve(strict=True)
    path = path.absolute()
    if path == owner or not path.is_relative_to(owner):
        raise AssertionError("Fixture operation escaped exact owned directory")
    for component in (path, *path.parents):
        if component == owner:
            break
        if component.exists():
            plain(component)
    resolved = path.resolve(strict=exists)
    if resolved != path or not resolved.is_relative_to(owner):
        raise AssertionError("Fixture operation redirected")
    return path


def seal(runtime):
    manifest = runtime / "airk-install.json"
    value = json.loads(manifest.read_text(encoding="utf-8"))
    assert value["schema"] == 1 and value["os"] == "windows" and value["arch"] == "x86_64"
    assert isinstance(value["files"], dict) and 1 <= len(value["files"]) <= 4096
    hashes = {}
    for relative, expected in value["files"].items():
        item = PurePosixPath(relative)
        assert not item.is_absolute() and ".." not in item.parts and "\\" not in relative
        path = runtime.joinpath(*item.parts)
        for component in (path, *path.parents):
            plain(component)
            if component == runtime:
                break
        assert sha(path) == expected, f"Changed public payload: {relative}"
        hashes[relative] = expected
    return {"manifest_sha256": sha(manifest), "version": value["version"], "files": hashes}


def immutable_upgrade(control):
    path = control / "index-catalog-upgrade.json"
    assert path.stat().st_size <= 32 * 1024 * 1024
    journal = json.loads(path.read_text(encoding="utf-8"))
    backups = {}
    for entry in journal["backups"]:
        if entry["complete"]:
            backup = control / f"catalog-schema11-{journal['plan']['id']}-{entry['slot']}.sqlite3"
            assert sha(backup) == entry["sha256"] and backup.stat().st_size == entry["bytes"]
            backups[backup.name] = entry["sha256"]
    assert backups, "No actual immutable native backup"
    return {"plan": journal["plan"], "plan_digest": journal["plan_digest"], "backups": backups}


def publication(value):
    found = []
    def visit(item):
        if isinstance(item, dict):
            pub = item.get("publication")
            if isinstance(pub, dict) and isinstance(pub.get("token"), str):
                found.append(pub["token"])
            for entry in item.values():
                visit(entry)
        elif isinstance(item, list):
            for entry in item:
                visit(entry)
    visit(value)
    assert found and len(set(found)) == 1, "Missing/ambiguous exact retained publication"
    return found[0]


def success_flag(envelope):
    assert isinstance(envelope, dict)
    assert "isError" not in envelope or envelope["isError"] is False, envelope


def search_arguments(payload, old_payload, new_payload):
    # The actual sealed 1.8.7 parser rejects verification. These common inputs
    # retain identical lexical read semantics; only the new parser accepts Off.
    assert payload in (old_payload, new_payload) and old_payload != new_payload
    args = {"query": "legacyhealthyneedle", "mode": "lexical", "limit": 4,
        "response_mode": "full"}
    if payload == new_payload:
        args["verification"] = "off"
    return args


def selected_payload(root, install, old_payload, new_payload):
    selection_path = contained(root, install / "selected.json")
    assert selection_path.stat().st_size <= MAX_FRAME
    selected = json.loads(selection_path.read_text(encoding="utf-8"))
    identity = selected["active"]
    expected = [item for item in (old_payload, new_payload)
        if item["manifest_sha256"] == identity]
    assert len(expected) == 1, "Selected runtime is outside the two actual sealed payloads"
    assert isinstance(identity, str) and len(identity) == 64 and all(c in "0123456789abcdef" for c in identity)
    directory = contained(root, install / "runtimes" / identity)
    assert seal(directory) == expected[0], "Selected installed payload changed"
    binary = contained(root, directory / "airk.exe")
    assert sha(binary) == expected[0]["files"]["airk.exe"]
    return binary, selected


def retained_launcher_snapshot(root, install):
    """Observe exact owned fixture paths without reading an executable into RAM."""
    result = {}
    for name, maximum in (("airk-launcher.exe", 2 * 1024 * 1024 * 1024), ("launcher.json", 4096)):
        path = contained(root, install / name, exists=False)
        try:
            info = path.lstat()
        except FileNotFoundError:
            result[name] = None
            continue
        plain(path)
        assert path.is_file() and info.st_nlink == 1 and info.st_size <= maximum
        result[name] = {"bytes": info.st_size, "sha256": sha(path), "device": info.st_dev, "inode": info.st_ino}
    return result


def qualify_launcher_repair(pair, proof, selected, before, after):
    assert type(pair.get("schema")) is int and pair["schema"] in (1, 2), "Unknown runtime launcher contract"
    expected = pair["schema"] == 1
    assert proof == {"launcher_repaired": expected, "runtime": selected["active"]}
    assert type(proof["launcher_repaired"]) is bool
    if not expected:
        assert before == after, "Modern all-role proof created or changed predecessor launcher state"
    return proof


def old_status_ready(value, project, indexed_job):
    # Actual public 1.8.7 Status exposes compact freshness even in full mode.
    # The completed Index receipt binds publication; full provenance is checked
    # separately in BOTH Search bodies, where the public payload exposes it.
    assert indexed_job["state"] == "succeeded" and indexed_job["kind"] == "index"
    assert indexed_job["outcome"]["state"] == "succeeded"
    assert value["collection"] == project["collection"]
    freshness = value["freshness"]
    assert isinstance(freshness, dict) and {"state", "live_source_verified"} <= freshness.keys()
    assert freshness["state"] in ("pending", "running", "idle"), freshness
    assert freshness["live_source_verified"] is False, freshness
    if freshness["state"] != "idle":
        return False
    assert value["publication"]["token"] == indexed_job["outcome"]["revision"], "Status publication differs from exact completed Index"
    return True


def validate_old_search_freshness(value, project, indexed_job):
    assert value["collection"] == project["collection"]
    assert value["publication"]["token"] == indexed_job["outcome"]["revision"]
    freshness = value["freshness"]
    assert freshness["state"] == "idle" and freshness["error"] is None, freshness
    assert freshness["source_job"] == indexed_job["id"], freshness
    requested, completed = freshness["requested_watermark"], freshness["completed_watermark"]
    assert type(requested) is int and type(completed) is int and requested >= 0 and completed >= requested
    assert freshness["running_watermark"] is None, freshness
    assert type(freshness["last_success_unix_ms"]) is int and freshness["last_success_unix_ms"] > 0


def finish_owner(fixture, receipt):
    receipt["actual_host_readonly_parity"] = fixture.host_parity_receipts
    receipt["actual_old_read_quiescence"] = getattr(fixture, "old_read_quiescence", [])
    receipt["fixture_retained"] = True
    try:
        # Selection may still be OLD after a failed pre-upgrade assertion. Never
        # guess from the intended candidate or start/abort/activate during teardown.
        receipt["final_quiescence"] = fixture.drain()
    except Exception as error:
        receipt["stop_error"] = repr(error)
        receipt["status"] = "failed"


def bounded_differences(left, right, limit=32):
    """Diagnostic only: never changes the actual full parity comparison."""
    entries = []
    truncated = False
    def shown(value):
        text = json.dumps(value, ensure_ascii=False, sort_keys=True)
        return {"type": type(value).__name__, "json_preview": text[:256],
            "truncated": len(text) > 256,
            "json_sha256": hashlib.sha256(text.encode("utf-8")).hexdigest()}
    def visit(a, b, pointer):
        nonlocal truncated
        if len(entries) >= limit:
            truncated = True
            return
        if type(a) is not type(b):
            entries.append({"pointer": pointer, "cli": shown(a), "mcp": shown(b)})
        elif isinstance(a, dict):
            for key in sorted(a.keys() | b.keys()):
                path = pointer + "/" + key.replace("~", "~0").replace("/", "~1")
                if key not in a or key not in b:
                    if len(entries) >= limit:
                        truncated = True
                        return
                    entries.append({"pointer": path, "present_in": "cli" if key in a else "mcp",
                        "value": shown(a[key] if key in a else b[key])})
                else:
                    visit(a[key], b[key], path)
        elif isinstance(a, list):
            if len(a) != len(b):
                entries.append({"pointer": pointer, "cli_length": len(a), "mcp_length": len(b)})
            for index, (x, y) in enumerate(zip(a, b)):
                visit(x, y, pointer + "/" + str(index))
        elif a != b:
            entries.append({"pointer": pointer, "cli": shown(a), "mcp": shown(b)})
    assert isinstance(limit, int) and 1 <= limit <= 64
    visit(left, right, "")
    return {"difference_limit": limit, "differences_shown": len(entries),
        "truncated": truncated, "entries": entries}


class Mcp:
    """One exact owned stdio child, bounded lines/queue/wait; never a PID lookup."""
    def __init__(self, fixture, binary, project):
        self.fixture = fixture
        identity = uuid.uuid4().hex
        self.log = (fixture.root / f"mcp-{identity}.stderr").open("xb")
        self.trace = (fixture.root / f"mcp-{identity}-wire.jsonl").open("x", encoding="utf-8")
        try:
            self.child = subprocess.Popen([str(binary), *fixture.project_args(project), "mcp"],
                stdin=subprocess.PIPE, stdout=subprocess.PIPE, stderr=self.log,
                env=fixture.environment, creationflags=fixture.flags)
        except BaseException:
            self.trace.close()
            self.log.close()
            raise
        self.lines = queue.Queue(maxsize=8)
        self.counter = 0
        def receive():
            try:
                while True:
                    line = self.child.stdout.readline(MAX_FRAME + 1)
                    if not line:
                        break
                    if len(line) > MAX_FRAME:
                        raise AssertionError("MCP frame exceeds public bound")
                    self.lines.put(line, timeout=5)
            except Exception as error:
                self.lines.put(error, timeout=5)
            finally:
                try:
                    self.lines.put(None, timeout=5)
                except queue.Full:
                    pass
        self.reader = threading.Thread(target=receive, daemon=True)
        self.reader.start()
        try:
            reply = self.call("initialize", {"protocolVersion": "2025-11-25", "capabilities": {},
                "clientInfo": {"name": "legacy-retirement-fixture", "version": "1"}})
            assert reply["serverInfo"]["name"] == "airk"
            self.write({"jsonrpc": "2.0", "method": "notifications/initialized"})
        except BaseException:
            self.close()
            raise

    def write(self, value):
        raw = (json.dumps(value, ensure_ascii=False) + "\n").encode("utf-8")
        assert len(raw) <= MAX_FRAME
        self.child.stdin.write(raw)
        self.child.stdin.flush()

    def call(self, method, params):
        self.counter += 1
        request = {"jsonrpc": "2.0", "id": self.counter, "method": method, "params": params}
        self.trace.write(json.dumps({"request": request}, ensure_ascii=False) + "\n")
        self.trace.flush()
        self.write(request)
        line = self.lines.get(timeout=60)
        if line is None or isinstance(line, Exception):
            raise AssertionError(f"Owned MCP reader failed: {line}")
        # Exact raw bytes survive malformed/error replies and every subsequent
        # assertion. The unchanged reader frame bound still applies.
        self.trace.write(json.dumps({"response_bytes_hex": line.hex()}) + "\n")
        self.trace.flush()
        reply = json.loads(line)
        assert reply.get("id") == self.counter and "error" not in reply, reply
        return reply["result"]

    def close(self):
        self.child.stdin.close()
        try:
            self.child.wait(timeout=15)
        except subprocess.TimeoutExpired:
            self.child.kill()  # Exact Popen child only; service roles drain separately.
            self.child.wait(timeout=15)
            raise AssertionError("Owned MCP child failed to exit")
        finally:
            self.child.stdout.close()
            self.log.close()
            self.trace.close()


class Fixture:
    def __init__(self, args, helpers, clients):
        self.helpers, self.clients = helpers, clients
        self.temporary = helpers.owned_temporary(args.fixture_parent.resolve(strict=True))
        self.root = Path(self.temporary.name).resolve(strict=True)
        self.install, self.state = self.root / "install", self.root / "state"
        self.control = self.state / "control"
        self.environment = dict(os.environ, AIRK_MCP_RESPONSE_MODE="full",
            HTTP_PROXY="http://127.0.0.1:1", HTTPS_PROXY="http://127.0.0.1:1")
        self.flags = subprocess.CREATE_NO_WINDOW
        self.commands = []
        self.old, self.new = args.previous_runtime.resolve(strict=True), args.runtime.resolve(strict=True)
        self.old_seal, self.new_seal = seal(self.old), seal(self.new)
        assert self.old_seal["manifest_sha256"] == OLD_MANIFEST
        assert self.old_seal["version"] == "1.8.7" and len(self.old_seal["files"]) == 674
        assert self.old_seal["files"]["airk.exe"] == OLD_MAIN
        assert self.old_seal["files"]["airk-embed.exe"] == OLD_EMBED
        assert self.new_seal["version"] == "1.8.8" and self.new_seal != self.old_seal
        self.projects = {}
        self.host_parity_receipts = []
        self.source_jobs = {}
        self.old_read_quiescence = []
        self.host_parity_options = args

    def command(self, binary, *arguments, body=None, check=True, timeout=180):
        result = self.helpers.record_native_command(self.root, self.commands, binary, arguments,
            input=None if body is None else json.dumps(body), flags=self.flags,
            environment=self.environment, timeout=timeout)
        if check and result.returncode:
            raise AssertionError(f"Native fixture command failed: {result.stderr[-4096:]}")
        return result

    def invoke(self, binary, *arguments, body=None):
        return json.loads(self.command(binary, *arguments, body=body).stdout)

    def rpc(self, binary, body, deadline=None):
        def invoke(exact, until):
            result = self.command(binary, "service", "rpc", "--runtime-dir", self.control,
                "--stdin", body=json.loads(exact), timeout=self.clients.remaining(until, 90))
            return json.loads(result.stdout)
        # Existing bounded registry READ whitelist only; register/RequestEpoch/
        # configuration/catalog mutations are one-shot even on busy or lost reply.
        return self.clients.metadata_rpc(invoke, body, deadline)

    def project_args(self, project):
        return ["--service-runtime-dir", str(self.control), "--service-project", project["binding"],
            "--root", project["root"], "--data-dir", project["data_root"], "--collection", project["collection"]]

    def wait(self, binary, receipt):
        identity = receipt["job"]["id"]
        deadline = time.monotonic() + 180
        while True:
            job = self.rpc(binary, {"job": {"id": identity}}, deadline)["job"]
            assert job["id"] == identity
            if job["state"] in ("succeeded", "failed", "canceled"):
                assert job["state"] == "succeeded", job
                return job
            assert time.monotonic() < deadline, "Job deadline; mutation is never resubmitted"
            time.sleep(0.05)

    def stage(self, directory, old=None):
        binary = directory / "airk.exe"
        identity = sha(directory / "airk-install.json")
        self.invoke(binary, "runtime", "stage", "--source", directory, "--install-dir", self.install,
            "--state-dir", self.state, "--approved-sha256", identity,
            "--verification-basis", "organization-digest")
        args = ["runtime", "activate", "--install-dir", self.install, "--runtime-sha256", identity]
        if old is not None:
            args += ["--expected-active", old]
        return self.command(binary, *args, check=False)

    def drain(self, binary=None):
        # Do not use lifecycle Stop: that persists a separate startup prohibition.
        # First request ordinary shutdown, then wait using EXISTING advisory files.
        selected_binary, selected = selected_payload(self.root, self.install, self.old_seal, self.new_seal)
        if binary is not None:
            assert sha(binary) == sha(selected_binary), "Requested drain binary differs from actual selected owner"
        binary = selected_binary
        observed = self.command(binary, "service", "health", "--runtime-dir", self.control, check=False)
        health = None
        if observed.returncode == 0:
            health = json.loads(observed.stdout)
            assert health["status"] == "health" and health["observation"]["runtime_sha256"] == selected["active"]
            assert health["observation"]["executable_sha256"] == sha(binary)
            reply = self.invoke(binary, "service", "shutdown", "--runtime-dir", self.control)
            assert reply["status"] == "stopping", reply
        # Unavailable health does not imply absence. Existing owner files and the
        # final native all-role proof remain mandatory; never restart for a receipt.
        names = ["instance-1.4.lock", "embedding-1.4.lock", "source-watch-1.4.lock"]
        names += [f"worker-1.4-{p['binding']}.lock" for p in self.projects.values()]
        deadline = time.monotonic() + 60
        import msvcrt
        while True:
            try:
                with ExitStack() as stack:
                    for name in names:
                        # Optional helpers may never have started in this lexical
                        # fixture. Do not create files to manufacture absence.
                        if not (self.control / name).exists() and name in {
                            "embedding-1.4.lock", "source-watch-1.4.lock"}:
                            continue
                        path = contained(self.root, self.control / name)
                        stream = stack.enter_context(path.open("r+b"))  # NEVER creates a lock.
                        msvcrt.locking(stream.fileno(), msvcrt.LK_NBLCK, 1)
                        stack.callback(lambda f=stream: (f.seek(0), msvcrt.locking(f.fileno(), msvcrt.LK_UNLCK, 1)))
                break
            except OSError:
                assert time.monotonic() < deadline, "Existing owner leases did not drain"
                time.sleep(0.05)
        # Keep the actual owning native call as the final all-role lease proof.
        # Modern schema 2 must not edit the retained historical launcher at all.
        pair_path = contained(self.root, binary.parent / "airk-runtime.json")
        assert pair_path.stat().st_size <= 4096
        expected_seal = next(item for item in (self.old_seal, self.new_seal)
                             if item["manifest_sha256"] == selected["active"])
        assert sha(pair_path) == expected_seal["files"]["airk-runtime.json"]
        pair = json.loads(pair_path.read_text(encoding="utf-8"))
        assert pair["airk_sha256"] == sha(binary)
        before_launcher = retained_launcher_snapshot(self.root, self.install)
        proof = self.invoke(binary, "runtime", "repair-launcher", "--install-dir", self.install)
        qualify_launcher_repair(pair, proof, selected, before_launcher,
                                retained_launcher_snapshot(self.root, self.install))
        assert json.loads((self.install / "selected.json").read_text(encoding="utf-8")) == selected
        selected_after, _ = selected_payload(self.root, self.install, self.old_seal, self.new_seal)
        assert selected_after.samefile(binary)
        return {"selected_manifest_sha256": selected["active"], "binary": str(binary),
            "authenticated_health": health, "health_exit_code": observed.returncode,
            "native_all_role_proof": proof}

    def wait_old_read_quiescence(self, binary, project, deadline):
        assert sha(binary) == self.old_seal["files"]["airk.exe"]
        indexed_job = self.source_jobs[project["binding"]]
        # airk_status is the existing content-free native read. Its full response
        # is retained by command(); no search is submitted by this barrier.
        def read_status(until):
            result = self.command(binary, *self.project_args(project), "tools", "call", "airk_status",
                "--args", json.dumps({"response_mode": "full"}), check=False,
                timeout=self.clients.remaining(until, 60))
            envelope = json.loads(result.stdout)
            assert result.returncode in (0, 1)
            assert result.returncode == 0 or self.clients.read_retry_delay(envelope) is not None, envelope
            return envelope
        for poll in range(601):
            self.clients.remaining(deadline, 60)
            envelope, _ = self.clients.read_envelope(read_status, deadline)
            value = self.clients.tool_value(envelope)
            if old_status_ready(value, project, indexed_job):
                self.old_read_quiescence.append({"project": project["binding"],
                    "source_job": indexed_job["id"], "publication": indexed_job["outcome"]["revision"],
                    "freshness": value["freshness"], "metadata_polls": poll + 1,
                    "final_status_command_ordinal": len(self.commands),
                    "qualification": "Actual old compact Status idle; full Search provenance checked separately; not live verification"})
                return
            time.sleep(min(.1, self.clients.remaining(deadline, 60)))
        raise AssertionError("Old metadata observation budget exhausted; no evidence read was retried to match")

    def query(self, binary, project):
        digest = sha(binary)
        payloads = [item for item in (self.old_seal, self.new_seal) if item["files"]["airk.exe"] == digest]
        assert len(payloads) == 1, "Query binary differs from actual old/new payload seals"
        payload = payloads[0]["manifest_sha256"]
        args = search_arguments(payload, self.old_seal["manifest_sha256"], self.new_seal["manifest_sha256"])
        deadline = time.monotonic() + 60
        if payload == self.old_seal["manifest_sha256"]:
            self.wait_old_read_quiescence(binary, project, deadline)
        observation = uuid.uuid4().hex
        retained = {}
        def retain(suffix, value):
            path = self.root / f"search-{observation}-{suffix}.json"
            with path.open("x", encoding="utf-8") as stream:
                json.dump(value, stream, indent=2)
                stream.write("\n")
            retained[suffix] = {"path": str(path), "sha256": sha(path)}
        attempts = {"cli": 0, "mcp": 0}
        def read(until):
            result = self.command(binary, *self.project_args(project), "tools", "call", "airk_search",
                "--args", json.dumps(args), check=False, timeout=self.clients.remaining(until, 60))
            envelope = json.loads(result.stdout)
            attempts["cli"] += 1
            retain(f"cli-attempt-{attempts['cli']}", envelope)
            assert result.returncode in (0, 1)
            assert result.returncode == 0 or self.clients.read_retry_delay(envelope) is not None, envelope
            return envelope
        cli, _ = self.clients.read_envelope(read, deadline)
        retain("cli", cli)
        success_flag(cli)
        body = self.clients.tool_value(cli)
        assert body["collection"] == project["collection"]
        assert isinstance(body["generation"], int) and not isinstance(body["generation"], bool)
        assert len(body["hits"]) == 1 and body["truncated"] is False, body
        hit = body["hits"][0]
        assert hit["citation"]["path"] == "README.md"
        assert hit["citation"]["content_hash"] == sha(Path(project["root"]) / "README.md")
        assert "legacyhealthyneedle" in hit["text"] and hit["truncated"] is False
        mcp = Mcp(self, binary, project)
        try:
            def read_mcp(_):
                envelope = mcp.call("tools/call", {"name": "search", "arguments": args})
                attempts["mcp"] += 1
                retain(f"mcp-attempt-{attempts['mcp']}", envelope)
                return envelope
            wire, _ = self.clients.read_envelope(read_mcp, time.monotonic() + 60)
            retain("mcp", wire)
        finally:
            mcp.close()
        success_flag(wire)
        other = self.clients.tool_value(wire)
        # Ordinary single-index legacy tool replies, not combined hashed envelopes.
        # Only documented request IDs and dated observation timing are normalized.
        def stable(value):
            if isinstance(value, dict):
                return {k: stable(v) for k, v in value.items() if k not in {
                    "request_id", "observed_unix_ms", "observed_from_unix_ms", "observed_to_unix_ms",
                    "cache_age_ms", "cached", "elapsed_ms"}}
            if isinstance(value, list):
                return [stable(v) for v in value]
            return value
        diagnostic = {"schema": "airk.legacy-read-parity-diagnostic/1",
            "complete_retained_replies": retained,
            "raw_body_differences": bounded_differences(body, other),
            "existing_parity_projection_differences": bounded_differences(stable(body), stable(other)),
            "comparison_unchanged": True,
            "no_cause_inferred": True}
        assert len(json.dumps(diagnostic).encode("utf-8")) <= MAX_FRAME
        retain("comparison", diagnostic)
        if payload == self.old_seal["manifest_sha256"]:
            for observed in (body, other):
                validate_old_search_freshness(observed, project, self.source_jobs[project["binding"]])
        assert stable(body) == stable(other), (
            "Complete CLI/MCP legacy-body mismatch; complete replies and bounded structural diff: "
            + retained["comparison"]["path"])
        # Explicit retained-data projection across versions: preserve ALL literal
        # hit/citation/text/location/truncation fields plus the exact publication.
        # Newly disclosed execution/readiness/version metadata is not projected.
        return {"publication": body["publication"], "token": publication(body),
            "collection": body["collection"], "generation": body["generation"],
            "original_root": project["root"], "hits": body["hits"], "truncated": body["truncated"]}

    def readonly_host_parity(self, phase, expected):
        args = self.host_parity_options
        if args.host_parity_extension is None:
            return
        assert args.scenario == "deleted" and phase in ("review", "authorized", "applied")
        authorization = expected["receipt"]["authorization"] if phase == "applied" else expected.get("authorization", expected)
        request = authorization["preview"]["request"]
        assert expected["status"] == phase
        input_path = contained(self.root, self.root / f"host-parity-{phase}-input.json", exists=False)
        output_path = contained(self.root, self.root / f"host-parity-{phase}-result.json", exists=False)
        payload = {"schema": 1, "phase": phase, "fixture": str(self.root),
            "fixture_parent": str(args.fixture_parent.resolve(strict=True)),
            "install": str(self.install), "control": str(self.control),
            "extension": str(args.host_parity_extension.resolve(strict=True)),
            "modules": str(args.host_parity_modules.resolve(strict=True)),
            "runtime": str(self.new), "runtime_manifest_sha256": self.new_seal["manifest_sha256"],
            "runtime_main_sha256": self.new_seal["files"]["airk.exe"],
            "expected": expected, "request": request}
        with input_path.open("x", encoding="utf-8") as output:
            json.dump(payload, output, indent=2)
            output.write("\n")
        before = immutable_upgrade(self.control)
        probe = Path(__file__).with_name("check-runtime-legacy-retirement-host.cjs").resolve(strict=True)
        self.command(args.host_parity_node.resolve(strict=True), probe,
            "--input", input_path, "--output", output_path, timeout=180)
        assert output_path.stat().st_size <= 16 * 1024 * 1024
        observed = json.loads(output_path.read_text(encoding="utf-8"))
        assert observed["status"] == "passed" and observed["phase"] == phase
        assert observed["fixture"] == str(self.root)
        assert observed["runtime_manifest_sha256"] == self.new_seal["manifest_sha256"]
        assert observed["input_sha256"] == sha(input_path)
        assert immutable_upgrade(self.control) == before, "Read-only host parity changed immutable upgrade authority"
        self.host_parity_receipts.append({"phase": phase, "input": str(input_path),
            "input_sha256": sha(input_path), "output": str(output_path), "output_sha256": sha(output_path)})

    def admin(self, action, body=None, *extra, check=True):
        return self.command(self.new / "airk.exe", "runtime", "legacy-retirement-" + action,
            "--install-dir", self.install, *extra, body=body, check=check)


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("--runtime", type=Path, required=True)
    parser.add_argument("--previous-runtime", type=Path, required=True)
    parser.add_argument("--fixture-parent", type=Path, required=True,
        help="Existing qualified owner-only synthetic fixture base; no ACL changes")
    parser.add_argument("--scenario", choices=("source-only", "deleted", "returned", "foreign-return"), required=True)
    parser.add_argument("--output", type=Path, required=True)
    parser.add_argument("--host-parity-extension", type=Path,
        help="Optional same-candidate canonical debug extension stage; deleted scenario only")
    parser.add_argument("--host-parity-modules", type=Path,
        help="Actual freshly compiled extension out directory, not mocked modules")
    parser.add_argument("--host-parity-node", type=Path,
        help="Explicit existing Node executable for read-only real RuntimeHost parity")
    args = parser.parse_args()
    parity = (args.host_parity_extension, args.host_parity_modules, args.host_parity_node)
    if any(value is not None for value in parity):
        assert all(value is not None for value in parity) and args.scenario == "deleted"
        assert args.host_parity_node.resolve(strict=True).is_file()
        assert (args.host_parity_modules.resolve(strict=True) / "serviceBootstrap.js").is_file()
        assert (args.host_parity_modules.resolve(strict=True) / "runtimeLegacyRetirement.js").is_file()
        assert args.host_parity_extension.resolve(strict=True).is_dir()
    assert os.name == "nt", "Actual retained 1.8.7 payload is Windows x86_64 only"
    args.output.parent.mkdir(parents=True, exist_ok=True)
    # Reserve a NEW receipt before any native work. Never overwrite old evidence,
    # even if another process races the initial path-existence observation.
    output_stream = args.output.open("x", encoding="utf-8")
    scripts = Path(__file__).resolve().parent
    helpers = load("legacy_upgrade_helpers", scripts / "check-runtime-upgrade.py")
    clients = load("legacy_client_helpers", scripts / "check-service-clients.py")
    fixture = None
    receipt = {"schema": 1, "scenario": args.scenario, "status": "failed", "executed": True,
        "qualification": "actual installed-extension 1.8.7 payload; not Git-tag equivalence",
        "raw_direct_evidence_connection_parity": "not exercised by this Python harness"}
    try:
        fixture = f = Fixture(args, helpers, clients)
        receipt["fixture"] = str(f.root)
        receipt["old_payload"], receipt["new_payload"] = f.old_seal, f.new_seal
        first = f.stage(f.old)
        assert first.returncode == 0
        original = b"legacyhealthyneedle original fictional bytes\n"
        for name in ("a", "b"):
            ancestor = f.root / name
            source, data = ancestor / "source", ancestor / "data"
            source.mkdir(parents=True)
            data.mkdir()
            (source / "README.md").write_bytes(original)
            p = f.invoke(f.old / "airk.exe", "--root", source, "--data-dir", data,
                "--collection", "legacy-" + name, "service", "prepare-registration")
            assert Path(p["root"]).samefile(source) and Path(p["data_root"]).samefile(data)
            assert p["collection"] == "legacy-" + name
            registered = f.rpc(f.old / "airk.exe", {"register": {"project": p}})
            assert registered["status"] == "registered"
            f.projects[name] = p
            # Explicit old-fixture actions: migration never invents these facts.
            configured = f.invoke(f.old / "airk.exe", *f.project_args(p), "configure", "--stdin",
                body={"operation": "vectors", "enabled": False})
            f.wait(f.old / "airk.exe", configured)
            f.source_jobs[p["binding"]] = f.wait(f.old / "airk.exe",
                f.invoke(f.old / "airk.exe", *f.project_args(p), "index"))
        before = f.query(f.old / "airk.exe", f.projects["b"])
        with closing(sqlite3.connect((f.control / "service-1.4.sqlite3").as_uri() + "?mode=ro", uri=True)) as db:
            db.execute("PRAGMA query_only=ON")
            receipt["actual_predecessor_user_version"] = db.execute("PRAGMA user_version").fetchone()[0]
        receipt["predecessor_quiescence"] = f.drain(f.old / "airk.exe")
        ancestor = contained(f.root, f.root / "a")
        source = contained(f.root, ancestor / "source")
        offline = contained(f.root, f.root / "a-offline", exists=False)
        identity = (ancestor.stat().st_dev, ancestor.stat().st_ino)
        if args.scenario == "source-only":
            shutil.rmtree(source)
        elif args.scenario == "returned":
            ancestor.rename(offline)
        else:
            shutil.rmtree(ancestor)
        assert not source.exists()
        activated = f.stage(f.new, OLD_MANIFEST)
        old_hash = f.old_seal["manifest_sha256"]
        assert old_hash == OLD_MANIFEST
        if args.scenario == "source-only":
            assert activated.returncode == 0, activated.stderr
            assert f.query(f.new / "airk.exe", f.projects["b"]) == before
            # Ordinary canonical native RetireIndex review, never the upgrade lane.
            binding = f.rpc(f.new / "airk.exe", {"catalog_legacy_binding": {
                "project": f.projects["a"]["binding"], "expected_catalog_revision": None}})["observation"]
            index = binding["eligibility"]["grant"]["grant"]["memberships"][0]["membership"]["index_id"]
            epoch = f.rpc(f.new / "airk.exe", "request_epoch")
            draft = f.rpc(f.new / "airk.exe", {"catalog_source_mutation_draft": {"selection": {
                "key": {"namespace": epoch["namespace"], "epoch": epoch["epoch"], "nonce": uuid.uuid4().hex},
                "expected_catalog_revision": binding["catalog_revision"], "index_id": index,
                "change": {"operation": "retire_index"}}}})["draft"]
            applied = f.rpc(f.new / "airk.exe", {"catalog_apply": {
                "request": draft["request"], "preview_digest": draft["preview_digest"]}})
            assert applied["status"] == "catalog_applied", applied
        else:
            assert activated.returncode != 0 and "legacy_compatibility_recovery_required" in activated.stderr
            assert "ConfigurationUnavailable" in activated.stderr
            immutable = immutable_upgrade(f.control)
            receipt["immutable_original"] = immutable
            assert len(immutable["plan"]["indexes"]) == 2
            page = json.loads(f.admin("candidates").stdout)
            assert page["schema"] == 1 and page["total_eligible"] == 1
            assert page["continuation"] is None and len(page["items"]) == 1
            candidate = page["items"][0]
            assert candidate["registration"] == f.projects["a"]
            assert candidate["source_import"] == "pending_metadata"
            review = json.loads(f.admin("review", None, "--index-id", candidate["index_id"]).stdout)
            assert review["status"] == "review", review
            f.readonly_host_parity("review", review)
            request = review["preview"]["request"]
            crossed = json.loads(json.dumps(review))
            crossed["preview"]["request"]["registration"] = f.projects["b"]
            assert f.admin("authorize", {"preview": crossed["preview"],
                "review_digest": crossed["review_digest"]}, check=False).returncode != 0
            if args.scenario == "returned":
                # Same genuine directory identities, before confirmation: the
                # reviewed Unknown precondition has changed and MUST refuse.
                offline.rename(contained(f.root, ancestor, exists=False))
                assert (ancestor.stat().st_dev, ancestor.stat().st_ino) == identity
                assert f.admin("authorize", {"preview": review["preview"],
                    "review_digest": review["review_digest"]}, check=False).returncode != 0
                ancestor.rename(contained(f.root, offline, exists=False))
                review = json.loads(f.admin("review", None, "--index-id", candidate["index_id"]).stdout)
                request = review["preview"]["request"]
            authorized = json.loads(f.admin("authorize", {"preview": review["preview"],
                "review_digest": review["review_digest"]}).stdout)
            assert authorized["status"] == "authorized", authorized
            assert json.loads(f.admin("authorize", {"preview": review["preview"],
                "review_digest": review["review_digest"]}).stdout) == authorized
            assert immutable_upgrade(f.control) == immutable
            observed = json.loads(f.admin("receipt", request).stdout)
            assert observed["observation"] == authorized
            f.readonly_host_parity("authorized", authorized)
            if args.scenario == "returned":
                contained(f.root, offline).rename(contained(f.root, ancestor, exists=False))
                assert (ancestor.stat().st_dev, ancestor.stat().st_ino) == identity
            elif args.scenario == "foreign-return":
                data = contained(f.root, ancestor / "data", exists=False)
                data.mkdir(parents=True)
                (data / "foreign-sentinel").write_bytes(b"never-owned-by-old-index\n")
            applied = json.loads(f.admin("resume", request).stdout)
            assert applied["status"] == "applied", applied
            assert applied["receipt"]["cleanup"] == "pending"
            assert json.loads(f.admin("receipt", request).stdout)["observation"] == applied
            f.readonly_host_parity("applied", applied)
            assert json.loads(f.admin("resume", request).stdout) == applied
            assert immutable_upgrade(f.control) == immutable
            if args.scenario == "foreign-return":
                assert (ancestor / "data/foreign-sentinel").read_bytes() == b"never-owned-by-old-index\n"
            if args.scenario == "deleted":
                assert not ancestor.exists(), "Logical retirement recreated absent source/data"
        assert (f.root / "b/source/README.md").read_bytes() == original
        assert f.query(f.new / "airk.exe", f.projects["b"]) == before
        healthy_binding = f.rpc(f.new / "airk.exe", {"catalog_legacy_binding": {
            "project": f.projects["b"]["binding"], "expected_catalog_revision": None}})["observation"]
        assert healthy_binding["eligibility"]["kind"] == "imported_snapshot"
        assert healthy_binding["eligibility"]["registration"] == f.projects["b"]
        healthy_plan = immutable_upgrade(f.control)["plan"]
        b_index = next(index for index in healthy_plan["indexes"] if index["registration"] == f.projects["b"])
        memberships = healthy_binding["eligibility"]["grant"]["grant"]["memberships"]
        assert len(memberships) == 1 and memberships[0]["membership"]["index_id"] == b_index["index_id"]
        receipt["healthy_native_binding"] = healthy_binding
        if args.scenario == "foreign-return":
            assert (ancestor / "data/foreign-sentinel").read_bytes() == b"never-owned-by-old-index\n"
        if args.scenario == "deleted":
            assert not ancestor.exists(), "Healthy B read recreated retired A paths"
        with closing(sqlite3.connect((f.control / "service-1.4.sqlite3").as_uri() + "?mode=ro", uri=True)) as db:
            db.execute("PRAGMA query_only=ON")
            a = db.execute("SELECT registered,paused FROM projects WHERE binding=?", (f.projects["a"]["binding"],)).fetchone()
            assert a is None or a == (0, 1), "Accepted A retirement is not logical unregistration"
            assert db.execute("SELECT count(*) FROM service_jobs WHERE project=? AND state IN('pending','running')",
                (f.projects["a"]["binding"],)).fetchone()[0] == 0, "Retired A retains runnable work"
            row = db.execute("SELECT substr(payload,1,20481),registered FROM projects WHERE binding=?", (f.projects["b"]["binding"],)).fetchone()
            assert row is not None and row[1] == 1
            assert json.loads(row[0]) == f.projects["b"], "Healthy B registration changed"
        assert seal(f.old) == f.old_seal and seal(f.new) == f.new_seal
        receipt.update(status="passed", healthy_retained_projection_unchanged=before, healthy_source_unchanged=True)
    except BaseException as error:
        receipt["primary_error"] = repr(error)
    finally:
        if fixture is not None:
            # Retain even successful fixtures; quiescence is separate from physical
            # cleanup and from the scenario's original primary result.
            finish_owner(fixture, receipt)
        try:
            output_stream.write(json.dumps(receipt, indent=2) + "\n")
            output_stream.flush()
            os.fsync(output_stream.fileno())
        finally:
            output_stream.close()
    return 0 if receipt["status"] == "passed" else 1


if __name__ == "__main__":
    sys.exit(main())
