# SPDX-License-Identifier: Apache-2.0
"""Offline two-project native service/MiniLM smoke test; never packages or installs a VSIX."""
import argparse
import json
import os
from pathlib import Path
import queue
import signal
import subprocess
import tempfile
import threading
import time
import uuid


class Bridge:
    def __init__(self, binary, runtime, binding, environment):
        self.process = subprocess.Popen(
            [str(binary), "bridge", "--runtime-dir", str(runtime), "--project", binding],
            stdin=subprocess.PIPE, stdout=subprocess.PIPE, stderr=subprocess.DEVNULL,
            env=environment, creationflags=getattr(subprocess, "CREATE_NO_WINDOW", 0))
        self.responses = queue.Queue(maxsize=4)
        self.sequence = 0

        def read():
            while True:
                line = self.process.stdout.readline(128 * 1024 + 1)
                if not line or len(line) > 128 * 1024:
                    return
                try:
                    self.responses.put_nowait(json.loads(line))
                except (ValueError, queue.Full):
                    return

        self.reader = threading.Thread(target=read, daemon=True)
        self.reader.start()
        self.call("initialize", {"protocolVersion": "2025-11-25", "capabilities": {},
                                "clientInfo": {"name": "airk-pool-original-fixture", "version": "1"}})
        self.send({"jsonrpc": "2.0", "method": "notifications/initialized"})

    def send(self, value):
        self.process.stdin.write((json.dumps(value) + "\n").encode("utf-8"))
        self.process.stdin.flush()

    def call(self, method, params):
        self.sequence += 1
        self.send({"jsonrpc": "2.0", "id": self.sequence, "method": method, "params": params})
        response = self.responses.get(timeout=120)
        if response.get("id") != self.sequence or "error" in response:
            raise RuntimeError("MCP bridge returned an invalid response")
        return response["result"]

    def search(self, mode):
        result = self.call("tools/call", {"name": "airk_search", "arguments": {
            "query": "copper beacon", "mode": mode, "max_bytes": 4096}})
        if result.get("isError") or "memo.md" not in json.dumps(result):
            raise RuntimeError(f"{mode} retrieval lost the original collection citation: {result}")

    def close(self):
        self.process.stdin.close()
        try:
            self.process.wait(timeout=5)
        except subprocess.TimeoutExpired:
            self.process.kill()
            self.process.wait(timeout=10)
        self.reader.join(timeout=5)
        self.process.stdout.close()


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("--runtime", type=Path, required=True)
    args = parser.parse_args()
    binary = args.runtime.resolve(strict=True) / ("airk.exe" if os.name == "nt" else "airk")
    environment = dict(os.environ, HTTP_PROXY="http://127.0.0.1:1",
                       HTTPS_PROXY="http://127.0.0.1:1", ALL_PROXY="http://127.0.0.1:1")
    flags = getattr(subprocess, "CREATE_NO_WINDOW", 0)

    def run(*arguments):
        result = subprocess.run([str(binary), *map(str, arguments)], capture_output=True,
                                env=environment, timeout=240, encoding="utf-8", creationflags=flags)
        if result.returncode:
            raise RuntimeError(result.stderr[-3000:])
        return json.loads(result.stdout)

    with tempfile.TemporaryDirectory(prefix="airk-pool-smoke-") as temporary:
        fixture = Path(temporary)
        runtime = fixture / "runtime"
        log = (fixture / "service.stderr").open("wb")
        service = subprocess.Popen([str(binary), "serve", "--runtime-dir", str(runtime)],
                                   stdin=subprocess.DEVNULL, stdout=subprocess.DEVNULL,
                                   stderr=log, env=environment, creationflags=flags)
        bridges = []

        def rpc(method):
            path = fixture / "request.json"
            path.write_text(json.dumps(method), encoding="utf-8")
            return run("service", "rpc", "--runtime-dir", runtime, "--request-file", path)

        try:
            deadline = time.monotonic() + 90
            while True:
                try:
                    if run("service", "health", "--runtime-dir", runtime)["status"] == "health":
                        break
                except RuntimeError:
                    pass
                if time.monotonic() >= deadline or service.poll() is not None:
                    raise RuntimeError("Native service did not become ready")
                time.sleep(0.1)

            projects = []
            originals = []
            for number in range(2):
                source = fixture / f"source-{number}"
                data = fixture / f"data-{number}"
                source.mkdir()
                data.mkdir()
                original = f"# Copper beacon {number}\nOriginal copper beacon maintenance fixture {number}.\n"
                document = source / "memo.md"
                document.write_text(original, encoding="utf-8")
                originals.append((document, document.read_bytes()))
                base = ["--root", source, "--data-dir", data, "--collection", f"pool-fixture-{number}"]
                run(*base, "collections", "vectors", "install-bundled")
                response = run(*base, "service", "register", "--runtime-dir", runtime)
                if response.get("status") != "registered":
                    raise RuntimeError(f"Project registration failed: {response}")
                projects.append(response["project"]["registration"])
            print("Offline provisioning and canonical registration passed for two projects.", flush=True)

            epoch = rpc("request_epoch")
            jobs = []
            for project in projects:
                caller = uuid.uuid4().hex
                accepted = rpc({"enqueue": {"project": project["binding"], "caller": caller,
                    "key": {"namespace": epoch["namespace"], "epoch": epoch["epoch"], "nonce": uuid.uuid4().hex},
                    "kind": "index"}})
                if accepted.get("status") != "job":
                    raise RuntimeError(f"Index job was not accepted: {accepted}")
                jobs.append(accepted["job"]["id"])
            deadline = time.monotonic() + 180
            while jobs:
                for job in list(jobs):
                    response = rpc({"job": {"id": job}})
                    state = response.get("job", {}).get("state")
                    if state == "succeeded":
                        jobs.remove(job)
                    elif state in ("failed", "canceled"):
                        raise RuntimeError(f"Supervised indexing failed: {response}")
                if time.monotonic() >= deadline:
                    raise RuntimeError("Supervised indexing did not complete within the fixture budget")
                time.sleep(0.2)

            for project in projects:
                bridge = Bridge(binary, runtime, project["binding"], environment)
                bridges.append(bridge)
                bridge.search("hybrid")
            observation = rpc("embedding_status")
            if (not observation.get("available") or observation.get("loads") != 1
                    or observation.get("reuses", 0) < 1):
                raise RuntimeError(f"Projects did not share one verified MiniLM load: {observation}")
            print("Two supervised projects reused one MiniLM session and returned hybrid citations.", flush=True)

            # This PID comes from our private authenticated service's supervised helper.
            os.kill(observation["process_id"], signal.SIGTERM)
            time.sleep(0.3)
            for bridge in bridges:
                bridge.search("lexical")
            if run("service", "health", "--runtime-dir", runtime)["status"] != "health":
                raise RuntimeError("Embedding failure blocked independent service health")
            if any(document.read_bytes() != original for document, original in originals):
                raise RuntimeError("Native service changed an original source document")
            print("Embedding-process failure preserved lexical retrieval, service health and original source bytes.", flush=True)
        finally:
            for bridge in bridges:
                bridge.close()
            if service.poll() is None:
                try:
                    run("service", "shutdown", "--runtime-dir", runtime)
                    service.wait(timeout=60)
                except (RuntimeError, subprocess.TimeoutExpired):
                    service.kill()
                    service.wait(timeout=10)
            log.close()


if __name__ == "__main__":
    main()
