# SPDX-License-Identifier: Apache-2.0
"""Opt-in owned macOS schema-2 SMAppService and HTTPS acceptance.

The fixed preview registration must be unused. This never changes Login Items
consent or a user's existing installation. Failed/approval-pending fixtures stay
intact; only their canonical native receipt can authorize cleanup.
"""
import argparse
import ctypes
import errno
import hashlib
import importlib.util
import json
import os
from pathlib import Path
import platform
import stat
import subprocess
import tempfile
import time

ROOT = Path(__file__).resolve().parents[1]
LABEL = "io.iokaio.airk.service.preview"
TEAM = "5PX7D99PXA"
LEAF = "4d8496dcab73794378194d040d9177fb28a0f982"


def load(name, filename):
    spec = importlib.util.spec_from_file_location(name, ROOT / "scripts" / filename)
    module = importlib.util.module_from_spec(spec)
    spec.loader.exec_module(module)
    return module


def digest(path):
    with path.open("rb") as source:
        return hashlib.file_digest(source, "sha256").hexdigest()


def job_observation(text, main, plist):
    """Independently bind launchd's top-level owner, argv and PID."""
    if len(text.encode()) > 65536:
        raise ValueError("Owned launchd observation exceeds allowance")
    depth = 0
    fields, arguments = {}, []
    in_arguments = False
    for line in text.splitlines():
        value = line.strip()
        if depth == 1:
            if value == "arguments = {":
                if "arguments" in fields:
                    raise ValueError("Duplicate launchd arguments")
                fields["arguments"] = True
                in_arguments = True
            for key in ("path", "program", "pid"):
                prefix = key + " = "
                if value.startswith(prefix):
                    if key in fields:
                        raise ValueError("Duplicate launchd owner field")
                    fields[key] = value[len(prefix):]
        elif in_arguments and depth == 2 and value != "}":
            if len(arguments) >= 32 or len(value) > 4096:
                raise ValueError("Owned launchd argument allowance exceeded")
            arguments.append(value)
        if in_arguments and depth == 2 and value == "}":
            in_arguments = False
        depth += int(value.endswith("{")) - int(value == "}")
        if not 0 <= depth <= 16:
            raise ValueError("Unknown launchd observation structure")
    expected = ["airk", "runtime", "login-serve", "--channel", "preview"]
    if (depth != 0 or fields.get("path") != str(plist) or fields.get("program") != str(main)
            or arguments != expected or not fields.get("pid", "").isdigit()):
        raise ValueError("Owned launchd program/plist/argv/PID differs")
    pid = int(fields["pid"])
    if not 1 <= pid <= 2**31 - 1:
        raise ValueError("Invalid owned launchd PID")
    return {"program": fields["program"], "path": fields["path"], "arguments": arguments, "process_id": pid}


def require_unregistered(status):
    if (status.get("installed") is not False or status.get("loaded") is not False
            or status.get("running") is not False or status.get("manager_output") != "not_registered"):
        raise RuntimeError("Existing or unverifiable preview registration is preserved")


def require_stopped_registration(status):
    if (status.get("installed") is not True or status.get("loaded") is not False
            or status.get("running") is not False or status.get("manager_output") != "not_registered"):
        raise RuntimeError("Owned login Stop did not retain its exact inactive registration intent")


def require_selected_lifecycle(observation, identity, main, explicitly_stopped):
    lifecycle = observation.get("lifecycle", {})
    if (observation.get("payload_verified") is not True or observation.get("selection", {}).get("active") != identity
            or observation.get("executable") != str(main)
            or lifecycle.get("explicitly_stopped") is not explicitly_stopped or lifecycle.get("pending") is not None):
        raise RuntimeError("Owned login lifecycle changed approved selection or durable Stop state")


def wait_exact_exit(pid, main):
    deadline = time.monotonic() + 45
    while (current := process_path(pid)) is not None:
        if current != main or time.monotonic() >= deadline:
            raise RuntimeError("Exact owned service PID exit remains unverified")
        time.sleep(.05)


def process_path(pid):
    library = ctypes.CDLL("/usr/lib/libproc.dylib", use_errno=True)
    function = library.proc_pidpath
    function.argtypes = [ctypes.c_int, ctypes.c_void_p, ctypes.c_uint32]
    function.restype = ctypes.c_int
    buffer = ctypes.create_string_buffer(4096)
    ctypes.set_errno(0)
    count = function(pid, buffer, len(buffer))
    if count <= 0:
        observed = ctypes.get_errno()
        if observed == errno.ESRCH:
            return None
        raise OSError(observed, "Owned process path cannot be established")
    return Path(buffer.value.decode("utf-8", errors="strict"))


def main(argv=None):
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("--runtime", type=Path, required=True)
    parser.add_argument("--approved-sha256", required=True)
    parser.add_argument("--evidence-parent", type=Path, required=True)
    parser.add_argument("--allow-preview-registration", action="store_true", required=True,
                        help="Allow one private fixture to register the otherwise unused fixed preview label")
    args = parser.parse_args(argv)
    if platform.system() != "Darwin" or os.getuid() == 0:
        parser.error("Requires a standard user's native Mac GUI session")
    payload = args.runtime.resolve(strict=True)
    inventory = json.loads((payload / "airk-install.json").read_text())
    runtime_metadata = json.loads((payload / "airk-runtime.json").read_text())
    if (digest(payload / "airk-install.json") != args.approved_sha256 or inventory.get("schema") != 1
            or runtime_metadata.get("schema") != 2 or runtime_metadata.get("version") != inventory.get("version")
            or inventory.get("os") != "macos" or tuple(map(int, inventory["version"].split("."))) < (1, 8, 14)):
        parser.error("Expected the exact explicitly approved modern Mac runtime")
    binary = payload / "AIRK.app/Contents/MacOS/airk"
    parent = args.evidence_parent.resolve(strict=True)
    if parent.is_symlink() or parent.stat().st_uid != os.getuid():
        parser.error("Evidence parent must belong to the current user")
    fixture = Path(tempfile.mkdtemp(prefix="airk-mac-direct-", dir=parent)).resolve(strict=True)
    fixture.chmod(0o700)
    install, state, source, data = [fixture / name for name in ("install", "state", "source", "index-state")]
    source.mkdir(mode=0o700)
    sentinel = source / "sentinel.txt"
    sentinel.write_text("Exact owned Mac direct-service HTTPS evidence.\n", encoding="utf-8")
    source_sha = digest(sentinel)
    control = state / "control"
    uid = os.getuid()
    target = f"gui/{uid}/{LABEL}"
    report = {"schema": 1, "result": "running", "fixture": str(fixture), "runtime_sha256": args.approved_sha256,
              "version": inventory["version"], "uid": uid, "architecture": platform.machine(), "commands": [],
              "scope": "standard-user preview SMAppService sealed main and actual installed HTTPS; no Intel or oldest-OS claim"}
    started = time.monotonic()
    registration_attempted = installed = False
    failure = cleanup_error = None
    helper = load("mac_direct_installation_fixture", "check-service-install.py")
    environment = dict(os.environ)
    environment.update(load("mac_direct_git_fixture", "check-runtime-upgrade.py").isolated_git_environment(fixture))

    def retain():
        report["elapsed_seconds"] = round(time.monotonic() - started, 3)
        report["source_preserved"] = sentinel.is_file() and digest(sentinel) == source_sha
        destination = fixture / "qualification.json"
        destination.write_text(json.dumps(report, indent=2) + "\n", encoding="utf-8")
        destination.chmod(0o600)

    def execute(command, *, input=None, rejected=False, rejected_reason=None, json_result=False, timeout=180):
        tick = time.monotonic()
        result = subprocess.run(list(map(str, command)), stdin=None if input is not None else subprocess.DEVNULL,
                                input=input, capture_output=True, text=True, encoding="utf-8", timeout=timeout, env=environment)
        record = {"argv": list(map(str, command)), "exit_code": result.returncode,
                  "elapsed_seconds": round(time.monotonic() - tick, 3), "stdout_sha256": hashlib.sha256(result.stdout.encode()).hexdigest(),
                  "stderr": result.stderr[-4096:]}
        report["commands"].append(record); retain()
        if rejected:
            if not result.returncode:
                raise RuntimeError("Expected exact fixture policy refusal")
            if rejected_reason is not None and rejected_reason not in result.stderr:
                raise RuntimeError("Owned refusal did not report its expected native policy reason")
            return None
        if result.returncode:
            raise RuntimeError(f"Owned command refused ({result.returncode}): {result.stderr[-4096:]}")
        return json.loads(result.stdout) if json_result else result.stdout

    def invoke(executable, *arguments, rejected=False, rejected_reason=None, raw=False, input=None):
        return execute([executable, *arguments], rejected=rejected, rejected_reason=rejected_reason, input=input, json_result=not raw)

    def runtime(*arguments, **kwargs):
        return invoke(binary, "runtime", *arguments, **kwargs)

    def launchd_absent():
        result = subprocess.run(["/bin/launchctl", "print", target], capture_output=True, text=True, timeout=15)
        if result.returncode == 0 or f'Could not find service "{LABEL}"' not in result.stderr:
            raise RuntimeError("Existing or unverifiable fixed preview launchd job is preserved")

    def managed_identity():
        health = runtime("ensure", "--install-dir", install)
        installed_main = install / "runtimes" / args.approved_sha256 / "AIRK.app/Contents/MacOS/airk"
        plist = installed_main.parents[1] / "Library/LaunchAgents" / f"{LABEL}.plist"
        text = execute(["/bin/launchctl", "print", target], timeout=15)
        job = job_observation(text, installed_main, plist)
        if (job["process_id"] != health["process_id"] or health["runtime_sha256"] != args.approved_sha256
                or health["executable_sha256"] != digest(installed_main)
                or process_path(job["process_id"]) != installed_main):
            raise RuntimeError("Registered launchd PID is not the authenticated sealed service main")
        report.setdefault("managed_identities", []).append({"health": health, "launchd": job}); retain()
        return health, installed_main

    try:
        console = execute(["/usr/bin/stat", "-f", "%u", "/dev/console"], timeout=15).strip()
        if console != str(uid):
            raise RuntimeError("SSH and active GUI users differ; no registration attempted")
        launchd_absent()
        requirement = f'=anchor apple generic and certificate leaf[subject.OU] = "{TEAM}" and certificate leaf[field.1.2.840.113635.100.6.1.13] exists and certificate leaf = H"{LEAF}"'
        execute(["/usr/bin/codesign", "--verify", "--strict", "--verbose=2", "-R", requirement, payload / "AIRK.app"])
        execute(["/usr/bin/xcrun", "stapler", "validate", payload / "AIRK.app"])
        execute(["/usr/sbin/spctl", "--assess", "--type", "execute", "--verbose=4", payload / "AIRK.app"])
        runtime("stage", "--source", payload, "--install-dir", install, "--state-dir", state,
                "--approved-sha256", args.approved_sha256, "--verification-basis", "trusted-distribution", "--channel", "preview")
        installed = True
        runtime("activate", "--install-dir", install, "--runtime-sha256", args.approved_sha256)
        before = runtime("login", "--install-dir", install, "status")
        require_unregistered(before)
        initial = runtime("ensure", "--install-dir", install)
        initial_main = install / "runtimes" / args.approved_sha256 / "AIRK.app/Contents/MacOS/airk"
        if initial["executable_sha256"] != digest(initial_main) or process_path(initial["process_id"]) != initial_main:
            raise RuntimeError("Initial fixture service identity differs")
        invoke(binary, "service", "shutdown", "--runtime-dir", control)
        wait_exact_exit(initial["process_id"], initial_main)
        registration_attempted = True
        runtime("login", "--install-dir", install, "install")
        health, installed_main = managed_identity()

        # Public registration Stop/Start is distinct from durable UserStop.
        # Retain the owned registration intent while proving the exact old
        # sealed main exited; only the public Start may register it again.
        selected_before = runtime("inspect", "--install-dir", install)
        require_selected_lifecycle(selected_before, args.approved_sha256, installed_main, False)
        stopped = runtime("login", "--install-dir", install, "stop")
        require_stopped_registration(stopped)
        launchd_absent()
        wait_exact_exit(health["process_id"], installed_main)
        require_stopped_registration(runtime("login", "--install-dir", install, "status"))
        stopped_inspect = runtime("inspect", "--install-dir", install)
        require_selected_lifecycle(stopped_inspect, args.approved_sha256, installed_main, False)
        started_status = runtime("login", "--install-dir", install, "start")
        restarted, restarted_main = managed_identity()
        if restarted["instance"] == health["instance"] or restarted_main != installed_main:
            raise RuntimeError("Public login Start did not create a fresh owner of the same approved main")
        report["public_login_stop_start"] = {"before":health, "stopped_status":stopped,
                                            "stopped_inspection":stopped_inspect,
                                            "start_status":started_status, "after":restarted}
        retain()
        health = restarted

        def login_cannot_bypass_user_stop():
            before = runtime("inspect", "--install-dir", install)
            require_selected_lifecycle(before, args.approved_sha256, installed_main, True)
            require_stopped_registration(runtime("login", "--install-dir", install, "status"))
            launchd_absent()
            wait_exact_exit(health["process_id"], installed_main)
            runtime("login", "--install-dir", install, "start", rejected=True,
                    rejected_reason="service_explicitly_stopped")
            after = runtime("inspect", "--install-dir", install)
            require_selected_lifecycle(after, args.approved_sha256, installed_main, True)
            for field in ("selection", "lifecycle", "registry_namespace", "executable", "approval"):
                if before[field] != after[field]:
                    raise RuntimeError("Refused login Start changed durable UserStop authority")
            launchd_absent()
            if process_path(health["process_id"]) is not None:
                raise RuntimeError("Refused login Start left an unverified old PID")
            report["public_login_start_preserved_user_stop"] = {"before":before, "after":after}
            retain()

        health = helper.modern_client_lifecycle(invoke, binary, installed_main, install, control, source, data,
                                                health, fixture / "https-lifecycle-private.json",
                                                stopped_probe=login_cannot_bypass_user_stop)
        (fixture / "https-lifecycle-private.json").chmod(0o600)
        after, _ = managed_identity()
        if after["instance"] != health["instance"]:
            raise RuntimeError("OS-managed HTTPS lifecycle returned another service instance")
        runtime("login", "--install-dir", install, "uninstall")
        registration_attempted = False
        launchd_absent()
        runtime("uninstall", "--install-dir", install)
        installed = False
        if digest(sentinel) != source_sha:
            raise RuntimeError("Original fixture source changed")
        report["result"] = "passed"
    except BaseException as error:
        failure = error
        report["result"] = "failed_retained"
        report["error"] = str(error)
        if registration_attempted:
            try:
                status = runtime("login", "--install-dir", install, "status")
                report["failure_login_status"] = status
                if status.get("manager_output") == "requires_approval":
                    report["result"] = "approval_pending_retained"
                else:
                    runtime("login", "--install-dir", install, "uninstall")
                    registration_attempted = False
            except Exception as cleanup:
                cleanup_error = str(cleanup)
        if installed and not registration_attempted:
            try:
                runtime("uninstall", "--install-dir", install)
            except Exception as cleanup:
                cleanup_error = str(cleanup)
    finally:
        report["cleanup_error"] = cleanup_error
        report["registration_retained"] = registration_attempted
        retain()
        print(f"Mac direct-service {report['result']}; evidence: {fixture / 'qualification.json'}", flush=True)
    if failure is not None:
        raise RuntimeError("Owned Mac qualification did not pass; inspect its retained receipt") from failure


if __name__ == "__main__":
    main()
