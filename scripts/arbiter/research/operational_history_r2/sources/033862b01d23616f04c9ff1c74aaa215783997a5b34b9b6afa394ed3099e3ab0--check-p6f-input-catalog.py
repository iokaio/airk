# SPDX-License-Identifier: Apache-2.0
"""Ignored owned-registration CLI/MCP catalog diagnostic; no indexing/activation.

Run only in the lead's queue against its exact native runtime inventory pin.
Strict comparisons preserve public project/text/help projection differences.
This does not qualify retrieval, scoped reads, installation or an editor.
"""
import argparse
import importlib.util
import json
import os
from pathlib import Path
import queue
import subprocess
import tempfile
import threading
import time

REPO = Path(__file__).resolve().parents[2]
SPEC = importlib.util.spec_from_file_location("catalog_parity_fixture", Path(__file__).with_name("check-p6f-scoped-parity.py"))
PARITY = importlib.util.module_from_spec(SPEC)
SPEC.loader.exec_module(PARITY)
MAX_FRAME = 4 * 1024 * 1024


class MetadataFixture(PARITY.CLIENTS.Fixture):
    """Canonical native schema-11 registration; no setup_watch/job request."""
    def __init__(self, runtime, owner, deadline):
        super().__init__(runtime, owner, command_timeout=20)
        self.deadline = deadline
        self.observations = []

    def run(self, *arguments, **kwargs):
        assert len(self.observations) < 32, "Metadata lifecycle call bound exceeded"
        check = kwargs.pop("check", True)
        kwargs["timeout"] = PARITY.CLIENTS.remaining(self.deadline, kwargs.get("timeout") or 20)
        response = super().run(*arguments, check=False, **kwargs)
        assert len(response.stdout.encode("utf-8")) <= MAX_FRAME, "Metadata lifecycle response bound exceeded"
        self.observations.append({"arguments": list(map(str, arguments)), "input": kwargs.get("input"),
                                  "exit_code": response.returncode, "stdout": response.stdout, "stderr": response.stderr[-8192:]})
        assert not check or response.returncode == 0, "Native metadata lifecycle call failed; actual response retained"
        return response

    def close_owned(self):
        cleanup = {"pid": self.child.pid if self.child is not None else None, "all_exited": True}
        try:
            if self.child is not None:
                if self.child.poll() is None:
                    try:
                        # Cleanup has a separate finite budget; it is not a
                        # mutation retry and targets only this owned service.
                        shutdown = PARITY.CLIENTS.Fixture.run(self, "service", "shutdown", "--runtime-dir", self.control,
                                                               check=False, timeout=5)
                        cleanup["shutdown"] = {"exit_code": shutdown.returncode, "stdout": shutdown.stdout, "stderr": shutdown.stderr[-8192:]}
                    except BaseException as error:
                        cleanup["shutdown_error"] = type(error).__name__
                try:
                    self.child.wait(timeout=5)
                except subprocess.TimeoutExpired:
                    self.child.kill()
                    self.child.wait(timeout=5)
                    cleanup["forced"] = True
                cleanup.update(exit_code=self.child.returncode, all_exited=self.child.poll() is not None)
        finally:
            self.log.close()
        return cleanup


def require_metadata_only_progress(value, binding):
    assert value["status"] == "progress" and value["project"] == binding
    assert value.get("observation") is None and value.get("pending_job") is None
    assert value["queued_jobs"] == [] and value["queue_truncated"] is False, "Metadata-only fixture unexpectedly admitted native work"


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("--runtime-dir", type=Path, required=True)
    parser.add_argument("--expected-runtime-sha256", required=True)
    parser.add_argument("--fixture-parent", type=Path, default=REPO / "target")
    parser.add_argument("--report", type=Path, required=True)
    args = parser.parse_args()
    runtime = PARITY.CLIENTS.runtime_root(args.runtime_dir)
    manifest_path = runtime / "airk-install.json"
    manifest_hash = PARITY.digest(manifest_path)
    assert manifest_hash == args.expected_runtime_sha256, "Runtime inventory differs from lead pin"
    inventory = json.loads(manifest_path.read_text(encoding="utf-8"))
    assert 1 <= len(inventory["files"]) <= 4096
    for name, expected in inventory["files"].items():
        artifact = runtime / name
        assert artifact.resolve(strict=True).is_relative_to(runtime) and not artifact.is_symlink()
        assert PARITY.digest(artifact) == expected, "Runtime pair bytes differ from native inventory"
    binary = runtime / ("airk.exe" if os.name == "nt" else "airk")
    if not binary.is_file():
        binary = runtime / "AIRK.app/Contents/MacOS/airk"
    started = time.monotonic()
    deadline = started + 60
    report = {"schema": 1, "fixture_kind": "owned_registered_native_cli_mcp_input_catalog_diagnostic", "status": "running",
              "runtime_manifest_sha256": manifest_hash, "script_sha256": PARITY.digest(Path(__file__)),
              "comparison_source_sha256": PARITY.digest(Path(PARITY.__file__)),
              "projection_comparator_sha256": PARITY.digest(Path(PARITY.CATALOG_PROJECTION.__file__)),
              "qualification": {"catalog_metadata_only": True, "owned_service_registration": True, "indexing": False, "scoped_reads": False,
                                "installer": False, "managed_activation": False, "installed_editor": False},
              "native_requests": [], "native_responses": [], "owned_cleanup": {"all_exited": False}}
    failure = None
    with tempfile.TemporaryDirectory(prefix="airk-input-catalog-", dir=args.fixture_parent) as temporary:
        owner = Path(temporary).resolve(strict=True)
        assert owner.parent == args.fixture_parent.resolve(strict=True) and owner.name.startswith("airk-input-catalog-")
        root, data = owner / "source", owner / "data"
        root.mkdir()
        data.mkdir()
        environment = {**os.environ, "AIRK_MCP_RESPONSE_MODE": "full", "HTTP_PROXY": "http://127.0.0.1:1", "HTTPS_PROXY": "http://127.0.0.1:1", "ALL_PROXY": "http://127.0.0.1:1"}
        fixture = MetadataFixture(runtime, owner, deadline)
        fixture.environment = environment
        report["metadata_lifecycle"] = fixture.observations
        child = None
        reader = None
        stderr_reader = None
        stderr_tail = bytearray()
        stderr_bytes = 0
        messages = queue.Queue(maxsize=4)
        def receive(identifier):
            for _ in range(8):
                kind, payload = messages.get(timeout=PARITY.CLIENTS.remaining(deadline, 20))
                if kind != "native":
                    raise AssertionError("Native MCP metadata reader failed: " + payload)
                report["native_responses"].append(payload)
                if payload.get("id") == identifier:
                    assert "error" not in payload, "Native MCP catalog RPC returned an error"
                    return payload
            raise AssertionError("Native MCP metadata response bound exceeded")
        def send(value):
            assert len(report["native_requests"]) < 3, "Static MCP request bound exceeded"
            report["native_requests"].append(value)
            child.stdin.write(json.dumps(value) + "\n")
            child.stdin.flush()
        def read_native():
            try:
                while True:
                    line = child.stdout.readline(MAX_FRAME + 1)
                    if not line:
                        messages.put(("reader", "EOF"))
                        return
                    if len(line.encode("utf-8")) > MAX_FRAME or not line.endswith("\n"):
                        messages.put(("reader", "frame bound"))
                        return
                    messages.put(("native", PARITY.safe_json(line)))
            except BaseException as error:
                messages.put(("reader", type(error).__name__))
        def read_native_stderr():
            nonlocal stderr_bytes
            while True:
                chunk = child.stderr.buffer.read(1024)
                if not chunk:
                    return
                stderr_bytes += len(chunk)
                stderr_tail.extend(chunk)
                del stderr_tail[:-8192]
        try:
            # Register with the real metadata owner. Register has no setup_watch
            # field and schedules no model/index job; no saved row is invented.
            fixture.start()
            report["owned_cleanup"]["service_pid"] = fixture.child.pid
            fixture.register("catalog-probe", root, data)
            registration = fixture.projects["catalog-probe"]
            report["native_registration"] = registration
            before = fixture.rpc({"progress": {"project": registration["binding"]}})
            report["progress_before"] = before
            require_metadata_only_progress(before, registration["binding"])
            common = [str(binary), *fixture.arguments("catalog-probe")]
            cli_command = [*common, "tools", "list", "--json"]
            report["cli_command"] = cli_command
            cli = subprocess.run(cli_command, cwd=owner, env=environment, capture_output=True, text=True,
                                 encoding="utf-8", timeout=PARITY.CLIENTS.remaining(deadline, 20),
                                 creationflags=getattr(subprocess, "CREATE_NO_WINDOW", 0))
            assert len(cli.stdout.encode("utf-8")) <= MAX_FRAME, "CLI catalog output bound exceeded"
            report["cli"] = {"exit_code": cli.returncode, "stdout": cli.stdout, "stderr": cli.stderr[-4096:]}
            assert cli.returncode == 0, "Actual CLI catalog process failed"
            native_catalog = PARITY.safe_json(cli.stdout)
            report["cli_catalog"] = native_catalog
            child = subprocess.Popen([*common, "mcp"], cwd=owner, env=environment,
                                     stdin=subprocess.PIPE, stdout=subprocess.PIPE, stderr=subprocess.PIPE,
                                     text=True, encoding="utf-8", creationflags=getattr(subprocess, "CREATE_NO_WINDOW", 0))
            report["owned_cleanup"]["mcp_pid"] = child.pid
            reader = threading.Thread(target=read_native, daemon=True)
            reader.start()
            stderr_reader = threading.Thread(target=read_native_stderr, daemon=True)
            stderr_reader.start()
            send({"jsonrpc": "2.0", "id": 1, "method": "initialize", "params": {
                "protocolVersion": "2025-11-25", "capabilities": {}, "clientInfo": {"name": "airk-native-check", "version": "fixture"}}})
            initialized = receive(1)
            assert "result" in initialized
            send({"jsonrpc": "2.0", "method": "notifications/initialized"})
            send({"jsonrpc": "2.0", "id": 2, "method": "tools/list", "params": {}})
            catalog_reply = receive(2)
            report["mcp_catalog"] = catalog_reply["result"]
            # BOTH complete catalogs are retained before strict comparisons.
            differences = PARITY.input_catalog_differences(native_catalog, catalog_reply["result"])
            report["strict_schema_differences"] = differences
            projection = PARITY.CATALOG_PROJECTION.compare(native_catalog, catalog_reply["result"])
            report["declared_projection"] = projection
            after = fixture.rpc({"progress": {"project": registration["binding"]}})
            report["progress_after"] = after
            require_metadata_only_progress(after, registration["binding"])
            assert PARITY.digest(manifest_path) == manifest_hash
            assert PARITY.digest(Path(__file__)) == report["script_sha256"]
            assert PARITY.digest(Path(PARITY.__file__)) == report["comparison_source_sha256"]
            assert PARITY.digest(Path(PARITY.CATALOG_PROJECTION.__file__)) == report["projection_comparator_sha256"]
            assert projection["passed"], "Native/public declared catalog projection mismatch: " + json.dumps(projection["differences"][0], ensure_ascii=False)
            report["status"] = "passed"
        except BaseException as error:
            failure = error
            report.update(status="failed", failure={"class": type(error).__name__, "message": str(error)[:2048]})
        finally:
            if child is not None:
                if child.stdin is not None:
                    try:
                        child.stdin.close()
                    except OSError:
                        pass
                try:
                    child.wait(timeout=5)
                except subprocess.TimeoutExpired:
                    child.kill()  # Only the exact child this fixture created.
                    child.wait(timeout=5)
                    report["owned_cleanup"]["forced"] = True
                report["owned_cleanup"].update(exit_code=child.returncode, all_exited=child.poll() is not None)
                if reader is not None:
                    reader.join(timeout=1)
                if stderr_reader is not None:
                    stderr_reader.join(timeout=1)
                report["mcp_stderr"] = {"tail": bytes(stderr_tail).decode("utf-8", "replace"),
                                         "total_bytes": stderr_bytes, "truncated": stderr_bytes > len(stderr_tail),
                                         "reader_exited": stderr_reader is None or not stderr_reader.is_alive()}
            else:
                report["owned_cleanup"]["all_exited"] = True
            if child is not None and (child.returncode != 0 or report["owned_cleanup"].get("forced")):
                report["status"] = "failed"
                report["cleanup_failure"] = "Owned MCP catalog process did not close normally"
                failure = failure or AssertionError(report["cleanup_failure"])
            service_cleanup = fixture.close_owned()
            report["owned_cleanup"]["service"] = service_cleanup
            report["owned_cleanup"]["all_exited"] = report["owned_cleanup"]["all_exited"] and service_cleanup["all_exited"]
            if service_cleanup.get("forced") or service_cleanup.get("exit_code", 0) != 0:
                report["status"] = "failed"
                report["service_cleanup_failure"] = "Owned metadata service did not close normally"
                failure = failure or AssertionError(report["service_cleanup_failure"])
            report["elapsed_seconds"] = round(time.monotonic() - started, 3)
            output = json.dumps(report, ensure_ascii=False, indent=2)
            assert len(output.encode("utf-8")) <= 32 * MAX_FRAME, "Static metadata diagnostic report exceeds bound"
            args.report.write_text(output + "\n", encoding="utf-8")
    print(json.dumps({"status": report["status"], "report": str(args.report), "differences": len(report.get("strict_schema_differences", []))}))
    if failure:
        raise SystemExit(1)


if __name__ == "__main__":
    main()
