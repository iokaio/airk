# SPDX-License-Identifier: Apache-2.0
"""Exercise an exact native service pair in private, disposable project scopes."""
import argparse
from contextlib import contextmanager
import errno
import hashlib
import json
import os
from pathlib import Path
import queue
import subprocess
import tempfile
import threading
import time
import uuid
import sys

TOOLS = {"airk_" + name for name in (
    "capabilities", "changes", "code_facts", "context", "document_facts", "file_info", "help", "imports",
    "list_files", "outline", "read_file", "references", "relationships", "search", "status", "symbols")}


def tool_value(envelope):
    assert isinstance(envelope, dict) and isinstance(envelope.get("content"), list), envelope
    assert len(envelope["content"]) == 1 and envelope["content"][0].get("type") == "text", "Invalid native text envelope"
    value = json.loads(envelope["content"][0]["text"])
    assert isinstance(value, dict), value
    return value


def unsubmitted_busy(envelope):
    if not isinstance(envelope, dict) or envelope.get("isError") is not True:
        return False
    try:
        error = tool_value(envelope)["error"]
        if not isinstance(error, dict):
            return False
        code = error.get("code")
        return (code in ("direct_query_admission", "direct_query_setup") and error.get("reason") == "busy"
                and error.get("query_submitted") is False and error.get("retryable") is True
                and error.get("fresh_brokerage_required") is (code == "direct_query_setup"))
    except (AssertionError, AttributeError, IndexError, KeyError, TypeError, ValueError):
        return False


def read_retry_delay(envelope):
    if not isinstance(envelope, dict):
        return None
    if unsubmitted_busy(envelope):
        delay = tool_value(envelope)["error"].get("retry_after_ms")
        return delay / 1000 if type(delay) is int and 1 <= delay <= 1000 else None
    if envelope.get("isError") is not True:
        return None
    try:
        hint = tool_value(envelope)
        delay = hint.get("retry_after_ms")
        if (hint.get("status") == "retry" and hint.get("retryable") is True
                and hint.get("action") == "repeat_same_request"
                and hint.get("code") in ("publication_busy", "selection_busy")
                and type(delay) is int and 1 <= delay <= 1000):
            return delay / 1000
    except (AssertionError, AttributeError, IndexError, KeyError, TypeError, ValueError):
        pass
    return None


class ObservationTimeout(AssertionError):
    pass


def remaining(deadline, ceiling):
    budget = min(ceiling, deadline - time.monotonic())
    if budget <= 0:
        raise ObservationTimeout("Native observation deadline exhausted")
    return budget


def read_envelope(read, deadline, attempts=41, admission_only=False):
    """Only complete typed rejection authorizes exact read replay; never transport errors."""
    for attempt in range(attempts):
        remaining(deadline, 120)
        envelope = read(deadline)
        remaining(deadline, 120)
        assert isinstance(envelope, dict), "Native read returned no object"
        delay = read_retry_delay(envelope)
        if admission_only and not unsubmitted_busy(envelope):
            delay = None
        if delay is None or attempt + 1 == attempts or time.monotonic() + delay >= deadline:
            if envelope.get("isError") is True:
                # Contract codes only; arbitrary error text can contain source paths.
                code = "unknown"
                try:
                    value = tool_value(envelope)
                    candidate = value.get("code") if value.get("status") == "retry" else value.get("error", {}).get("code")
                    if candidate in ("publication_busy", "selection_busy", "direct_query_admission", "direct_query_setup"):
                        code = candidate
                except (AssertionError, AttributeError, KeyError, TypeError, ValueError):
                    pass
                raise AssertionError(f"Native read rejected; no further safe replay within budget: code={code}")
            return envelope, attempt
        time.sleep(delay)
    raise AssertionError("Native read attempt budget exhausted")


def status_summary(value):
    """Content-free diagnostics: never print source/error messages or process paths."""
    obj = lambda item: item if isinstance(item, dict) else {}
    work, runtime = obj(value.get("work")), obj(value.get("runtime"))
    worker, service = obj(work.get("worker")), obj(runtime.get("service"))
    state, code = value.get("index_state"), obj(value.get("index_error")).get("code")
    flag = lambda item: item if type(item) is bool else None
    return {"index_state": state if state in ("available", "unavailable") else "invalid",
            "code": code if code in ("index_status_unavailable", "index_worker_unavailable") else "unknown",
            "service_responsive": flag(service.get("responsive")), "service_accepting": flag(service.get("accepting")),
            "service_observation_available": bool(service) and "service_error" in runtime and runtime["service_error"] is None,
            "worker_observed": worker.get("identity") is not None,
            "automatic_start_blocked": flag(worker.get("automatic_start_blocked")),
            "metadata_samples_complete": work.get("failed_observations") == []}


def publication_pending(value, collection, project):
    """Validate observations, not retry hints. Unavailable can conceal a permanent cause."""
    assert isinstance(value, dict), "Native status is not an object"
    summary = status_summary(value)
    if value.get("index_state") == "available":
        assert (value.get("collection") == collection and isinstance(value.get("publication"), dict)
                and value["publication"].get("collection") == collection
                and isinstance(value["publication"].get("token"), str) and value["publication"]["token"]
                and type(value.get("files")) is int and value["files"] >= 0), summary
        return False
    error = value.get("index_error")
    assert (value.get("index_state") == "unavailable" and isinstance(error, dict)
            and isinstance(error.get("message"), str)
            and all(key in value and value[key] is None for key in ("publication", "files", "chunks"))), summary
    runtime, work = value.get("runtime"), value.get("work")
    assert isinstance(runtime, dict) and isinstance(work, dict), summary
    service, worker = runtime.get("service"), work.get("worker")
    assert isinstance(service, dict) and isinstance(worker, dict), summary
    if error.get("code") == "index_status_unavailable":
        assert value.get("collection") == collection, summary
    else:
        assert error.get("code") == "index_worker_unavailable" and value.get("project") == project, summary
        assert work.get("serving_worker_unavailable") is True, summary
        assert isinstance(runtime.get("index_worker"), dict) and runtime["index_worker"].get("process") is None, summary
    assert ("service_error" in runtime and runtime["service_error"] is None and service.get("responsive") is True
            and service.get("accepting") is True and work.get("basis") == "instance_bound_service_metadata"
            and work.get("failed_observations") == [] and worker.get("automatic_start_blocked") is False), summary
    identity = worker.get("identity")
    assert identity is None or (isinstance(identity, dict) and identity.get("project") == project
                                and isinstance(service.get("instance"), str) and identity.get("service") == service["instance"]), summary
    return True


def semantic_readiness(status):
    readiness = status.get("semantic_readiness")
    assert (isinstance(readiness, dict) and readiness.get("state") in ("disabled", "pending", "partial", "ready", "failed")
            and type(readiness.get("completed_artifacts")) is int and readiness["completed_artifacts"] >= 0), "Invalid semantic readiness observation"
    return readiness


def observe_publication(read, collection, project, deadline):
    last = None
    announced = False
    while time.monotonic() < deadline:
        try:
            value = read(deadline)
            remaining(deadline, 120)
        except (ObservationTimeout, subprocess.TimeoutExpired, queue.Empty):
            raise ObservationTimeout(f"Publication observation deadline exhausted; cause remains unknown: {last}") from None
        if not publication_pending(value, collection, project):
            return value
        last = status_summary(value)
        if not announced:
            print("Waiting for a publication observation: " + json.dumps(last), file=sys.stderr, flush=True)
            announced = True
        time.sleep(min(0.025, max(0, deadline - time.monotonic())))
    raise ObservationTimeout(f"Publication observation deadline exhausted; cause remains unknown: {last}")


def runtime_root(directory):
    root = directory.resolve(strict=True)
    if root.parts[-3:] == ("AIRK.app", "Contents", "MacOS"):
        root = root.parents[2]
    if not root.is_dir():
        raise ValueError("Expected a native runtime directory")
    return root


class Fixture:
    def __init__(self, runtime, owner, command_timeout=120, job_timeout=180):
        self.runtime = runtime_root(runtime)
        self.owner = owner.resolve(strict=True)
        self.binary = self.runtime / ("airk.exe" if os.name == "nt" else "airk")
        if not self.binary.is_file():
            self.binary = self.runtime / "AIRK.app/Contents/MacOS/airk"
        # Child commands run from the runtime directory, not the caller's cwd.
        # Bind every fixture path before crossing that process boundary.
        self.control = self.owner / "control"
        self.projects = {}
        self.environment = {**os.environ, "AIRK_MCP_RESPONSE_MODE": "full"}
        self.log = (self.owner / "service.stderr").open("wb")
        self.child = None
        self.command_timeout = command_timeout
        self.job_timeout = job_timeout

    def __enter__(self):
        try:
            self.start()
        except BaseException:
            self.close()
            raise
        return self

    def __exit__(self, *_):
        self.close()

    def run(self, *args, input=None, check=True, timeout=None):
        result = subprocess.run([str(self.binary), *map(str, args)], input=input, capture_output=True,
                                text=True, encoding="utf-8", timeout=min(self.command_timeout, timeout) if timeout is not None else self.command_timeout, cwd=self.runtime, env=self.environment,
                                creationflags=subprocess.CREATE_NO_WINDOW if os.name == "nt" else 0)
        if check and result.returncode:
            raise AssertionError(f"Native client failed ({result.returncode}): {result.stderr[-4096:]} {result.stdout[-4096:]}")
        return result

    def start(self):
        self.child = subprocess.Popen([str(self.binary), "serve", "--runtime-dir", str(self.control)],
                                      stdin=subprocess.DEVNULL, stdout=subprocess.DEVNULL, stderr=self.log,
                                      cwd=self.runtime, env=self.environment,
                                      creationflags=subprocess.CREATE_NO_WINDOW if os.name == "nt" else 0)
        deadline = time.monotonic() + 90
        while time.monotonic() < deadline:
            if self.run("service", "health", "--runtime-dir", self.control, check=False).returncode == 0:
                return
            if self.child.poll() is not None:
                raise AssertionError("Owned native fixture exited during startup")
            time.sleep(0.05)
        raise AssertionError("Owned native service did not become healthy")

    def close(self):
        try:
            if self.child is not None:
                if self.child.poll() is None:
                    self.run("service", "shutdown", "--runtime-dir", self.control, check=False)
                try:
                    self.child.wait(timeout=45)
                except subprocess.TimeoutExpired:
                    # Only this Popen child can be terminated; never discover/kill a PID.
                    self.child.kill()
                    self.child.wait(timeout=15)
                    raise AssertionError("Owned service failed to drain")
        finally:
            self.log.close()

    def rpc(self, method):
        value = json.loads(self.run("service", "rpc", "--runtime-dir", self.control, "--stdin", input=json.dumps(method)).stdout)
        if value.get("status") in ("failure", "incompatible"):
            raise AssertionError(f"Native metadata refused: {value}")
        return value

    def register(self, name, root, data):
        root, data = Path(root).resolve(), Path(data).resolve()
        prepared = json.loads(self.run("--root", root, "--data-dir", data, "--collection", name,
                                       "service", "prepare-registration").stdout)
        observed = self.rpc({"register": {"project": prepared}})
        assert observed["project"]["registration"] == prepared
        self.projects[name] = prepared

    def arguments(self, name):
        project = self.projects[name]
        return ["--service-runtime-dir", str(self.control), "--service-project", project["binding"],
                "--root", project["root"], "--data-dir", project["data_root"], "--collection", project["collection"]]

    def tool(self, project, tool, arguments=None):
        return self.query(project, "tools", "call", tool, "--args",
                          json.dumps(arguments or {"response_mode": "full"}))

    def published_status(self, project, deadline=None):
        deadline = min(deadline if deadline is not None else float("inf"), time.monotonic() + 90)
        registration = self.projects[project]
        return observe_publication(lambda until: self.query(project, "tools", "call", "airk_status", "--args",
                                                           '{"response_mode":"full"}', deadline=until),
                                   registration["collection"], registration["binding"], deadline)

    @contextmanager
    def hold_publication(self, project):
        """Fixture-only writer exclusion for exact single-attempt usage measurements.

        Match FileStore::lock's advisory file, not its legacy writer.lock marker.
        No index data or marker bytes are written. Never use outside this owned fixture.
        """
        registered = self.projects[project]
        data = Path(registered["data_root"]).resolve(strict=True)
        assert any(parent.samefile(self.owner) for parent in data.parents), "Measurement storage is outside the owned fixture"
        lock = data / "collections-1.4" / ".writer-locks" / registered["collection"]
        assert lock.is_file() and lock.resolve(strict=True) == lock, "Unexpected fixture writer-lock topology"
        for component in (lock, *lock.parents):
            assert not component.is_symlink() and not getattr(component.lstat(), "st_file_attributes", 0) & 0x400, "Linked fixture writer-lock path"
            if component.samefile(self.owner):
                break
        with lock.open("r+b") as stream:
            if os.name == "nt":
                import msvcrt
                def acquire():
                    stream.seek(0)
                    msvcrt.locking(stream.fileno(), msvcrt.LK_NBLCK, 1)
                def release():
                    stream.seek(0)
                    msvcrt.locking(stream.fileno(), msvcrt.LK_UNLCK, 1)
            else:
                import fcntl
                def acquire():
                    fcntl.flock(stream.fileno(), fcntl.LOCK_EX | fcntl.LOCK_NB)
                def release():
                    fcntl.flock(stream.fileno(), fcntl.LOCK_UN)
            deadline = time.monotonic() + 5
            while True:
                try:
                    acquire()
                    break
                except OSError as error:
                    if error.errno not in (errno.EACCES, errno.EAGAIN) or time.monotonic() >= deadline:
                        raise
                    time.sleep(min(0.005, remaining(deadline, 5)))
            try:
                yield
            finally:
                release()

    def query(self, project, *arguments, deadline=None, retry=True, admission_only=False):
        deadline = min(deadline if deadline is not None else float("inf"), time.monotonic() + 90)
        assert (arguments[:2] == ("tools", "call") and len(arguments) >= 3 and arguments[2] in TOOLS
                or arguments and arguments[0] in ("status", "search", "files", "read", "info", "outline", "symbol",
                                                  "imports", "facts", "refs", "context", "changes", "code-facts", "relationships")), "Expected a read-only fixture query"
        exact = (*self.arguments(project), *arguments)
        def read(until):
            result = self.run(*exact, check=False, timeout=remaining(until, 90))
            assert result.returncode in (0, 1), f"Native read process failed: exit {result.returncode}"
            envelope = json.loads(result.stdout)
            assert result.returncode == 0 or read_retry_delay(envelope) is not None, "Native read failed without a typed rejection"
            return envelope
        envelope, _ = read_envelope(read, deadline, 41 if retry or admission_only else 1, admission_only)
        return tool_value(envelope)

    def wait(self, receipt, expected="succeeded"):
        deadline = time.monotonic() + self.job_timeout
        while time.monotonic() < deadline:
            job = self.rpc({"job": {"id": receipt["job"]["id"]}})["job"]
            if job["state"] in ("succeeded", "failed", "canceled"):
                assert job["state"] == expected, job
                return job
            time.sleep(0.05)
        raise AssertionError("Native job did not finish")

    def job(self, project, command, expected="succeeded"):
        return self.wait(json.loads(self.run(*self.arguments(project), command).stdout), expected)

    def enqueue(self, project, kind):
        epoch = self.rpc("request_epoch")
        return self.rpc({"enqueue": {"project": self.projects[project]["binding"], "kind": kind,
                                     "caller": uuid.uuid4().hex, "key": {"namespace": epoch["namespace"],
                                     "epoch": epoch["epoch"], "nonce": uuid.uuid4().hex}}})

    def configure(self, project, draft, expected="succeeded"):
        # Inspection can queue behind a large scan. Follow its durable receipt
        # using this campaign's deadline instead of the interactive CLI wait.
        self.wait(self.enqueue(project, "inspect"))
        return self.submit_configuration(project, draft, expected)

    def configure_inactive(self, project, draft, expected="succeeded"):
        assert draft.get("operation") in ("prepare_derived_cleanup", "delete_derived")
        return self.submit_configuration(project, draft, expected)

    def submit_configuration(self, project, draft, expected="succeeded"):
        path = self.control.parent / f"cleanup-draft-{uuid.uuid4().hex}.json"
        with path.open("x", encoding="utf-8") as stream:
            json.dump(draft, stream)
        try:
            prepared = json.loads(self.run("service", "requests", "--runtime-dir", self.control,
                                          "prepare-configuration", "--project", self.projects[project]["binding"],
                                          "--file", path).stdout)
        finally:
            path.unlink()
        assert prepared["submitted"] is False
        # One exact saved request submission; an unknown outcome is never re-created.
        receipt = json.loads(self.run("service", "requests", "--runtime-dir", self.control,
                                      "retry", prepared["prepared"]).stdout)
        return self.wait(receipt, expected)

    def stdio(self, name):
        child = subprocess.Popen([str(self.binary), *self.arguments(name), "mcp"], cwd=self.runtime,
                                 stdin=subprocess.PIPE, stdout=subprocess.PIPE, stderr=subprocess.DEVNULL,
                                 text=True, encoding="utf-8", env=self.environment,
                                 creationflags=subprocess.CREATE_NO_WINDOW if os.name == "nt" else 0)
        responses = queue.Queue()
        threading.Thread(target=lambda: [responses.put(line) for line in child.stdout], daemon=True).start()
        def send(value):
            child.stdin.write(json.dumps(value) + "\n")
            child.stdin.flush()
        def receive(identifier, deadline=None):
            deadline = min(deadline if deadline is not None else float("inf"), time.monotonic() + 60)
            for _ in range(16):
                value = json.loads(responses.get(timeout=remaining(deadline, 60)))
                if value.get("id") == identifier:
                    assert "error" not in value, "Native stdio protocol error"
                    return value
            raise AssertionError("Matching stdio response missing")
        try:
            send({"jsonrpc": "2.0", "id": 1, "method": "initialize", "params": {
                "protocolVersion": "2025-11-25", "capabilities": {}, "clientInfo": {"name": "airk-native-check", "version": "1"}}})
            initialized = receive(1)["result"]
            assert initialized["serverInfo"]["name"] == "airk" and "response_mode=full" in initialized["instructions"]
            send({"jsonrpc": "2.0", "method": "notifications/initialized"})
            send({"jsonrpc": "2.0", "id": 2, "method": "tools/list"})
            catalog = receive(2)["result"]["tools"]
            assert len(catalog) == len(TOOLS) and {item["name"] for item in catalog} == TOOLS
            assert all(item["annotations"]["readOnlyHint"] for item in catalog)
            request = {"jsonrpc": "2.0", "id": 3, "method": "tools/call", "params": {
                "name": "airk_search", "arguments": {"query": "polaronlymarker", "mode": "lexical", "response_mode": "full"}}}
            def read(until):
                send(request)
                return receive(3, until)["result"]
            result, _ = read_envelope(read, time.monotonic() + 60)
            assert json.loads(result["content"][0]["text"])["hits"]
        finally:
            child.stdin.close()
            try:
                child.wait(timeout=15)
            except subprocess.TimeoutExpired:
                child.kill()
                child.wait(timeout=15)


def qualify(runtime, files, model):
    runtime = runtime_root(runtime)
    started = time.monotonic()
    with tempfile.TemporaryDirectory(prefix="airk-service-clients-") as temporary:
        owner = Path(temporary)
        data = owner / "data"
        data.mkdir()
        roots = [owner / name for name in ("project-a", "project-b")]
        original = {}
        for ordinal, root in enumerate(roots):
            root.mkdir()
            for number in range(files):
                path = root / f"fixture_{number}.py"
                marker = "polaronlymarker" if ordinal == 0 else "tropicalonlymarker"
                text = (f"def {'polar_fixture' if ordinal == 0 else 'other_fixture'}_{number}():\n"
                        f'    """{marker}: Study local temperature observations for station {number}.\n'
                        '    Retain original measurements and their source identity before computing averages.\n'
                        '    This original fixture exercises bounded extraction and offline semantic indexing.\n'
                        '    """\n'
                        f"    return {number}\n")
                path.write_text(text, encoding="utf-8")
                original[path] = hashlib.sha256(path.read_bytes()).hexdigest()
        fixture = Fixture(runtime, owner)
        try:
            fixture.start()
            for name, root in zip(("project-a", "project-b"), roots):
                fixture.register(name, root, data)
                fixture.job(name, "index")
            a = fixture.tool("project-a", "airk_search", {"query": "polaronlymarker", "mode": "lexical", "response_mode": "full"})
            b = fixture.tool("project-b", "airk_search", {"query": "polaronlymarker", "mode": "lexical", "response_mode": "full"})
            assert a["hits"] and not b["hits"]
            terse = fixture.tool("project-a", "airk_outline", {"path": "fixture_0.py", "response_mode": "terse"})
            full = fixture.tool("project-a", "airk_outline", {"path": "fixture_0.py", "response_mode": "full", "compact": False})
            assert terse["items"][0]["name"] == full["items"][0]["name"] == "polar_fixture_0"
            fixture.stdio("project-a")
            offline = owner / "temporarily-offline"
            roots[0].rename(offline)
            try:
                retained = fixture.tool("project-a", "airk_read_file", {"path": "fixture_0.py", "start_line": 1, "end_line": 2, "response_mode": "full"})
                assert "polar_fixture_0" in retained["hits"][0]["text"]
                rejected = fixture.job("project-a", "index", expected="failed")
                assert rejected["outcome"]["code"] == "source_unavailable"
            finally:
                offline.rename(roots[0])
            if model:
                fixture.job("project-a", "setup-model")
                fixture.job("project-a", "index")
                deadline = time.monotonic() + 180
                while True:
                    status = fixture.published_status("project-a", deadline)
                    readiness = semantic_readiness(status)
                    assert readiness["state"] != "failed", "Semantic work explicitly failed"
                    if readiness["state"] == "ready" and readiness["completed_artifacts"] == files:
                        break
                    assert time.monotonic() < deadline, readiness
                    time.sleep(0.1)
                vector = fixture.tool("project-a", "airk_search", {"query": "polar fixture", "mode": "vector", "response_mode": "full"})
                assert vector["hits"]
            assert all(hashlib.sha256(path.read_bytes()).hexdigest() == expected for path, expected in original.items())
            report = {"schema": 1, "version": json.loads((runtime / "airk-runtime.json").read_text(encoding="utf-8"))["version"],
                      "runtime_manifest_sha256": hashlib.sha256((runtime / "airk-install.json").read_bytes()).hexdigest(),
                      "projects": 2, "files_per_project": files, "tools": len(TOOLS), "model_checked": model,
                      "terse_bytes": len(json.dumps(terse).encode()), "full_bytes": len(json.dumps(full).encode()),
                      "elapsed_seconds": round(time.monotonic() - started, 3), "source_bytes_preserved": True}
        finally:
            fixture.close()
        return report


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("--runtime-dir", type=Path, required=True)
    parser.add_argument("--files", type=int, default=16)
    parser.add_argument("--model", action="store_true")
    parser.add_argument("--report", type=Path)
    args = parser.parse_args()
    if not 1 <= args.files <= 2048:
        parser.error("files must be 1..2048")
    report = qualify(args.runtime_dir.resolve(strict=True), args.files, args.model)
    if args.report:
        args.report.parent.mkdir(parents=True, exist_ok=True)
        args.report.write_text(json.dumps(report, indent=2) + "\n", encoding="utf-8")
    print(json.dumps(report, indent=2))


if __name__ == "__main__":
    main()
