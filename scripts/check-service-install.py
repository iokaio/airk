# SPDX-License-Identifier: Apache-2.0
"""Offline native lifecycle fixture; installs only into its owned temporary directory."""
import argparse
import contextlib
import hashlib
import importlib.util
import json
import os
from pathlib import Path
import shutil
import stat
import subprocess
import sys
import tempfile
import time
import uuid


# Keep the native 180-second checkpoint authoritative. The outer lifecycle
# allowance also covers the ordinary lock/setup/exit budget, candidate health
# (60 seconds), and error reporting. This does not change native deadlines.
LIFECYCLE_TIMEOUT = 180 + 180 + 60 + 60


def invoke_native(executable, *arguments, environment, rejected=False, raw=False, input=None, timeout=None):
    if timeout is None:
        timeout = LIFECYCLE_TIMEOUT if arguments[:2] in (("runtime", "activate"), ("runtime", "rollback")) else 180
    try:
        result = subprocess.run([str(executable), *map(str, arguments)], capture_output=True,
                                encoding="utf-8", env=environment, timeout=timeout,
                                input=input, creationflags=getattr(subprocess, "CREATE_NO_WINDOW", 0))
    except subprocess.TimeoutExpired as error:
        # subprocess retains captured streams on the original exception. Expose
        # bounded diagnostic tails only for lifecycle commands, whose output is
        # an activation receipt/error rather than an enrollment credential.
        if arguments[:2] in (("runtime", "activate"), ("runtime", "rollback")):
            def tail(value):
                return (value.decode("utf-8", errors="replace") if isinstance(value, bytes) else value or "")[-4096:]
            error.add_note(f"Native lifecycle timed out without retry. Last stdout: {tail(error.stdout)}; last stderr: {tail(error.stderr)}")
        raise
    if rejected:
        if not result.returncode:
            raise RuntimeError("Expected native policy/inventory rejection")
        return None
    if result.returncode:
        raise RuntimeError(result.stderr[-4096:])
    return result.stdout.strip() if raw else json.loads(result.stdout)


def sha256(path):
    with path.open("rb") as stream:
        return hashlib.file_digest(stream, "sha256").hexdigest()


def fixture_parent(root=Path(__file__).resolve().parents[1]):
    """Return the repository target directory, creating it when no native build has.

    Validation jobs that reuse a transferred runtime never compile here, so the
    fixture must not assume Cargo already created this directory.
    """
    parent = root / "target"
    parent.mkdir(parents=True, exist_ok=True)
    return parent


@contextlib.contextmanager
def owned_fixture():
    parent = fixture_parent().resolve(strict=True)
    fixture = Path(tempfile.mkdtemp(prefix="airk install original café fixture-", dir=parent))
    try:
        yield fixture
    except BaseException:
        print(f"Failed owned installation fixture retained: {fixture}", file=sys.stderr, flush=True)
        raise
    else:
        resolved = fixture.resolve(strict=True)
        if (fixture.is_symlink() or resolved.parent != parent
                or not resolved.name.startswith("airk install original café fixture-")):
            raise RuntimeError("Owned installation cleanup boundary changed")
        shutil.rmtree(resolved)


def check_login_plan(plan, install, platform=sys.platform, *, version="1.8.13"):
    if not plan["id"].startswith("airksvc-"):
        raise RuntimeError("Optional startup plan reused a legacy scheduler identity")
    arguments = plan["arguments"]
    if platform == "darwin":
        # SMAppService dispatches the sealed app through its fixed channel entry.
        # It intentionally does not use the Windows/systemd launcher arguments.
        valid = (plan["platform"] == "smappservice_user"
                 and arguments == ["runtime", "login-serve" if runtime_builder().single_image_version(version) else "login-foreground", "--channel", "stable"])
    else:
        entry = "managed-serve" if runtime_builder().single_image_version(version) else "foreground"
        valid = len(arguments) == 4 and arguments[:3] == ["runtime", entry, "--install-dir"]
        if valid:
            try:
                valid = Path(arguments[3]).samefile(install)
            except OSError:
                valid = False
    if not valid:
        raise RuntimeError(f"Optional startup plan has an unexpected platform entry: {plan}")


def runtime_builder():
    specification = importlib.util.spec_from_file_location("installer_runtime_builder", Path(__file__).with_name("build-service-runtime.py"))
    module = importlib.util.module_from_spec(specification)
    specification.loader.exec_module(module)
    return module


def check_image(path, expected):
    """Assert exact approved bytes, regular unaliased ownership and native mode."""
    try:
        metadata = path.lstat()
    except OSError as error:
        raise RuntimeError(f"Installed approved image is missing or unavailable: {path.name}") from error
    if (not stat.S_ISREG(metadata.st_mode) or metadata.st_nlink != 1
            or getattr(metadata, "st_file_attributes", 0) & 0x400
            or sha256(path) != expected or os.name != "nt" and metadata.st_mode & 0o111 == 0):
        raise RuntimeError(f"Installed image is missing, aliased, nonexecutable or modified: {path.name}")


def check_installed_images(payload, installed, inventory, invoke):
    builder = runtime_builder()
    operating_system, version = inventory["os"], inventory["version"]
    main, helper = builder.runtime_names(operating_system)
    if builder.single_image_version(version):
        if main not in inventory["files"]:
            raise RuntimeError("Approved executable inventory is incomplete")
        check_image(installed / main, inventory["files"][main])
        suffix = ".exe" if operating_system == "windows" else ""
        forbidden = [helper, "airk-launcher" + suffix, "airk-service" + suffix,
                     "airk-index" + suffix, "airk-source" + suffix]
        for name in forbidden:
            if name in inventory["files"] or os.path.lexists(installed / name) or os.path.lexists(payload / name):
                raise RuntimeError("Single-executable payload contains a retired helper, launcher or role alias")
        if invoke(installed / main, "runtime", "self-test").get("self_test") != "passed":
            raise RuntimeError("Single-executable main failed native self-test")
        return False, inventory["files"][main]
    launcher = builder.launcher_name(operating_system, version)
    current_mac = operating_system == "macos" and launcher != main
    required = {main, helper, launcher}
    if not required.issubset(inventory["files"]):
        raise RuntimeError("Approved executable inventory is incomplete")
    for name in required:
        check_image(installed / name, inventory["files"][name])
    if current_mac:
        # A sealed main carries its resource envelope. Service/worker copies are
        # deliberately absent; the standalone launcher has independent approval.
        for role in ("airk-service", "airk-index", "airk-source"):
            if os.path.lexists(installed / role):
                raise RuntimeError("Sealed Mac payload must not create process-role aliases")
            if os.path.lexists(payload / role):
                raise RuntimeError("Installer modified distribution payload with role copies")
        if invoke(installed / main, "runtime", "self-test").get("self_test") != "passed":
            raise RuntimeError("Sealed main cannot resolve its verified helper")
        if invoke(installed / launcher, "--version", raw=True) != f"airk {version}":
            raise RuntimeError("Standalone launcher image lost its executable identity")
    else:
        for role in ("airk-service", "airk-index", "airk-source"):
            name = role + (".exe" if operating_system == "windows" else "")
            check_image(installed / name, inventory["files"][main])
            if invoke(installed / name, "runtime", "self-test").get("self_test") != "passed":
                raise RuntimeError("Role image cannot resolve its verified helper")
            if os.path.lexists(payload / name):
                raise RuntimeError("Installer modified distribution payload with role copies")
    return current_mac, inventory["files"][launcher]


def check_payload_unchanged(payload, inventory, identity):
    if sha256(payload / "airk-install.json") != identity:
        raise RuntimeError("Lifecycle fixture changed the source installer inventory")
    expected_paths = set(inventory["files"]) | {"airk-install.json"}
    observed = set()
    for path in payload.rglob("*"):
        if path.is_symlink():
            raise RuntimeError("Lifecycle fixture introduced a source payload alias")
        if path.is_file():
            observed.add(path.relative_to(payload).as_posix())
            if len(observed) > len(expected_paths):
                raise RuntimeError("Lifecycle fixture changed source payload topology")
    if observed != expected_paths:
        raise RuntimeError("Lifecycle fixture changed source payload topology")
    for name, expected in inventory["files"].items():
        path = payload / name
        if path.is_symlink() or not path.is_file() or sha256(path) != expected:
            raise RuntimeError(f"Lifecycle fixture changed source payload: {name}")


def check_copied_mac_launcher(path, expected, invoke, version, verify_signature=None):
    check_image(path, expected)
    if verify_signature is None:
        def verify_signature(image):
            result = subprocess.run(["/usr/bin/codesign", "--verify", "--strict", str(image)],
                                    capture_output=True, text=True, timeout=30)
            if result.returncode:
                raise RuntimeError(f"Copied launcher signature is invalid: {result.stderr[-4096:]}")
    verify_signature(path)
    if invoke(path, "--version", raw=True) != f"airk {version}":
        raise RuntimeError("Copied Mac launcher changed version/stdio")


def load_fixture(name, filename):
    spec = importlib.util.spec_from_file_location(name, Path(__file__).with_name(filename))
    module = importlib.util.module_from_spec(spec)
    spec.loader.exec_module(module)
    return module


def parity_envelope(envelope, transport):
    """Retain the complete envelope, including future fields and structured data.

    Only serialized native JSON text needs decoding before applying the same
    documented timing normalization used for structured fields. Raw proofs stay
    unchanged in the evidence file.
    """
    value = json.loads(json.dumps(envelope))
    for block in value.get("content", []):
        if block.get("type") == "text" and isinstance(block.get("text"), str):
            try:
                body = json.loads(block["text"])
            except ValueError:
                continue  # Non-JSON text remains an exact part of the envelope.
            block["text"] = json.dumps(transport.parity_value(body), sort_keys=True, separators=(",", ":"))
    return transport.parity_value(value)


def modern_client_lifecycle(invoke, binary, installed_main, install, control, source, data, health, evidence_file,
                            *, stopped_probe=None):
    """Actual native grant/enrollment and two independent service-owned TLS sessions."""
    reviewed = load_fixture("installation_reviewed_fixture", "check-runtime-upgrade.py")
    transport = load_fixture("installation_http_fixture", "http_fixture.py")
    reads = load_fixture("installation_read_contract", "check-service-clients.py")
    def rpc(method):
        response = reads.metadata_rpc(lambda exact, _deadline: invoke(binary, "service", "rpc", "--runtime-dir", control,
                                                                      "--stdin", input=exact), method)
        if response.get("status") in ("failure", "incompatible"):
            raise RuntimeError("Owned installation metadata request refused: " + json.dumps(response))
        return response
    project, selected = reviewed.reviewed_fixture(rpc, source, data, collection="install-lifetime")
    job = invoke(binary, "--service-runtime-dir", control, "--service-project", project["binding"],
                 "--root", project["root"], "--data-dir", project["data_root"], "--collection", project["collection"], "index")["job"]
    deadline = time.monotonic() + 180
    while True:
        observed = rpc({"job": {"id": job["id"]}})["job"]
        if observed["id"] != job["id"]:
            raise RuntimeError("Owned indexing observation changed job identity")
        if observed["state"] in ("succeeded", "failed", "canceled"):
            if observed["state"] != "succeeded":
                raise RuntimeError("Owned installation indexing failed: " + json.dumps(observed))
            break
        if time.monotonic() >= deadline:
            raise RuntimeError("Owned indexing outcome unknown; no resubmission")
        time.sleep(.05)
    proof = {"project": project, "selection": selected, "index_job": observed, "connections": [], "responses": []}
    def retain():
        evidence_file.write_text(json.dumps(proof, indent=2) + "\n", encoding="utf-8")
    def enroll():
        identifier = uuid.uuid4().hex
        proof.setdefault("enrollment_attempts", []).append({"connection_id":identifier,
            "selection":{"kind":"scoped","selection":selected}, "client":"unknown"})
        retain()
        connection = transport.enroll_native(
            lambda *arguments, timeout=None: invoke(binary, *arguments, timeout=timeout),
            install, control, {"kind": "scoped", "selection": selected}, identifier)
        expected, actual = dict(selected), dict(connection["selection"]["selection"])
        expected.pop("expected_catalog_revision"); actual.pop("expected_catalog_revision")
        if connection["connection_id"] != identifier or connection["selection"]["kind"] != "scoped" or expected != actual:
            raise RuntimeError("Owned HTTPS enrollment changed immutable authority")
        native = rpc("health")["observation"]
        transport.validate_native_identity(connection, native)
        if native["instance"] != health["instance"]:
            raise RuntimeError("Client enrollment replaced the installed service")
        proof["connections"].append(connection); retain()
        return transport.McpClient(connection, control)
    first, second = None, None
    try:
        first = enroll()
        second = enroll()
        for number, client in enumerate((first, second)):
            client.send({"jsonrpc":"2.0","id":1,"method":"initialize","params":{"protocolVersion":"2025-11-25","capabilities":{},"clientInfo":{"name":"installed-http-lifetime","version":"1"}}})
            response = client.receive(1)
            proof["responses"].append({"client":number,"initialize":response}); retain()
            if response["result"]["serverInfo"]["name"] != "airk":
                raise RuntimeError("Installed HTTPS server identity changed")
            client.send({"jsonrpc":"2.0","method":"notifications/initialized"})
        if not first.session or not second.session or first.session == second.session:
            raise RuntimeError("Independent HTTP clients did not acquire distinct MCP sessions")
        proof["session_ids_sha256"] = [hashlib.sha256(client.session.encode()).hexdigest() for client in (first, second)]
        retain()
        first.close(); first = None
        second.send({"jsonrpc":"2.0","id":2,"method":"ping","params":{}})
        ping = second.receive(2)
        if ping.get("error") or ping.get("result") != {}:
            raise RuntimeError("Closing one HTTP client interrupted another session")
        arguments = {"path":"sentinel.txt","start_line":1,"end_line":1,"response_mode":"full"}
        proof["responses"].append({"surviving_ping":ping}); retain()
        def read(method):
            deadline = time.monotonic() + 60
            for attempt in range(41):
                if method == "mcp":
                    second.send({"jsonrpc":"2.0","id":3,"method":"tools/call","params":{"name":"read_file","arguments":arguments}})
                    result = second.receive(3)["result"]
                else:
                    result = second.api_read("read_file", arguments)
                proof["responses"].append({"transport":method,"attempt":attempt,"result":result}); retain()
                delay = reads.read_retry_delay(result)
                if delay is None:
                    return result
                if attempt == 40 or time.monotonic() + delay >= deadline:
                    raise RuntimeError("Owned HTTPS typed read retry deadline exhausted")
                time.sleep(delay)
        envelope = read("mcp")
        if envelope.get("isError") is True:
            raise RuntimeError("Surviving HTTPS client failed its source read")
        body = json.loads(envelope["content"][0]["text"])
        original = (source / "sentinel.txt").read_bytes()
        if not any(hit.get("citation",{}).get("path") == "sentinel.txt" and hit["citation"].get("content_hash") == hashlib.sha256(original).hexdigest()
                   and original.decode().strip() in hit.get("text","") for hit in body.get("hits",[])):
            raise RuntimeError("Surviving HTTPS client lost exact original source evidence")
        api = read("json_api")
        if (api.get("isError") or parity_envelope(envelope, transport) != parity_envelope(api, transport)
                or transport.parity_value(body) != transport.parity_value(json.loads(api["content"][0]["text"]))):
            raise RuntimeError("Installed HTTPS MCP/JSON API full read parity changed")
    finally:
        if first is not None: first.close()
        if second is not None: second.close()
    if rpc("health")["observation"]["instance"] != health["instance"]:
        raise RuntimeError("HTTP session shutdown disturbed the shared service")
    prepared = invoke(binary,"runtime","prepare-client","--install-dir",install)
    if (prepared.get("launcher") is not None or prepared.get("service_lifecycle_changed") is not False
            or not Path(prepared["executable"]).samefile(installed_main)):
        raise RuntimeError("Modern client preparation created a launcher or changed lifecycle")
    epoch = rpc("request_epoch")
    request = {"caller":uuid.uuid4().hex,"key":{"namespace":epoch["namespace"],"epoch":epoch["epoch"],"nonce":uuid.uuid4().hex},
               "action":"stop","expected_instance":health["instance"],"runtime":health["runtime_sha256"],"force":False}
    stopped = invoke(binary,"runtime","lifecycle","--install-dir",install,input=json.dumps(request))
    proof["managed_stop"] = stopped; retain()
    record = stopped.get("record",{})
    if (record.get("phase") != "complete" or
            any(record.get("request",{}).get(field) != request[field] for field in
                ("caller", "key", "action", "expected_instance", "runtime"))):
        raise RuntimeError("Managed Stop did not qualify its exact observed instance")
    inspect = invoke(binary,"runtime","inspect","--install-dir",install)
    if inspect["lifecycle"].get("explicitly_stopped") is not True or inspect["lifecycle"].get("pending") is not None:
        raise RuntimeError("Managed Stop did not preserve its settled durable fence")
    invoke(binary,"runtime","ensure","--install-dir",install,rejected=True)
    if stopped_probe is not None:
        stopped_probe()
    restarted = invoke(binary,"runtime","start","--install-dir",install)
    if restarted["instance"] == health["instance"] or restarted["runtime_sha256"] != health["runtime_sha256"]:
        raise RuntimeError("Explicit Start did not elect the same approved runtime as a fresh instance")
    proof["explicit_start"] = restarted; retain()
    return restarted


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("--runtime", type=Path, required=True)
    args = parser.parse_args()
    payload = args.runtime.resolve(strict=True)
    binary = payload / ("airk.exe" if os.name == "nt" else "airk")
    if not binary.is_file():
        binary = payload / "AIRK.app/Contents/MacOS/airk"
    identity = sha256(payload / "airk-install.json")
    inventory = json.loads((payload / "airk-install.json").read_text(encoding="utf-8"))
    version = inventory["version"]
    modern = runtime_builder().single_image_version(version)
    environment = dict(os.environ, HTTP_PROXY="http://127.0.0.1:1",
                       HTTPS_PROXY="http://127.0.0.1:1", ALL_PROXY="http://127.0.0.1:1")
    flags = getattr(subprocess, "CREATE_NO_WINDOW", 0)

    def invoke(executable, *arguments, **kwargs):
        return invoke_native(executable, *arguments, environment=environment, **kwargs)

    def client_lifetime(launcher, source, control):
        result = subprocess.run([sys.executable, str(Path(__file__).with_name("check-client-lifetime.py")),
                                 "--launcher", str(launcher), "--root", str(source), "--control-dir", str(control)],
                                capture_output=True, encoding="utf-8", timeout=120,
                                env=environment, creationflags=flags)
        if result.returncode:
            raise RuntimeError(f"Client lifetime fixture failed ({result.returncode}): {result.stdout[-4096:]} {result.stderr[-8192:]}")
        print(result.stdout.strip(), flush=True)

    with owned_fixture() as temporary:
        fixture = Path(temporary)
        environment.update(load_fixture("installed_fixture_git_config", "check-runtime-upgrade.py").isolated_git_environment(fixture))
        install = fixture / "install"
        state = fixture / "service-state"
        control = state / "control"
        user_data = fixture / "project-data"
        user_data.mkdir()
        original = b"Original project data must survive runtime uninstall.\n"
        (user_data / "sentinel").write_bytes(original)

        def run(*arguments, **kwargs):
            return invoke(binary, "runtime", *arguments, **kwargs)

        def stage(source, approved):
            return run("stage", "--source", source, "--install-dir", install,
                       "--approved-sha256", approved, "--verification-basis", "organization-digest",
                       "--state-dir", state)

        try:
            run("stage", "--source", payload, "--install-dir", install,
                "--approved-sha256", "0" * 64, "--verification-basis", "organization-digest", rejected=True)
            if (install / "selected.json").exists():
                raise RuntimeError("Rejected install changed selection")
            print("Checking offline staging and native self-test...", flush=True)
            approval = stage(payload, identity)
            if approval["manifest_sha256"] != identity or stage(payload, identity) != approval:
                raise RuntimeError("Repeat verified staging is not idempotent")
            if (install / "selected.json").exists():
                raise RuntimeError("Staging selected an untested/unstarted service")
            print("Checking service election and stable launcher...", flush=True)
            first = run("activate", "--install-dir", install, "--runtime-sha256", identity)
            if first["active"] != identity:
                raise RuntimeError("First activation lost approved identity")
            print("Checking approved installed images and platform role layout...", flush=True)
            installed_payload = install / "runtimes" / identity
            current_mac, launcher_digest = check_installed_images(payload, installed_payload, inventory, invoke)
            if sha256(installed_payload / "airk-install.json") != identity:
                raise RuntimeError("Role installation rewrote approved inventory")
            health = run("ensure", "--install-dir", install)
            if health["runtime_sha256"] != identity:
                raise RuntimeError("Health does not bind the installed payload")
            if run("ensure", "--install-dir", install)["instance"] != health["instance"]:
                raise RuntimeError("Idempotent ensure launched another instance")
            inspection = run("inspect", "--install-dir", install)
            if not Path(inspection["control_dir"]).samefile(control) or (install / "control").exists():
                raise RuntimeError("Service state was not isolated from executable installation")
            state_owner = state / "airk-state-1.4.owner.json"
            state_owner_digest = sha256(state_owner)
            plan = run("login", "--install-dir", install, "plan")
            check_login_plan(plan, install, version=version)
            if modern:
                installed_main = installed_payload / runtime_builder().runtime_names(inventory["os"])[0]
                if (inspection.get("launcher") is not None or inspection.get("universal_launcher") is not None
                        or not Path(inspection["executable"]).samefile(installed_main)):
                    raise RuntimeError("Modern inspection exposed a copied launcher or different main")
                print("Checking service-owned HTTPS sessions and instance-fenced Stop/Start...", flush=True)
                http_source = fixture / "https-source"
                http_source.mkdir()
                (http_source / "sentinel.txt").write_bytes(original)
                health = modern_client_lifecycle(invoke, binary, installed_main, install, control, http_source,
                                                 fixture / "index-state", health, fixture / "https-lifecycle-evidence.json")
            else:
                launcher = Path(inspection["launcher"])
                if current_mac:
                    check_copied_mac_launcher(launcher, launcher_digest, invoke, version)
                print("Checking client shutdown preserves other sessions and the service...", flush=True)
                client_lifetime(launcher, user_data, control)
                universal = fixture / "universal"
                universal.mkdir()
                entrypoint = universal / launcher.name
                shutil.copy2(launcher, entrypoint)
                (universal / "airk-launcher.json").write_text(json.dumps({
                    "schema": 1, "install": str(install.resolve()), "launcher_sha256": sha256(entrypoint)
                }), encoding="utf-8")
                if current_mac:
                    check_copied_mac_launcher(entrypoint, launcher_digest, invoke, version)
                client_lifetime(entrypoint, user_data, control)
                if run("ensure", "--install-dir", install)["instance"] != health["instance"]:
                    raise RuntimeError("MCP client shutdown disturbed the shared service")
                actual = invoke(launcher, "runtime", "launch", "--install-dir", install,
                                "--", "--version", raw=True)
                if actual != f"airk {version}":
                    raise RuntimeError("Stable launcher changed stdio or executable identity")
                print("Checking stopped-owner launcher repair and modified-file preservation...", flush=True)
                before_launcher = sha256(launcher)
                run("repair-launcher", "--install-dir", install, rejected=True)
                if sha256(launcher) != before_launcher or (install / "launcher-repair.json").exists():
                    raise RuntimeError("Live-owner repair changed launcher state")
                invoke(binary, "service", "shutdown", "--runtime-dir", control)
                until = time.monotonic() + 30
                while True:
                    try:
                        run("repair-launcher", "--install-dir", install)
                        break
                    except RuntimeError as error:
                        if "stop the service and wait for all owners" not in str(error) or time.monotonic() >= until:
                            raise
                        time.sleep(0.1)
                changed = b"Original user-modified launcher fixture.\n"
                launcher.write_bytes(changed)
                run("repair-launcher", "--install-dir", install, rejected=True)
                if launcher.read_bytes() != changed:
                    raise RuntimeError("Repair overwrote a modified launcher")
                launcher.unlink()
                if not run("repair-launcher", "--install-dir", install).get("launcher_repaired"):
                    raise RuntimeError("Missing launcher could not be repaired from approved offline bytes")
                if sha256(launcher) != before_launcher:
                    raise RuntimeError("Repaired launcher has the wrong approved identity")
                if current_mac:
                    check_copied_mac_launcher(launcher, launcher_digest, invoke, version)
                actual = invoke(launcher, "runtime", "launch", "--install-dir", install,
                                "--", "--version", raw=True)
                if actual != f"airk {version}":
                    raise RuntimeError("Repaired launcher lost argument forwarding")
                print("Checking continuous foreground dispatch through the repaired launcher...", flush=True)
                foreground = subprocess.Popen([str(launcher), "runtime", "foreground", "--install-dir", str(install)],
                                              stdin=subprocess.DEVNULL, stdout=subprocess.DEVNULL,
                                              stderr=subprocess.DEVNULL, env=environment, creationflags=flags)
                try:
                    until = time.monotonic() + 90
                    while True:
                        if foreground.poll() is not None:
                            raise RuntimeError("Foreground launcher exited before native health")
                        try:
                            observed = invoke(binary, "service", "health", "--runtime-dir", control)
                            if observed.get("observation", {}).get("runtime_sha256") == identity:
                                break
                        except RuntimeError:
                            pass
                        if time.monotonic() >= until:
                            raise RuntimeError("Foreground dispatch did not use the pinned writable-state namespace")
                        time.sleep(0.1)
                    if (install / "control").exists():
                        raise RuntimeError("Foreground dispatch recreated legacy install-root state")
                    invoke(binary, "service", "shutdown", "--runtime-dir", control)
                    if foreground.wait(timeout=30):
                        raise RuntimeError("Foreground launcher did not preserve clean service exit")
                finally:
                    if foreground.poll() is None:
                        subprocess.run([str(binary), "service", "shutdown", "--runtime-dir", str(control)],
                                       stdout=subprocess.DEVNULL, stderr=subprocess.DEVNULL,
                                       timeout=30, env=environment, creationflags=flags)
                        try:
                            foreground.wait(timeout=30)
                        except subprocess.TimeoutExpired:
                            foreground.kill()
                            foreground.wait(timeout=30)
                health = run("ensure", "--install-dir", install)
            # Earlier steps deliberately restart the same approved payload several
            # times. Observe the documented healthy interval before testing another
            # stop/upgrade/rollback sequence; do not reset or bypass its crash circuit.
            print("Observing the stable-owner interval before upgrade/rollback...", flush=True)
            stable_deadline = time.monotonic() + 90
            while health["uptime_ms"] < 60_000:
                if time.monotonic() >= stable_deadline:
                    raise RuntimeError("Owned runtime did not sustain its healthy startup interval")
                time.sleep(1)
                health = run("ensure", "--install-dir", install)
            # A legal-payload change deliberately retains identical native executable
            # hashes. Health must still distinguish these approved installation identities.
            next_payload = fixture / "next-payload"
            shutil.copytree(payload, next_payload)
            license_path = next_payload / "LICENSE"
            license_path.write_bytes(license_path.read_bytes() + b"\nOriginal lifecycle fixture marker.\n")
            manifest_path = next_payload / "airk-install.json"
            manifest = json.loads(manifest_path.read_text(encoding="utf-8"))
            manifest["files"]["LICENSE"] = sha256(license_path)
            manifest_path.write_text(json.dumps(manifest, indent=2) + "\n", encoding="utf-8")
            next_identity = sha256(manifest_path)
            stage(next_payload, next_identity)
            print("Checking exact-payload upgrade and rollback...", flush=True)
            second = run("activate", "--install-dir", install, "--runtime-sha256", next_identity)
            if second != {"schema": 1, "active": next_identity, "previous": identity}:
                raise RuntimeError("Upgrade did not preserve a verified rollback selection")
            next_health = run("ensure", "--install-dir", install)
            if next_health["runtime_sha256"] != next_identity or next_health["instance"] == health["instance"]:
                raise RuntimeError("Upgrade reused the preceding service instead of draining it")
            restored = run("rollback", "--install-dir", install)
            if restored["active"] != identity or restored["previous"] != next_identity:
                raise RuntimeError("Rollback did not restore the preceding verified payload")
            if run("ensure", "--install-dir", install)["runtime_sha256"] != identity:
                raise RuntimeError("Rollback health points to the wrong runtime")
            # This private fixture deliberately restarts the same healthy image
            # several times within the two-minute crash-circuit window. Start
            # the independent retirement scenario with a fresh attempt budget;
            # retain the production circuit and its dedicated regression tests.
            if run("reset-startup", "--install-dir", install).get("startup_circuit") != "reset":
                raise RuntimeError("Fixture startup budget was not reset")
            print("Checking previous runtime retirement and interrupted cleanup...", flush=True)
            run("retire-previous", "--install-dir", install, "--expected-active", next_identity, rejected=True)
            unknown_previous = install / "runtimes" / next_identity / "unrecognized.md"
            unknown_previous.write_text("Original retirement preservation fixture.\n", encoding="utf-8")
            run("retire-previous", "--install-dir", install, "--expected-active", identity, rejected=True)
            if unknown_previous.read_text(encoding="utf-8") != "Original retirement preservation fixture.\n":
                raise RuntimeError("Retirement changed an unknown file")
            if run("ensure", "--install-dir", install)["runtime_sha256"] != identity:
                raise RuntimeError("Deferred retirement did not restore selected service health")
            run("activate", "--install-dir", install, "--runtime-sha256", next_identity, rejected=True)
            unknown_previous.unlink()
            retired = run("retire-previous", "--install-dir", install, "--expected-active", identity)
            if retired.get("active") != identity or retired.get("retired_previous") != next_identity:
                raise RuntimeError("Retirement receipt lost the exact selected/previous identities")
            if (install / "runtimes" / next_identity).exists():
                raise RuntimeError("Previous runtime executable payload remains after retirement")
            if run("inspect", "--install-dir", install)["selection"]["previous"] is not None:
                raise RuntimeError("Retirement retained a rollback reference")
            if run("ensure", "--install-dir", install)["runtime_sha256"] != identity:
                raise RuntimeError("Retirement lost the selected service")
            if run("retire-previous", "--install-dir", install, "--expected-active", identity).get("retired_previous") is not None:
                raise RuntimeError("Completed retirement was not idempotent")
            stage(next_payload, next_identity)
            foreign = install / "runtimes" / next_identity / "unrecognized.md"
            foreign.write_text("Original unknown file fixture.\n", encoding="utf-8")
            print("Checking resumable owned uninstall...", flush=True)
            removal = run("uninstall", "--install-dir", install)
            if removal.get("uninstalled") or next_identity not in removal.get("deferred_runtimes", []):
                raise RuntimeError("Uninstall failed to preserve an unknown payload file")
            if foreign.read_text(encoding="utf-8") != "Original unknown file fixture.\n":
                raise RuntimeError("Unknown file was changed")
            diagnosis = run("diagnose", "--install-dir", install)
            if not diagnosis.get("runtime_error") or diagnosis.get("bootstrap") is None:
                raise RuntimeError("Down-service bootstrap diagnosis lost independent evidence")
            foreign.unlink()
            if not run("uninstall", "--install-dir", install).get("uninstalled"):
                raise RuntimeError("Owned uninstall did not resume after the unknown file was removed")
            if (user_data / "sentinel").read_bytes() != original:
                raise RuntimeError("Runtime uninstall changed project data")
            if not control.is_dir() or sha256(state_owner) != state_owner_digest:
                raise RuntimeError("Runtime uninstall removed durable service state or ownership")
            if modern and (http_source / "sentinel.txt").read_bytes() != original:
                raise RuntimeError("Native lifecycle changed the owned HTTPS source")
            check_payload_unchanged(payload, inventory, identity)
            print("Offline runtime staging, election, exact payload health, versioned client lifecycle, upgrade, rollback, and resumable uninstall passed.")
        finally:
            # Never operate on an installed user service: this exact private control
            # namespace belongs to the temporary fixture above.
            if control.exists():
                subprocess.run([str(binary), "service", "shutdown", "--runtime-dir", str(control)],
                               stdout=subprocess.DEVNULL, stderr=subprocess.DEVNULL,
                               timeout=30, env=environment, creationflags=flags)


if __name__ == "__main__":
    main()
