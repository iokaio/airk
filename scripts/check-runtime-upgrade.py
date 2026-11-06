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
import sys
import tempfile
import threading
import time


def digest(path):
    with path.open("rb") as stream:
        return hashlib.file_digest(stream, "sha256").hexdigest()


def record_native_command(root, records, executable, arguments, *, input=None, timeout=180, flags=0, environment=None):
    ordinal = len(records) + 1
    row = {"ordinal": ordinal, "command": [str(executable), *map(str, arguments)], "exit_code": None}
    def persist(stdout, stderr):
        for suffix, content in [("stdout", stdout), ("stderr", stderr)]:
            raw = content if isinstance(content, bytes) else (content or "").encode("utf-8")
            (root / f"command-{ordinal:03d}.{suffix}").write_bytes(raw)
        (root / "commands.json").write_text(json.dumps(records, indent=2) + "\n", encoding="utf-8")
    try:
        result = subprocess.run(row["command"], input=input, capture_output=True,
            text=True, encoding="utf-8", timeout=timeout, creationflags=flags, env=environment)
    except subprocess.TimeoutExpired as error:
        row.update(failure_type="TimeoutExpired", timeout_seconds=timeout)
        records.append(row)
        try:
            persist(error.stdout, error.stderr)
        except Exception as logging_error:
            row["diagnostic_error"] = repr(logging_error)
        raise  # Preserve the exact original timeout and its partial streams.
    row["exit_code"] = result.returncode
    records.append(row)
    try:
        persist(result.stdout, result.stderr)
    except Exception as logging_error:
        row["diagnostic_error"] = repr(logging_error)
        if not result.returncode:
            raise  # A successful operation without evidence does not qualify.
    return result


def owned_temporary(parent):
    temporary = tempfile.TemporaryDirectory(prefix="airk previous-version upgrade-", dir=parent)
    # Retention is established BEFORE native activation or any fallible cleanup.
    # Success explicitly removes this owned fixture after acknowledged uninstall.
    temporary._finalizer.detach()
    return temporary


def finish_fixture(temporary, root, completed, uninstall, client=None, client_log=None, observe_stop=None):
    errors = []
    def attempt(name, operation):
        try:
            return operation()
        except Exception as error:
            errors.append({"operation": name, "error": repr(error)})
            return None
    if client is not None:
        if client.poll() is None:
            attempt("kill_owned_client", client.kill)
        attempt("wait_owned_client", lambda: client.wait(timeout=15))
        attempt("close_owned_stdin", client.stdin.close)
        attempt("close_owned_stdout", client.stdout.close)
    if client_log is not None:
        attempt("close_client_log", client_log.close)
    pending = (root / "install/activation.json").exists()
    stopped = None
    # A committed pending catalog upgrade requires explicit owner recovery.
    # Never auto-abort it, restart an old metadata reader, or mask it by uninstall.
    acknowledged = False
    if completed and not pending:
        if not (root / "install").is_dir():
            errors.append({"operation": "uninstall", "error": "Owned installation missing; no native uninstall acknowledgment"})
        else:
            value = attempt("uninstall", uninstall)
            acknowledged = isinstance(value, dict) and value.get("uninstalled") is True
            if not acknowledged and not errors:
                errors.append({"operation": "uninstall", "error": "Native uninstall deferred or unacknowledged"})
    elif observe_stop is not None:
        stopped = attempt("observe_owned_candidate_stop", observe_stop)
    removed = completed and not pending and acknowledged and not errors
    if removed:
        attempt("remove_completed_fixture", temporary.cleanup)
        removed = not errors
    if not removed:
        print(f"Preserving owned fictional upgrade fixture: {root}", flush=True)
    return {"errors": errors, "activation_pending": pending, "uninstall_acknowledged": acknowledged, "fixture_retained": not removed,
            "candidate_stop_observation": stopped, "teardown_complete": removed}


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("--runtime", type=Path, required=True)
    parser.add_argument("--previous-runtime", type=Path, required=True)
    parser.add_argument("--interrupted-runtime", type=Path,
                        help="Stage a superseded candidate and seed its exact pre-probe intent in this fictional fixture")
    parser.add_argument("--output", type=Path, required=True)
    parser.add_argument("--source-state", choices=("indexed", "registered-unindexed"), default="indexed",
                        help="Indexed is a positive explicit old-fixture Index; registered-unindexed retains the original fail-closed recovery case")
    args = parser.parse_args()
    if os.name != "nt" and not os.uname().sysname == "Linux":
        raise RuntimeError("Forced predecessor stop is implemented on Windows and Linux")
    name = "airk.exe" if os.name == "nt" else "airk"
    new = args.runtime.resolve(strict=True)
    old = args.previous_runtime.resolve(strict=True)
    binary = new / name
    flags = getattr(subprocess, "CREATE_NO_WINDOW", 0)
    environment = dict(os.environ, HTTP_PROXY="http://127.0.0.1:1", HTTPS_PROXY="http://127.0.0.1:1")

    records = []
    root = None
    def command(executable, *arguments, input=None, timeout=180):
        return record_native_command(root, records, executable, arguments, input=input,
                                     timeout=timeout, flags=flags, environment=environment)
    def invoke(executable, *arguments, input=None):
        result = command(executable, *arguments, input=input)
        if result.returncode:
            raise RuntimeError(f"Native upgrade fixture failed: {result.stderr[-4096:]}")
        return json.loads(result.stdout)
    private_parent = os.environ.get("AIRK_PRIVATE_CENSUS_BASE")
    parent = Path(private_parent).resolve(strict=True) if private_parent else Path(__file__).resolve().parents[1] / "target"
    parent.mkdir(exist_ok=True)
    temporary = owned_temporary(parent)
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
    primary_error = None
    receipt = {"passed": False, "source_state": args.source_state, "fixture_root": str(root),
               "qualification_scope": "explicit old-index positive upgrade" if args.source_state == "indexed"
                   else "registered never-indexed recovery observation; not positive upgrade qualification"}
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
        result = command(old / name, "service", "rpc", "--runtime-dir", control, "--stdin",
                         input=json.dumps({"register": {"project": project}}))
        if result.returncode or json.loads(result.stdout).get("status") != "registered":
            raise RuntimeError(f"Old fixture registration failed: {result.stdout[-4096:]} {result.stderr[-4096:]}")
        if args.source_state == "indexed":
            # This is an explicit action on the OLD fictional fixture, never a
            # migration assumption or inferred grant/configuration for missing state.
            accepted = invoke(old / name, "--service-runtime-dir", control,
                              "--service-project", project["binding"], "--root", source,
                              "--data-dir", data, "--collection", "upgrade", "index")
            saved_job = accepted["job"]["id"]
            deadline = time.monotonic() + 180
            while True:
                observed = invoke(old / name, "service", "rpc", "--runtime-dir", control,
                                  "--stdin", input=json.dumps({"job": {"id": saved_job}}))["job"]
                if observed["id"] != saved_job:
                    raise RuntimeError("Old Index receipt changed identity")
                if observed["state"] in ("succeeded", "failed", "canceled"):
                    if observed["state"] != "succeeded":
                        raise RuntimeError("Explicit old fixture Index failed: " + json.dumps(observed))
                    break
                if time.monotonic() >= deadline:
                    raise RuntimeError("Explicit old fixture Index deadline exhausted; no resubmission")
                time.sleep(0.05)
            receipt["explicit_old_index_job"] = observed
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
        if args.source_state == "registered-unindexed":
            raise RuntimeError("Registered-unindexed activation unexpectedly succeeded; inspect the changed recovery contract")
        if selected.get("active") != new_identity:
            raise RuntimeError("Upgrade selected the wrong payload")
        client.wait(timeout=10)
        health = invoke(binary, "runtime", "ensure", "--install-dir", install)
        expected_version = json.loads((new / "airk-install.json").read_text(encoding="utf-8"))["version"]
        if health.get("version") != expected_version or health.get("runtime_sha256") != new_identity:
            raise RuntimeError("New service identity mismatch")
        with closing(sqlite3.connect(database.as_uri() + "?mode=ro", uri=True)) as connection:
            version = connection.execute("PRAGMA user_version").fetchone()[0]
            saved = connection.execute("SELECT payload,registered FROM projects WHERE binding=?", (project["binding"],)).fetchone()
            if saved is None or len(saved[0]) > 65536 or saved[1] != 1 or json.loads(saved[0]) != project:
                raise RuntimeError("Upgrade did not preserve the complete original project registration")
        # Native catalog upgrade owner writes SQL registry version12. This is
        # independent of public state/version and authority/policy contracts.
        if version != 12:
            raise RuntimeError(f"Upgraded registry schema is {version}; expected 12 (previous {old_format})")
        if (source / "README.md").read_bytes() != original:
            raise RuntimeError("Upgrade changed the fictional source bytes")
        receipt.update({"passed": True, "previous_version": json.loads((old / "airk-install.json").read_text(encoding="utf-8"))["version"],
                   "version": expected_version, "previous_format": old_format, "format": version,
                   "activation_elapsed_seconds": elapsed, "connected_client_terminated": True,
                   "registration_preserved": True, "source_preserved": True,
                   "superseded_preprobe_intent": bool(args.interrupted_runtime),
                   "candidate": new_identity, "previous": old_identity})
        completed = True
    except BaseException as error:
        primary_error = error
        receipt["primary_error"] = {"type": type(error).__name__, "message": str(error)}
        raise
    finally:
        def observe_owned_candidate():
            # Observe health, then explicitly mutate ONLY this owned candidate to stop;
            # neither successful health nor an absent listener creates authority.
            result = command(binary, "service", "rpc", "--runtime-dir", state / "control",
                             "--stdin", input=json.dumps("health"), timeout=15)
            observation = {"health_exit_code": result.returncode, "stopped": None,
                           "scope": "unknown unless matching candidate acknowledged shutdown; no PID inference"}
            if result.returncode == 0:
                health = json.loads(result.stdout)
                observation["health"] = health
                if isinstance(locals_new_identity, str) and len(locals_new_identity) == 64 and health.get("status") == "health" and health.get("observation", {}).get("runtime_sha256") == locals_new_identity:
                    stopped = command(binary, "service", "rpc", "--runtime-dir", state / "control",
                                      "--stdin", input=json.dumps("shutdown"), timeout=15)
                    observation["shutdown_exit_code"] = stopped.returncode
                    if stopped.returncode == 0:
                        observation["shutdown_reply"] = json.loads(stopped.stdout)
                        observation["shutdown_acknowledged"] = observation["shutdown_reply"].get("status") == "stopping"
                        observation["process_exit_confirmed"] = False  # Ack is not an exit proof.
            return observation
        locals_new_identity = locals().get("new_identity")
        try:
            receipt["source_preserved_before_teardown"] = (source / "README.md").read_bytes() == original
            receipt["pending_manual_recovery"] = (install / "activation.json").exists()
            if "database" in locals() and "project" in locals():
                try:
                    with closing(sqlite3.connect(database.as_uri() + "?mode=ro", uri=True)) as sql:
                        row = sql.execute("SELECT payload,registered FROM projects WHERE binding=?", (project["binding"],)).fetchone()
                        if row is None or len(row[0]) > 65536:
                            raise RuntimeError("Saved registration absent or exceeds 64 KiB")
                        receipt["registration_unchanged"] = json.loads(row[0]) == project and row[1] == 1
                        receipt["observed_registry_schema"] = sql.execute("PRAGMA user_version").fetchone()[0]
                        if args.source_state == "registered-unindexed":
                            receipt["observed_imported_grants"] = sql.execute("SELECT count(*) FROM index_access_grants WHERE import_legacy_binding=?", (project["binding"],)).fetchone()[0]
                except Exception as error:
                    receipt["registration_or_grant_observation_unavailable"] = repr(error)
        except Exception as error:
            receipt["source_observation_unavailable"] = repr(error)
        # Final exact observations, not mere row count or missing-as-success,
        # gate the positive receipt and whether teardown may remove evidence.
        if completed and (receipt.get("registration_unchanged") is not True
                          or receipt.get("source_preserved_before_teardown") is not True):
            receipt["preservation_error"] = "Exact final registration/source preservation unavailable or changed"
            completed = False
        try:
            teardown = finish_fixture(temporary, root, completed,
                lambda: invoke(binary, "runtime", "uninstall", "--install-dir", install),
                client, client_log, observe_owned_candidate if not completed else None)
            receipt["teardown"] = teardown
            receipt["passed"] = completed and teardown["teardown_complete"]
            receipt["source_preserved"] = receipt.get("source_preserved_before_teardown")
            receipt["source_scope_note"] = "Successful teardown removed only the verified fictional fixture; failures preserve source and native pending state."
            args.output.parent.mkdir(parents=True, exist_ok=True)
            with args.output.open("x", encoding="utf-8") as output:
                output.write(json.dumps(receipt, indent=2) + "\n")
            print(json.dumps(receipt), flush=True)
            if primary_error is None and not receipt["passed"]:
                raise RuntimeError("Upgrade assertions completed but owned teardown remains incomplete; retained fixture")
        except BaseException as diagnostic_error:
            if primary_error is None:
                raise
            # The detached fixture remains even if output persistence is broken.
            # Never replace the native refusal with a cleanup/logging exception.
            try:
                print(f"Secondary upgrade fixture diagnostic/teardown error: {diagnostic_error!r}; retained {root}", file=sys.stderr)
            except Exception:
                pass




if __name__ == "__main__":
    main()
