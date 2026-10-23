# SPDX-License-Identifier: Apache-2.0
"""Offline native lifecycle fixture; installs only into its owned temporary directory."""
import argparse
import hashlib
import json
import os
from pathlib import Path
import shutil
import subprocess
import tempfile
import time


def sha256(path):
    with path.open("rb") as stream:
        return hashlib.file_digest(stream, "sha256").hexdigest()


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("--runtime", type=Path, required=True)
    args = parser.parse_args()
    payload = args.runtime.resolve(strict=True)
    binary = payload / ("airk.exe" if os.name == "nt" else "airk")
    identity = sha256(payload / "airk-install.json")
    version = json.loads((payload / "airk-install.json").read_text(encoding="utf-8"))["version"]
    environment = dict(os.environ, HTTP_PROXY="http://127.0.0.1:1",
                       HTTPS_PROXY="http://127.0.0.1:1", ALL_PROXY="http://127.0.0.1:1")
    flags = getattr(subprocess, "CREATE_NO_WINDOW", 0)

    def invoke(executable, *arguments, rejected=False, raw=False):
        result = subprocess.run([str(executable), *map(str, arguments)], capture_output=True,
                                encoding="utf-8", env=environment, timeout=180,
                                creationflags=flags)
        if rejected:
            if not result.returncode:
                raise RuntimeError("Expected native policy/inventory rejection")
            return None
        if result.returncode:
            raise RuntimeError(result.stderr[-4096:])
        return result.stdout.strip() if raw else json.loads(result.stdout)

    with tempfile.TemporaryDirectory(prefix="airk install original café fixture-", dir=Path(__file__).resolve().parents[1] / "target") as temporary:
        fixture = Path(temporary)
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
            health = run("ensure", "--install-dir", install)
            if health["runtime_sha256"] != identity:
                raise RuntimeError("Health does not bind the installed payload")
            if run("ensure", "--install-dir", install)["instance"] != health["instance"]:
                raise RuntimeError("Idempotent ensure launched another instance")
            inspection = run("inspect", "--install-dir", install)
            if Path(inspection["control_dir"]).resolve() != control.resolve() or (install / "control").exists():
                raise RuntimeError("Service state was not isolated from executable installation")
            state_owner = state / "airk-state-1.4.owner.json"
            state_owner_digest = sha256(state_owner)
            launcher = Path(inspection["launcher"])
            actual = invoke(launcher, "runtime", "launch", "--install-dir", install,
                            "--", "--version", raw=True)
            if actual != f"airk {version}":
                raise RuntimeError("Stable launcher changed stdio or executable identity")
            plan = run("login", "--install-dir", install, "plan")
            if not plan["id"].startswith("airksvc-") or plan["arguments"][0:2] != ["runtime", "foreground"]:
                raise RuntimeError("Optional startup plan reused a legacy scheduler identity")
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
            print("Offline runtime staging, election, exact payload health, upgrade, rollback, stable launcher repair, and resumable uninstall passed.")
        finally:
            # Never operate on an installed user service: this exact private control
            # namespace belongs to the temporary fixture above.
            if control.exists():
                subprocess.run([str(binary), "service", "shutdown", "--runtime-dir", str(control)],
                               stdout=subprocess.DEVNULL, stderr=subprocess.DEVNULL,
                               timeout=30, env=environment, creationflags=flags)


if __name__ == "__main__":
    main()
