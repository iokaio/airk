# SPDX-License-Identifier: Apache-2.0
"""Explicit native Mac login acceptance in a retained, uniquely owned fixture.

Never modifies an existing registration. Keep the fixture on any failure so its
app/receipt cannot disappear while Service Management ownership is uncertain.
"""
import argparse
import hashlib
import json
from pathlib import Path
import platform
import subprocess
import time
import uuid

ROOT = Path(__file__).resolve().parents[1]


def digest(path):
    with path.open("rb") as stream:
        return hashlib.file_digest(stream, "sha256").hexdigest()


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("--runtime", type=Path, required=True)
    parser.add_argument("--approved-sha256", required=True,
                        help="Independently selected approval for the exact tested runtime inventory")
    args = parser.parse_args()
    if platform.system() != "Darwin":
        parser.error("Native Mac SDK/login acceptance requires a Mac GUI execution host")
    payload = args.runtime.resolve(strict=True)
    if digest(payload / "airk-install.json") != args.approved_sha256:
        parser.error("Runtime differs from the explicitly supplied approval")
    binary = payload / "AIRK.app/Contents/MacOS/airk"
    if not binary.is_file():
        parser.error("Expected the native 1.4 Mac app payload")
    base = ROOT / "target"
    base.mkdir(exist_ok=True)
    base = base.resolve(strict=True)
    fixture = base / ("macos-login-" + uuid.uuid4().hex)
    fixture.mkdir(mode=0o700)
    install = fixture / "install"
    source = fixture / "source"
    data = fixture / "data"
    source.mkdir()
    original = source / "original.txt"
    original.write_bytes(b"Original Mac login fixture source; preserve through uninstall.\n")
    source_hash = digest(original)
    started = time.monotonic()
    receipts = []

    def invoke(*arguments):
        result = subprocess.run([str(binary), *map(str, arguments)], stdin=subprocess.DEVNULL,
                                capture_output=True, text=True, encoding="utf-8", timeout=180)
        if result.returncode:
            raise RuntimeError(f"Native {arguments[0:2]} failed: {result.stderr[-4096:]}")
        value = json.loads(result.stdout)
        receipts.append({"command": list(map(str, arguments[:3])), "value": value})
        return value

    def retain(state, error=None):
        record = {"schema": 1, "state": state, "os": platform.platform(), "arch": platform.machine(),
                  "runtime": args.approved_sha256, "elapsed_seconds": time.monotonic() - started,
                  "source_preserved": original.is_file() and digest(original) == source_hash,
                  "receipts": receipts, "error": error}
        (fixture / "qualification.json").write_text(json.dumps(record, indent=2) + "\n", encoding="utf-8")

    try:
        invoke("runtime", "stage", "--source", payload, "--install-dir", install,
               "--approved-sha256", args.approved_sha256, "--verification-basis", "trusted-distribution", "--channel", "preview")
        invoke("runtime", "activate", "--install-dir", install, "--runtime-sha256", args.approved_sha256)
        launcher = subprocess.run([str(install / "airk-launcher"), "--version"], stdin=subprocess.DEVNULL,
                                  capture_output=True, text=True, encoding="utf-8", timeout=180)
        if launcher.returncode or not launcher.stdout.strip().startswith("airk 1.4."):
            raise RuntimeError("The copied lightweight launcher did not execute the selected app")
        # Registration is metadata-only; no source pass/model session is requested.
        invoke("--service-install-dir", install, "--root", source, "--data-dir", data,
               "--collection", "mac-login-fixture", "init")
        before = invoke("runtime", "login", "--install-dir", install, "status")
        if before.get("installed") or before.get("loaded") is not False:
            raise RuntimeError("Existing or unverifiable Mac registration is preserved")
        installed = invoke("runtime", "login", "--install-dir", install, "install")
        if installed.get("registered_definition_matches") is not True:
            raise RuntimeError("Actual app/launchd registration did not verify")
        stopped = invoke("runtime", "login", "--install-dir", install, "stop")
        if stopped.get("running") is not False or stopped.get("loaded") is not False:
            raise RuntimeError("Native ownership was not released after Stop")
        restarted = invoke("runtime", "login", "--install-dir", install, "start")
        if restarted.get("registered_definition_matches") is not True:
            raise RuntimeError("Restart did not preserve captured ownership")
        invoke("runtime", "login", "--install-dir", install, "uninstall")
        invoke("runtime", "uninstall", "--install-dir", install)
        if digest(original) != source_hash:
            raise RuntimeError("Original fixture source changed")
        retain("passed")
    except BaseException as error:
        # Only the fixture's own installed receipt can authorize removal. An
        # unverifiable registration/app is retained for explicit native diagnosis.
        cleanup_confirmed = not (install / "mac-login-1.4.json").is_file()
        if not cleanup_confirmed:
            try:
                invoke("runtime", "login", "--install-dir", install, "uninstall")
                cleanup_confirmed = True
            except Exception:
                pass
        try:
            if cleanup_confirmed:
                invoke("runtime", "uninstall", "--install-dir", install)
            else:
                invoke("service", "shutdown", "--runtime-dir", install / "control")
        except Exception:
            pass
        retain("failed_retained", str(error))
        raise
    print(f"Mac app/login acceptance passed; retained source/settings/receipts: {fixture}")


if __name__ == "__main__":
    main()
