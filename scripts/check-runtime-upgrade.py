# SPDX-License-Identifier: Apache-2.0
"""Upgrade a real older runtime with a connected launcher/MCP client in isolated state."""
import argparse
from contextlib import closing
import hashlib
import json
import os
from pathlib import Path
import queue
import sqlite3
import subprocess
import tempfile
import threading
import time


def digest(path):
    with path.open("rb") as stream:
        return hashlib.file_digest(stream, "sha256").hexdigest()


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("--runtime", type=Path, required=True)
    parser.add_argument("--previous-runtime", type=Path, required=True)
    parser.add_argument("--interrupted-runtime", type=Path,
                        help="Stage a superseded candidate and seed its exact pre-probe intent in this fictional fixture")
    parser.add_argument("--output", type=Path, required=True)
    args = parser.parse_args()
    if os.name != "nt" and not os.uname().sysname == "Linux":
        raise RuntimeError("Forced predecessor stop is implemented on Windows and Linux")
    name = "airk.exe" if os.name == "nt" else "airk"
    new = args.runtime.resolve(strict=True)
    old = args.previous_runtime.resolve(strict=True)
    binary = new / name
    flags = getattr(subprocess, "CREATE_NO_WINDOW", 0)
    environment = dict(os.environ, HTTP_PROXY="http://127.0.0.1:1", HTTPS_PROXY="http://127.0.0.1:1")

    def invoke(executable, *arguments):
        result = subprocess.run([str(executable), *map(str, arguments)], capture_output=True,
                                text=True, encoding="utf-8", timeout=180,
                                creationflags=flags, env=environment)
        if result.returncode:
            raise RuntimeError(f"Native upgrade fixture failed: {result.stderr[-4096:]}")
        return json.loads(result.stdout)

    temporary = tempfile.TemporaryDirectory(prefix="airk previous-version upgrade-", dir=Path(__file__).resolve().parents[1] / "target")
    root = Path(temporary.name)
    install, state = root / "install", root / "state"
    source, data = root / "source", root / "data"
    source.mkdir()
    # Register validates existing roots; prepare-registration alone only plans
    # storage. Both directories belong exclusively to this synthetic fixture.
    data.mkdir()
    original = b"Original fictional source preserved through upgrade.\n"
    (source / "README.md").write_bytes(original)
    client = None
    client_log = None
    completed = False
    try:
        old_identity, new_identity = digest(old / "airk-install.json"), digest(new / "airk-install.json")
        invoke(old / name, "runtime", "stage", "--source", old, "--install-dir", install,
               "--state-dir", state, "--approved-sha256", old_identity,
               "--verification-basis", "organization-digest")
        invoke(old / name, "runtime", "activate", "--install-dir", install, "--runtime-sha256", old_identity)
        control = state / "control"
        project = invoke(old / name, "--root", source, "--data-dir", data, "--collection", "upgrade",
                         "service", "prepare-registration")
        # Registration contains only the fictional paths created above.
        result = subprocess.run([str(old / name), "service", "rpc", "--runtime-dir", str(control), "--stdin"],
                                input=json.dumps({"register": {"project": project}}), capture_output=True,
                                text=True, encoding="utf-8", timeout=60, creationflags=flags, env=environment)
        if result.returncode or json.loads(result.stdout).get("status") != "registered":
            raise RuntimeError(f"Old fixture registration failed: {result.stdout[-4096:]} {result.stderr[-4096:]}")
        database = control / "service-1.4.sqlite3"
        with closing(sqlite3.connect(database.as_uri() + "?mode=ro", uri=True)) as connection:
            old_format = connection.execute("PRAGMA user_version").fetchone()[0]
        launcher = install / ("airk-launcher.exe" if os.name == "nt" else "airk-launcher")
        client_log_path = root / "client.stderr"
        client_log = client_log_path.open("wb")
        client = subprocess.Popen([str(launcher), "runtime", "launch", "--install-dir", str(install), "--",
                                   "--service-project", project["binding"],
                                   "--root", project["root"], "--data-dir", project["data_root"],
                                   "--collection", project["collection"], "mcp"],
                                  stdin=subprocess.PIPE, stdout=subprocess.PIPE, stderr=client_log,
                                  text=True, encoding="utf-8", creationflags=flags, env=environment)
        replies = queue.Queue()
        def receive_lines():
            try:
                for line in client.stdout:
                    replies.put(line)
            finally:
                replies.put(None)
        threading.Thread(target=receive_lines, daemon=True).start()
        client.stdin.write(json.dumps({"jsonrpc": "2.0", "id": 1, "method": "initialize", "params": {
            "protocolVersion": "2025-11-25", "capabilities": {}, "clientInfo": {"name": "old-upgrade-fixture", "version": "1"}}}) + "\n")
        client.stdin.flush()
        try:
            line = replies.get(timeout=60)
        except queue.Empty as error:
            raise RuntimeError(f"Old MCP initialize timed out (exit={client.poll()}): "
                               f"{client_log_path.read_bytes()[-4096:].decode('utf-8', errors='replace')}") from error
        if line is None:
            raise RuntimeError(f"Old MCP client closed before initialize: "
                               f"{client_log_path.read_bytes()[-4096:].decode('utf-8', errors='replace')}")
        initialized = json.loads(line)
        if initialized.get("id") != 1 or "error" in initialized:
            raise RuntimeError("Old connected MCP client did not initialize")
        client.stdin.write(json.dumps({"jsonrpc": "2.0", "method": "notifications/initialized"}) + "\n")
        client.stdin.flush()
        invoke(binary, "runtime", "stage", "--source", new, "--install-dir", install,
               "--state-dir", state, "--approved-sha256", new_identity,
               "--verification-basis", "organization-digest")
        started = time.monotonic()
        if args.interrupted_runtime:
            interrupted = args.interrupted_runtime.resolve(strict=True)
            interrupted_identity = digest(interrupted / "airk-install.json")
            invoke(binary, "runtime", "stage", "--source", interrupted, "--install-dir", install,
                   "--state-dir", state, "--approved-sha256", interrupted_identity,
                   "--verification-basis", "organization-digest")
            diagnosis = invoke(binary, "runtime", "diagnose", "--install-dir", install)
            # Seed only synthetic fixture metadata, never copied production state.
            intent = {"schema": 1, "install": diagnosis["state_layout"]["layout"]["install"],
                      "control": diagnosis["state_layout"]["control"], "candidate": interrupted_identity,
                      "prior": {"schema": 1, "active": old_identity, "previous": None},
                      "manager": False, "phase": "drain"}
            (install / "activation.json").write_text(json.dumps(intent), encoding="utf-8")
            invoke(binary, "runtime", "abort-activation", "--install-dir", install,
                   "--expected-candidate", interrupted_identity, "--expected-active", old_identity)
            if (install / "activation.json").exists():
                raise RuntimeError("Superseded pre-probe intent survived acknowledged abort")
        selected = invoke(binary, "runtime", "activate", "--install-dir", install,
                          "--runtime-sha256", new_identity, "--expected-active", old_identity)
        elapsed = time.monotonic() - started
        if selected.get("active") != new_identity:
            raise RuntimeError("Upgrade selected the wrong payload")
        client.wait(timeout=10)
        health = invoke(binary, "runtime", "ensure", "--install-dir", install)
        expected_version = json.loads((new / "airk-install.json").read_text(encoding="utf-8"))["version"]
        if health.get("version") != expected_version or health.get("runtime_sha256") != new_identity:
            raise RuntimeError("New service identity mismatch")
        with closing(sqlite3.connect(database.as_uri() + "?mode=ro", uri=True)) as connection:
            version = connection.execute("PRAGMA user_version").fetchone()[0]
            count = connection.execute("SELECT count(*) FROM projects WHERE binding=?", (project["binding"],)).fetchone()[0]
        # Registry schema 8 includes durable setup and recovery budgets. This
        # SQL schema is independent of the public service-state format label.
        if version != 8:
            raise RuntimeError(f"Upgraded registry schema is {version}; expected 8 (previous {old_format})")
        if count != 1:
            raise RuntimeError(f"Upgrade did not preserve the exact project registration: count={count}")
        if (source / "README.md").read_bytes() != original:
            raise RuntimeError("Upgrade changed the fictional source bytes")
        receipt = {"passed": True, "previous_version": json.loads((old / "airk-install.json").read_text(encoding="utf-8"))["version"],
                   "version": expected_version, "previous_format": old_format, "format": version,
                   "activation_elapsed_seconds": elapsed, "connected_client_terminated": True,
                   "registration_preserved": True, "source_preserved": True,
                   "superseded_preprobe_intent": bool(args.interrupted_runtime),
                   "candidate": new_identity, "previous": old_identity}
        completed = True
    finally:
        if client is not None:
            if client.poll() is None:
                client.kill()
            client.wait(timeout=15)
            client.stdin.close()
            client.stdout.close()
        if client_log is not None:
            client_log.close()
        if install.exists():
            invoke(binary, "runtime", "uninstall", "--install-dir", install)
        if completed:
            temporary.cleanup()
        else:
            # Preserve only this synthetic fixture for diagnosis after a failure.
            temporary._finalizer.detach()
            print(f"Preserving owned fictional upgrade fixture: {root}", flush=True)
    # A passing receipt also requires teardown to release every fixture owner.
    args.output.parent.mkdir(parents=True, exist_ok=True)
    args.output.write_text(json.dumps(receipt, indent=2) + "\n", encoding="utf-8")
    print(json.dumps(receipt), flush=True)


if __name__ == "__main__":
    main()
