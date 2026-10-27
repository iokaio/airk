# SPDX-License-Identifier: Apache-2.0
"""Exercise installed console lifecycle and crash recovery in one owned offline fixture; never build a VSIX."""
import argparse
from contextlib import closing
import hashlib
import importlib.util
import json
import os
from pathlib import Path
import sqlite3
import subprocess
import tempfile
import time
import uuid


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("--runtime", type=Path, required=True)
    args = parser.parse_args()
    payload = args.runtime.resolve(strict=True)
    binary = payload / ("airk.exe" if os.name == "nt" else "airk")
    if not binary.is_file():
        binary = payload / "AIRK.app/Contents/MacOS/airk"
    with (payload / "airk-install.json").open("rb") as stream:
        identity = hashlib.file_digest(stream, "sha256").hexdigest()
    spec = importlib.util.spec_from_file_location("airk_pool_fixture", Path(__file__).with_name("check-service-pool.py"))
    pool = importlib.util.module_from_spec(spec)
    spec.loader.exec_module(pool)
    environment = dict(os.environ, HTTP_PROXY="http://127.0.0.1:1", HTTPS_PROXY="http://127.0.0.1:1", ALL_PROXY="http://127.0.0.1:1")
    flags = getattr(subprocess, "CREATE_NO_WINDOW", 0)

    def invoke(*arguments, body=None, rejected=False, timeout=180):
        encoded = body if isinstance(body, str) else None if body is None else json.dumps(body)
        result = subprocess.run([str(binary), *map(str, arguments)], input=encoded,
                                capture_output=True, encoding="utf-8", env=environment, timeout=timeout, creationflags=flags)
        if rejected:
            if result.returncode == 0:
                raise RuntimeError("Expected an instance, stop-policy or key rejection")
            return None
        if result.returncode:
            raise RuntimeError(result.stderr[-4096:])
        return json.loads(result.stdout)

    fixture_parent = (Path(__file__).resolve().parents[1] / "target").resolve(strict=True)
    with tempfile.TemporaryDirectory(prefix="airk-console-fixture-", dir=fixture_parent) as temporary:
        fixture = Path(temporary).resolve()
        if fixture.parent != fixture_parent or not fixture.name.startswith("airk-console-fixture-"):
            raise RuntimeError("Fixture is outside its intended workspace target directory")
        install, state = fixture / "install", fixture / "state"
        control = state / "control"
        bridges = []
        command = None

        def runtime(action, *arguments, **kwargs):
            return invoke("runtime", action, "--install-dir", install, *arguments, **kwargs)

        def rpc(method, *, deadline=None):
            return pool.clients.metadata_rpc(lambda exact, until: invoke(
                "service", "rpc", "--runtime-dir", control, "--stdin", body=exact,
                timeout=pool.clients.remaining(until, 90)), method, deadline)

        def prepared(action, instance):
            epoch = rpc("request_epoch")
            return {"action": action, "caller": uuid.uuid4().hex, "expected_instance": instance, "runtime": identity,
                    "key": {"namespace": epoch["namespace"], "epoch": epoch["epoch"], "nonce": uuid.uuid4().hex}}

        def close_bridges():
            while bridges:
                bridges.pop().close()

        try:
            runtime("stage", "--source", payload, "--approved-sha256", identity, "--verification-basis", "trusted-distribution", "--state-dir", state)
            runtime("activate", "--runtime-sha256", identity)
            first = runtime("ensure")
            namespace = rpc("request_epoch")["namespace"]
            projects, jobs, originals = [], [], []
            for number in range(2):
                source, data = fixture / f"source-{number}", fixture / f"data-{number}"
                source.mkdir()
                data.mkdir()
                document = source / "memo.md"
                document.write_text(f"# Copper beacon {number}\nOriginal copper beacon maintenance fixture {number}.\n", encoding="utf-8")
                originals.append((document, document.read_bytes()))
                registration = invoke("--root", source, "--data-dir", data, "--collection", f"console-fixture-{number}",
                                      "service", "register", "--runtime-dir", control)["project"]["registration"]
                projects.append(registration)
                epoch = rpc("request_epoch")
                request = {"enqueue": {"project": projects[-1]["binding"], "kind": "index", "caller": uuid.uuid4().hex,
                                       "key": {"namespace": epoch["namespace"], "epoch": epoch["epoch"], "nonce": uuid.uuid4().hex}}}
                job = rpc(request)["job"]
                deadline = time.monotonic() + 90
                while True:
                    current = rpc({"job": {"id": job["id"]}}, deadline=deadline)["job"]
                    if current["state"] == "succeeded":
                        break
                    if current["state"] in ("failed", "canceled") or time.monotonic() >= deadline:
                        raise RuntimeError("Original source job did not complete")
                    time.sleep(0.1)
                jobs.append(current)
                bridge = pool.Bridge(binary, control, projects[-1]["binding"], environment, projects[-1]["collection"])
                bridges.append(bridge)
                bridge.search("lexical")
            print("Installed two-project direct evidence and independent source receipts passed.", flush=True)

            request = prepared("restart", first["instance"])
            receipt = runtime("lifecycle", body=request)["record"]
            close_bridges()
            second = runtime("ensure")
            if receipt["phase"] != "complete" or second["instance"] == first["instance"]:
                raise RuntimeError("Restart did not complete a new service incarnation")
            runtime("lifecycle", body=request)
            if runtime("ensure")["instance"] != second["instance"] or rpc("request_epoch")["namespace"] != namespace:
                raise RuntimeError("Exact restart replay restarted again or changed the registry namespace")
            stale = prepared("restart", first["instance"])
            runtime("lifecycle", body=stale, rejected=True)
            print("Instance fencing and exact restart replay passed.", flush=True)

            # Kill only this owned installer child after its durable Starting
            # checkpoint. Recovery must join a surviving new instance or start
            # it once, rather than drain/restart it a second time.
            crash_request = prepared("restart", second["instance"])
            command = subprocess.Popen([str(binary), "runtime", "lifecycle", "--install-dir", str(install)],
                                       stdin=subprocess.PIPE, stdout=subprocess.PIPE, stderr=subprocess.PIPE,
                                       env=environment, creationflags=flags)
            command.stdin.write(json.dumps(crash_request).encode("utf-8"))
            command.stdin.close()
            deadline = time.monotonic() + 60
            while True:
                try:
                    with closing(sqlite3.connect((control / "service-1.4.sqlite3").as_uri() + "?mode=ro", uri=True, timeout=0.05)) as connection:
                        row = connection.execute("SELECT phase FROM service_lifecycle WHERE phase='starting'").fetchone()
                except sqlite3.OperationalError:
                    row = None
                if row:
                    command.kill()
                    command.wait(timeout=10)
                    break
                if command.poll() is not None or time.monotonic() >= deadline:
                    raise RuntimeError("Could not observe the durable restart checkpoint")
                time.sleep(0.005)
            command.stdout.close()
            command.stderr.close()
            command = None
            third = runtime("ensure")
            runtime("lifecycle", body=crash_request)
            if runtime("ensure")["instance"] != third["instance"] or third["instance"] == second["instance"]:
                raise RuntimeError("Interrupted restart did not preserve its recovered incarnation")
            print("Installer-child crash and durable restart recovery passed.", flush=True)

            stopped = prepared("stop", third["instance"])
            if runtime("lifecycle", body=stopped)["record"]["phase"] != "complete":
                raise RuntimeError("Stop acknowledgement was incomplete")
            runtime("ensure", rejected=True)
            invoke("service", "health", "--runtime-dir", control, rejected=True)
            runtime("lifecycle", body=stopped)
            if not runtime("inspect")["lifecycle"]["explicitly_stopped"]:
                raise RuntimeError("Stop policy did not persist")
            started = runtime("start")
            if started["instance"] == third["instance"] or rpc("request_epoch")["namespace"] != namespace:
                raise RuntimeError("Explicit start lost stop semantics or registry identity")
            for project, job in zip(projects, jobs):
                if rpc({"job": {"id": job["id"]}})["job"]["state"] != "succeeded":
                    raise RuntimeError("Lifecycle changed an existing source receipt")
                bridge = pool.Bridge(binary, control, project["binding"], environment, project["collection"])
                bridges.append(bridge)
                bridge.search("lexical")
            if any(document.read_bytes() != original for document, original in originals):
                raise RuntimeError("Lifecycle changed an original source")
            print("Persistent stop, explicit start, retained citations and original-source preservation passed.", flush=True)
        finally:
            if command is not None:
                if command.poll() is None:
                    command.kill()
                command.wait(timeout=10)
                if command.stdout:
                    command.stdout.close()
                if command.stderr:
                    command.stderr.close()
            close_bridges()
            if (install / "selected.json").exists():
                runtime("uninstall")


if __name__ == "__main__":
    main()
