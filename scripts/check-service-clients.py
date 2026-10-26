# SPDX-License-Identifier: Apache-2.0
"""Exercise an exact native service pair in private, disposable project scopes."""
import argparse
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

TOOLS = {"airk_" + name for name in (
    "capabilities", "changes", "code_facts", "context", "document_facts", "file_info", "help", "imports",
    "list_files", "outline", "read_file", "references", "relationships", "search", "status", "symbols")}


def tool_value(envelope):
    assert isinstance(envelope, dict) and isinstance(envelope.get("content"), list), envelope
    assert len(envelope["content"]) == 1 and envelope["content"][0].get("type") == "text", envelope
    value = json.loads(envelope["content"][0]["text"])
    assert isinstance(value, dict), value
    return value


def unsubmitted_busy(envelope):
    if envelope.get("isError") is not True:
        return False
    try:
        error = tool_value(envelope)["error"]
        return (error.get("code") in ("direct_query_admission", "direct_query_setup") and error.get("reason") == "busy"
                and error.get("query_submitted") is False)
    except (AssertionError, KeyError, TypeError, ValueError):
        return False


def read_retry_delay(envelope):
    if unsubmitted_busy(envelope):
        delay = tool_value(envelope)["error"].get("retry_after_ms", 25)
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
    except (AssertionError, KeyError, TypeError, ValueError):
        pass
    return None


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
        self.binary = self.runtime / ("airk.exe" if os.name == "nt" else "airk")
        if not self.binary.is_file():
            self.binary = self.runtime / "AIRK.app/Contents/MacOS/airk"
        self.control = owner / "control"
        self.projects = {}
        self.environment = {**os.environ, "AIRK_MCP_RESPONSE_MODE": "full"}
        self.log = (owner / "service.stderr").open("wb")
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

    def run(self, *args, input=None, check=True):
        result = subprocess.run([str(self.binary), *map(str, args)], input=input, capture_output=True,
                                text=True, encoding="utf-8", timeout=self.command_timeout, cwd=self.runtime, env=self.environment,
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

    def query(self, project, *arguments):
        deadline = time.monotonic() + 90
        for attempt in range(41):
            result = self.run(*self.arguments(project), *arguments, check=False)
            envelope = json.loads(result.stdout)
            delay = read_retry_delay(envelope)
            if delay is None or attempt == 40 or time.monotonic() + delay >= deadline:
                assert result.returncode == 0 and not envelope.get("isError"), (result.stderr, envelope)
                return tool_value(envelope)
            # Exact read arguments/revision survive; unknown outcomes never replay.
            time.sleep(delay)
        raise AssertionError("Native query admission exhausted")

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
        def receive(identifier):
            for _ in range(16):
                value = json.loads(responses.get(timeout=60))
                if value.get("id") == identifier:
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
            send({"jsonrpc": "2.0", "id": 3, "method": "tools/call", "params": {
                "name": "airk_search", "arguments": {"query": "polaronlymarker", "mode": "lexical", "response_mode": "full"}}})
            result = receive(3)["result"]
            assert not result.get("isError"), result
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
                    status = fixture.tool("project-a", "airk_status")
                    readiness = status["semantic_readiness"]
                    if readiness["completed_artifacts"] == files:
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
