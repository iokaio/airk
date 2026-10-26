# SPDX-License-Identifier: Apache-2.0
"""Exercise approved read-only payloads in isolated user and external-supervisor namespaces."""
import argparse
import hashlib
import json
import os
from pathlib import Path
import shutil
import stat
import subprocess
import tempfile
import time


def digest(path):
    with path.open("rb") as stream:
        return hashlib.file_digest(stream, "sha256").hexdigest()


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("--runtime", type=Path, required=True)
    args = parser.parse_args()
    payload = args.runtime.resolve(strict=True)
    flags = getattr(subprocess, "CREATE_NO_WINDOW", 0)
    environment = dict(os.environ, HTTP_PROXY="http://127.0.0.1:1",
                       HTTPS_PROXY="http://127.0.0.1:1", ALL_PROXY="http://127.0.0.1:1")
    with tempfile.TemporaryDirectory(prefix="airk managed café fixture-", dir=Path(__file__).resolve().parents[1] / "target") as temporary:
        fixture = Path(temporary)
        provider = fixture / "organization payload"
        shutil.copytree(payload, provider)
        binary = provider / ("airk.exe" if os.name == "nt" else "airk")
        if not binary.is_file():
            binary = provider / "AIRK.app/Contents/MacOS/airk"
        approved = digest(provider / "airk-install.json")
        files = sorted(path for path in provider.rglob("*") if path.is_file())
        before = {str(path.relative_to(provider)): digest(path) for path in files}
        directories = sorted((path for path in provider.rglob("*") if path.is_dir()), key=lambda path: len(path.parts))
        for path in files:
            path.chmod(stat.S_IRUSR | (stat.S_IXUSR if path.name in ("airk", "airk-embed", "airk.exe", "airk-embed.exe") else 0))
        if os.name != "nt":
            for path in reversed([provider, *directories]):
                path.chmod(stat.S_IRUSR | stat.S_IXUSR)

        def invoke(*arguments, rejected=False):
            result = subprocess.run([str(binary), *map(str, arguments)], cwd=fixture,
                                    capture_output=True, encoding="utf-8", timeout=180,
                                    env=environment, creationflags=flags)
            if rejected:
                if not result.returncode:
                    raise RuntimeError("Expected managed lifecycle policy refusal")
                return None
            if result.returncode:
                raise RuntimeError(result.stderr[-4096:])
            return json.loads(result.stdout)

        controls = []
        supervisor = None
        try:
            for lifecycle in ("user-on-demand", "external"):
                root = fixture / (lifecycle + " metadata")
                state = fixture / (lifecycle + " state")
                control = state / "control"
                controls.append(control)
                print(f"Checking managed {lifecycle} with a read-only provider payload...", flush=True)
                configured = invoke("runtime", "managed", "--source", provider, "--install-dir", root,
                                    "--state-dir", state, "--approved-sha256", approved,
                                    "--verification-basis", "organization-digest", "--lifecycle", lifecycle)
                if (root / "runtimes").exists() or (root / "airk-launcher.exe").exists() or (root / "airk-launcher").exists():
                    raise RuntimeError("Managed configuration copied an executable into writable user metadata")
                if lifecycle == "external":
                    if configured.get("selected") or not configured.get("awaiting_external_supervisor"):
                        raise RuntimeError("Unavailable external runtime was selected or hidden")
                    if (root / "selected.json").exists() or (root / "bootstrap.json").exists() or list(root.glob("startup-*")):
                        raise RuntimeError("External managed mode started a competing user process")
                    supervisor = subprocess.Popen([str(binary), "serve", "--runtime-dir", str(control)], cwd=fixture,
                                                  stdin=subprocess.DEVNULL, stdout=subprocess.DEVNULL,
                                                  stderr=subprocess.DEVNULL, env=environment, creationflags=flags)
                    until = time.monotonic() + 60
                    while True:
                        if supervisor.poll() is not None:
                            raise RuntimeError("Owned external-supervisor fixture exited before health")
                        try:
                            observed = invoke("service", "health", "--runtime-dir", control)
                            if observed.get("observation", {}).get("runtime_sha256") == approved:
                                break
                        except RuntimeError:
                            pass
                        if time.monotonic() >= until:
                            raise RuntimeError("External runtime did not become healthy")
                        time.sleep(0.1)
                elif not configured.get("selected"):
                    raise RuntimeError("Approved user-on-demand runtime did not commit healthy selection")
                healthy = invoke("runtime", "ensure", "--install-dir", root)
                if healthy["runtime_sha256"] != approved:
                    raise RuntimeError("Managed health does not bind the complete approved payload")
                if supervisor is not None and healthy["process_id"] != supervisor.pid:
                    raise RuntimeError("Managed ensure used another process instead of the external supervisor")
                inspected = invoke("runtime", "inspect", "--install-dir", root)
                if not Path(inspected["executable"]).samefile(binary) or not Path(inspected["control_dir"]).samefile(control):
                    raise RuntimeError("Managed inspection lost provider/state identity")
                invoke("runtime", "activate", "--install-dir", root, "--runtime-sha256", approved, rejected=True)
                invoke("runtime", "repair-launcher", "--install-dir", root, rejected=True)
                invoke("runtime", "login", "--install-dir", root, "install", rejected=True)
                detached = invoke("runtime", "uninstall", "--install-dir", root)
                if not detached.get("detached") or not detached.get("managed_payload_preserved") or not control.is_dir():
                    raise RuntimeError("Managed detach lost provider files or durable state")
                if lifecycle == "external":
                    if supervisor.poll() is not None or not detached.get("external_supervisor_continues"):
                        raise RuntimeError("User detach stopped the organization-owned supervisor")
                    invoke("service", "shutdown", "--runtime-dir", control)
                    if supervisor.wait(timeout=30):
                        raise RuntimeError("Owned external-supervisor fixture did not drain cleanly")
                    supervisor = None
            after = {str(path.relative_to(provider)): digest(path) for path in provider.rglob("*") if path.is_file()}
            if after != before:
                raise RuntimeError("Managed lifecycle changed provider payload bytes or topology")
            print("Read-only managed payload, exact health, lifecycle policy, external ownership and payload/state preservation passed.")
        finally:
            for control in controls:
                if control.exists():
                    subprocess.run([str(binary), "service", "shutdown", "--runtime-dir", str(control)],
                                   stdout=subprocess.DEVNULL, stderr=subprocess.DEVNULL, timeout=30,
                                   env=environment, creationflags=flags)
            if supervisor is not None:
                try:
                    supervisor.wait(timeout=30)
                except subprocess.TimeoutExpired:
                    supervisor.kill()
                    supervisor.wait(timeout=30)
            for path in [provider, *directories]:
                path.chmod(stat.S_IRUSR | stat.S_IWUSR | stat.S_IXUSR)
            for path in files:
                path.chmod(stat.S_IRUSR | stat.S_IWUSR | stat.S_IXUSR)


if __name__ == "__main__":
    main()
