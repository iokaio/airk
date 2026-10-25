# SPDX-License-Identifier: Apache-2.0
"""Crash and resume an approved activation in an isolated namespace; optionally test its owned login manager."""
import argparse
import hashlib
import json
import os
from pathlib import Path
import shutil
import subprocess
import tempfile
import time


def digest(path):
    with path.open("rb") as stream:
        return hashlib.file_digest(stream, "sha256").hexdigest()


def read_intent(path):
    # A polling fixture must allow native atomic replacement of the same metadata.
    # The Windows C runtime's ordinary file sharing need not include delete access.
    if os.name == "nt":
        import ctypes
        import msvcrt
        from ctypes import wintypes
        kernel = ctypes.WinDLL("kernel32", use_last_error=True)
        create = kernel.CreateFileW
        create.argtypes = [wintypes.LPCWSTR, wintypes.DWORD, wintypes.DWORD,
                           wintypes.LPVOID, wintypes.DWORD, wintypes.DWORD, wintypes.HANDLE]
        create.restype = wintypes.HANDLE
        handle = create(str(path), 0x80000000, 7, None, 3, 0x80, None)
        if handle == ctypes.c_void_p(-1).value:
            error = ctypes.get_last_error()
            if error in (2, 3):
                raise FileNotFoundError(str(path))
            raise ctypes.WinError(error)
        try:
            descriptor = msvcrt.open_osfhandle(handle, os.O_RDONLY | os.O_BINARY)
        except BaseException:
            close = kernel.CloseHandle
            close.argtypes = [wintypes.HANDLE]
            close(handle)
            raise
        with os.fdopen(descriptor, "r", encoding="utf-8") as stream:
            return json.loads(stream.read(16385))
    return json.loads(path.read_text(encoding="utf-8"))


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("--runtime", type=Path, required=True)
    parser.add_argument("--login-manager", action="store_true",
                        help="Create and remove only this fixture's unique per-user login registration")
    args = parser.parse_args()
    payload = args.runtime.resolve(strict=True)
    binary = payload / ("airk.exe" if os.name == "nt" else "airk")
    if not binary.is_file():
        binary = payload / "AIRK.app/Contents/MacOS/airk"
    flags = getattr(subprocess, "CREATE_NO_WINDOW", 0)
    environment = dict(os.environ, HTTP_PROXY="http://127.0.0.1:1", HTTPS_PROXY="http://127.0.0.1:1",
                       ALL_PROXY="http://127.0.0.1:1")
    def exercise(temporary):
        fixture = Path(temporary)
        root = fixture / "install"
        state = fixture / "state"
        control = state / "control"
        installed_manager = False
        activation = None

        def command(*arguments, rejected=False):
            result = subprocess.run([str(binary), "runtime", *map(str, arguments)], cwd=fixture,
                                    capture_output=True, encoding="utf-8", timeout=300,
                                    env=environment, creationflags=flags)
            if rejected:
                if result.returncode == 0:
                    raise RuntimeError("Expected unsettled activation refusal")
                return None
            if result.returncode:
                raise RuntimeError(result.stderr[-4096:])
            return json.loads(result.stdout)

        def await_phase(phase):
            until = time.monotonic() + 300
            last_phase = None
            while time.monotonic() < until:
                if activation.poll() is not None:
                    raise RuntimeError("Activation exited before the crash boundary")
                try:
                    intent = read_intent(root / "activation.json")
                    last_phase = intent.get("phase")
                    if intent.get("phase") == phase:
                        return
                except FileNotFoundError:
                    pass
                time.sleep(0.005)
            raise RuntimeError(f"Activation did not reach {phase}; last observed phase: {last_phase}")

        def interrupt_at(identity, phase):
            nonlocal activation
            activation = subprocess.Popen([str(binary), "runtime", "activate", "--install-dir", str(root),
                                           "--runtime-sha256", identity], cwd=fixture,
                                          stdin=subprocess.DEVNULL, stdout=subprocess.DEVNULL,
                                          stderr=subprocess.DEVNULL, env=environment, creationflags=flags)
            await_phase(phase)
            activation.kill()
            activation.wait(timeout=30)
            activation = None

        try:
            first = digest(payload / "airk-install.json")
            print("Staging the initial approved payload...", flush=True)
            command("stage", "--source", payload, "--install-dir", root, "--state-dir", state,
                    "--approved-sha256", first, "--verification-basis", "organization-digest")
            command("activate", "--install-dir", root, "--runtime-sha256", first)
            if args.login_manager:
                print("Registering only the fixture's uniquely named per-user startup definition...", flush=True)
                # Mark before invocation: registration can succeed before a subsequent
                # native proof/startup failure. Cleanup must inspect that receipt too.
                installed_manager = True
                command("login", "--install-dir", root, "install")
                owned = command("login", "--install-dir", root, "status")
                if not owned.get("installed") or owned.get("registered_definition_matches") is not True:
                    raise RuntimeError("Actual OS-held startup configuration did not match the approved definition")
                command("ensure", "--install-dir", root)
            candidate = fixture / "candidate payload"
            shutil.copytree(payload, candidate)
            manifest = json.loads((candidate / "airk-install.json").read_text(encoding="utf-8"))
            # The same code with a separately approved legal inventory still has a
            # different complete payload identity, which native health must bind.
            with (candidate / "LICENSE").open("a", encoding="utf-8") as stream:
                stream.write("\nIsolated fictional activation fixture inventory marker.\n")
            manifest["files"]["LICENSE"] = digest(candidate / "LICENSE")
            (candidate / "airk-install.json").write_text(json.dumps(manifest, indent=2) + "\n", encoding="utf-8")
            second = digest(candidate / "airk-install.json")
            command("stage", "--source", candidate, "--install-dir", root, "--approved-sha256", second,
                    "--verification-basis", "organization-digest")
            print("Interrupting before drain and explicitly restoring the preceding selection...", flush=True)
            interrupt_at(second, "drain")
            command("abort-activation", "--install-dir", root)
            if command("ensure", "--install-dir", root)["runtime_sha256"] != first:
                raise RuntimeError("Aborting drain did not restore the exact preceding payload")
            print("Interrupting candidate probing and resuming through ensure...", flush=True)
            interrupt_at(second, "probe")
            pending = json.loads((root / "selected.json").read_text(encoding="utf-8"))
            if pending["active"] != first:
                raise RuntimeError("Public selection advanced before candidate health")
            command("remove", "--install-dir", root, "--runtime-sha256", second, rejected=True)
            resumed = command("ensure", "--install-dir", root)
            selected = json.loads((root / "selected.json").read_text(encoding="utf-8"))
            if resumed["runtime_sha256"] != second or selected["active"] != second or selected["previous"] != first:
                raise RuntimeError("Interrupted activation did not commit the exact healthy candidate")
            diagnosis = command("diagnose", "--install-dir", root)
            if diagnosis.get("pending_activation") or diagnosis.get("activation_result", {}).get("outcome") != "selected":
                raise RuntimeError("Activation completion lost its independent durable receipt")
            if args.login_manager:
                owned = command("login", "--install-dir", root, "status")
                if owned.get("running") is not True or owned.get("registered_definition_matches") is not True:
                    raise RuntimeError("OS-owned candidate was replaced by an on-demand process")
                command("login", "--install-dir", root, "uninstall")
                installed_manager = False
            command("uninstall", "--install-dir", root)
            if digest(payload / "airk-install.json") != first or digest(candidate / "airk-install.json") != second:
                raise RuntimeError("Activation changed its approved source inventories")
            print("Durable drain/probe interruption, explicit abort, exact candidate recovery and owned cleanup passed.")
        finally:
            if activation is not None:
                activation.kill()
                activation.wait(timeout=30)
            if (root / "activation.json").exists():
                intent = read_intent(root / "activation.json")
                selected_path = root / "selected.json"
                selected = json.loads(selected_path.read_text(encoding="utf-8")) if selected_path.exists() else {}
                if selected.get("active") == intent.get("candidate"):
                    command("ensure", "--install-dir", root)
                else:
                    command("abort-activation", "--install-dir", root)
            if installed_manager:
                status = command("login", "--install-dir", root, "status")
                if status.get("installed"):
                    command("login", "--install-dir", root, "uninstall")
                elif status.get("loaded") is not False:
                    raise RuntimeError("OS startup state remains unknown or unowned; preserve the fixture")
                if (root / "startup-owner.json").exists():
                    raise RuntimeError(f"Fixture startup ownership remains; preserve this directory for cleanup: {root}")
            if control.exists():
                subprocess.run([str(binary), "service", "shutdown", "--runtime-dir", str(control)], cwd=fixture,
                               stdout=subprocess.DEVNULL, stderr=subprocess.DEVNULL, timeout=30,
                               env=environment, creationflags=flags)
            if (root / "runtime-layout.json").exists():
                # Prove all native owners have exited before removing this fixture.
                # Failure preserves the complete directory for bounded diagnosis.
                command("uninstall", "--install-dir", root)

    temporary = tempfile.mkdtemp(prefix="airk activation café fixture-", dir=Path(__file__).resolve().parents[1] / "target")
    try:
        exercise(temporary)
    except BaseException:
        print(f"Preserving activation fixture for diagnosis and owned cleanup: {temporary}", flush=True)
        raise
    else:
        shutil.rmtree(temporary)


if __name__ == "__main__":
    main()
