# SPDX-License-Identifier: Apache-2.0
"""Opt-in actual native Provision census in a retained, isolated service fixture.

Consumes the Rust catalog; never constructs configuration, model, or protocol DTOs.
This acquisition gate deliberately reports native load/call qualification separately.
The caller must establish the logged-in execution identity before invoking it.
"""
import argparse
import ctypes
from ctypes import wintypes
from datetime import datetime, timezone
import hashlib
import importlib.util
import json
import os
from pathlib import Path
import subprocess
import sys
import time

HELPERS = ("check-service-clients.py", "check-runtime-upgrade.py", "build-service-runtime.py",
           "split_receipts.py", "standalone-installer-contract.py")

MODELS = {"arbiter-focus-qwen3-reranker-06b", "arbiter-context-qwen3-17b", "arbiter-context-qwen3-4b"}


def module(name, path):
    spec = importlib.util.spec_from_file_location(name, path)
    result = importlib.util.module_from_spec(spec)
    spec.loader.exec_module(result)
    return result


def sha256(path):
    with Path(path).open("rb") as stream:
        return hashlib.file_digest(stream, "sha256").hexdigest()


def pinned_catalog(path, expected):
    data = Path(path).read_bytes()
    if hashlib.sha256(data).hexdigest() != expected:
        raise ValueError("Native catalog SHA-256 differs from the reviewed input")
    value = json.loads(data)
    rows = value["profiles"]
    if len(rows) != 3 or {row["model"] for row in rows} != MODELS:
        raise ValueError("Expected exactly the three selected native profiles")
    return rows


def profile_status(status, profile):
    rows = status[profile["stage"]]["profiles"]
    matches = [row for row in rows if row["model"] == profile["model"]]
    if len(matches) != 1 or matches[0]["profile"] != profile["manifest"]:
        raise ValueError("Running product does not expose the pinned native profile")
    return matches[0]


def acquisition_complete(status, profile):
    row = profile_status(status, profile)
    if row["availability"] in {"failed", "draining", "unqualified", "unsupported_system", "constrained"}:
        raise RuntimeError("Native acquisition refused: " + row["reason"])
    # While the accepted background operation is busy, native status reports
    # Downloading/Verifying. Requested here is the completed cached-only profile.
    return row["availability"] in {"requested", "ready"}


class Changes:
    """Wait for actual recursive directory changes, never fixed polling sleeps."""
    def __init__(self, directory):
        if os.name != "nt":
            raise RuntimeError("This event observer currently qualifies Windows only")
        self.api = ctypes.WinDLL("kernel32", use_last_error=True)
        self.api.FindFirstChangeNotificationW.argtypes = [wintypes.LPCWSTR, wintypes.BOOL, wintypes.DWORD]
        self.api.FindFirstChangeNotificationW.restype = wintypes.HANDLE
        self.api.FindNextChangeNotification.argtypes = [wintypes.HANDLE]
        self.api.FindNextChangeNotification.restype = wintypes.BOOL
        self.api.FindCloseChangeNotification.argtypes = [wintypes.HANDLE]
        self.api.FindCloseChangeNotification.restype = wintypes.BOOL
        self.api.WaitForSingleObject.argtypes = [wintypes.HANDLE, wintypes.DWORD]
        self.api.WaitForSingleObject.restype = wintypes.DWORD
        self.handle = self.api.FindFirstChangeNotificationW(str(directory), True, 0x1 | 0x2 | 0x8 | 0x10)
        if self.handle == ctypes.c_void_p(-1).value:
            raise ctypes.WinError(ctypes.get_last_error())

    def wait(self, deadline):
        remaining = deadline - time.monotonic()
        if remaining <= 0:
            raise TimeoutError("Original acquisition observation deadline expired")
        result = self.api.WaitForSingleObject(self.handle, max(1, min(0xFFFFFFFE, int(remaining * 1000))))
        if result == 258:
            raise TimeoutError("No native acquisition change before original deadline")
        if result != 0 or not self.api.FindNextChangeNotification(self.handle):
            raise ctypes.WinError(ctypes.get_last_error())

    def close(self):
        if self.handle is not None:
            handle, self.handle = self.handle, None
            if not self.api.FindCloseChangeNotification(handle):
                raise ctypes.WinError(ctypes.get_last_error())


def observe(read, profile, observer, deadline, record, files_present):
    while True:
        if time.monotonic() >= deadline:
            raise TimeoutError("Original acquisition observation deadline expired")
        status = read()
        record(status)
        complete = acquisition_complete(status, profile)
        present = files_present()
        if complete and present:
            return status
        if not present:
            observer.wait(deadline)
        # Final file publication precedes the in-memory lifecycle transition.
        # Once every logical file exists, continue actual bounded Status reads;
        # waiting for another filesystem event could lose that last transition.


def asset_root(owner, profile):
    roots = [path for path in owner.rglob(profile["manifest"]) if path.is_dir() and path.parent.name == "assets"]
    if len(roots) > 1:
        raise ValueError("Duplicate acquired native asset directories")
    return roots[0] if roots else None


def files_present(owner, profile):
    root = asset_root(owner, profile)
    return root is not None and all(
        (root / asset["name"]).is_file() and (root / asset["name"]).stat().st_size == asset["bytes"]
        for asset in profile["assets"])


def verify_assets(owner, profile):
    # This is a read-only independent census of the NEW fixture, not Store
    # authority. Native Provision has already performed transaction verification.
    root = asset_root(owner, profile)
    if root is None:
        raise ValueError("Missing acquired native asset directory")
    result = []
    for asset in profile["assets"]:
        path = root / asset["name"]
        if path.is_symlink() or not path.is_file() or path.stat().st_size != asset["bytes"]:
            raise ValueError("Acquired logical asset type or length differs: " + asset["name"])
        actual = sha256(path)
        if actual != asset["sha256"]:
            raise ValueError("Acquired logical asset digest differs: " + asset["name"])
        result.append({"file": str(path), "bytes": asset["bytes"], "sha256": actual})
    return result


def freeze_helpers(output, source=None, names=HELPERS):
    # Freeze the inspected split-pair startup/cleanup closure in repo-relative
    # layout. These paths execute no builder, recipe, or evidence-fixture reads.
    # All dynamic sibling imports resolve inside this new immutable snapshot.
    source = source or Path(__file__).parent
    destination = output / "frozen-source" / "scripts"
    destination.mkdir(parents=True)
    files = {}
    for name in names:
        path = source / name
        data = path.read_bytes()
        compile(data, str(path), "exec")
        with (destination / path.name).open("xb") as stream:
            stream.write(data)
        files[path.name] = hashlib.sha256(data).hexdigest()
    manifest = output / "helper-inputs.json"
    with manifest.open("x", encoding="utf-8") as stream:
        json.dump({"files": files}, stream, indent=2)
        stream.write("\n")
    return destination, {"manifest_sha256": sha256(manifest), "files": files}


def qualify(runtime, catalog, output, owner, timeout):
    started = time.monotonic()
    report = {"schema": "airk.arbiter-acquisition-run/1", "started_utc": datetime.now(timezone.utc).isoformat(),
              "command": [sys.executable, *sys.argv], "harness_sha256": sha256(Path(__file__)),
              "explicit_provision_consent": True, "fixture": str(owner), "models": [],
              "native_catalog_profiles": catalog,
              "native_load_and_call_exercised": False, "release_qualified": False,
              "limitation": "Provision/byte census only; native load, ABI call, failure/recovery and quality gates remain separate."}
    fixture = None
    journal = output / "native-status.jsonl"
    def record(value):
        with journal.open("a", encoding="utf-8") as stream:
            stream.write(json.dumps({"elapsed_seconds": time.monotonic() - started, "status": value}) + "\n")
    def call(*args):
        return json.loads(fixture.run("--service-runtime-dir", fixture.control, "arbiter", *args).stdout)
    def read():
        return call("status")
    try:
        helpers, report["helper_inputs"] = freeze_helpers(output)
        clients = module("arbiter_actual_clients", helpers / "check-service-clients.py")
        report["runtime"] = json.loads((runtime / "airk-runtime.json").read_bytes())
        report["runtime_manifest_sha256"] = sha256(runtime / "airk-install.json")
        report["source_commit"] = subprocess.run(["git", "rev-parse", "HEAD"], cwd=Path(__file__).resolve().parents[1], capture_output=True, text=True, check=True).stdout.strip()
        fixture = clients.Fixture(runtime, owner, command_timeout=150)
        with fixture:
            initial = read()
            record(initial)
            if initial.get("initialization") is not None:
                record(call("initialize"))
            for profile in catalog:
                status = read()
                row = profile_status(status, profile)
                if row["availability"] != "missing_assets":
                    raise ValueError("Fresh fixture unexpectedly contains optional assets")
                revision = status["configuration"]["revision"]
                observer = Changes(owner)
                try:
                    accepted = call("provision", "--expected-revision", revision, "--profile", profile["manifest"])
                    record(accepted)
                    final = observe(read, profile, observer, time.monotonic() + timeout, record,
                                    lambda: files_present(owner, profile))
                finally:
                    observer.close()
                report["models"].append({"model": profile["model"], "manifest": profile["manifest"],
                    "accepted": accepted, "settled_status": final, "logical_assets": verify_assets(owner, profile),
                    "acquisition_passed": True, "native_call_passed": None})
            report["acquisition_passed"] = True
    except BaseException as error:
        report["acquisition_passed"] = False
        report["failure"] = {"type": type(error).__name__, "detail": str(error)[:4096]}
        raise
    finally:
        stop = getattr(fixture, "managed_stop", None)
        report["cleanup"] = {"fixture_constructed": fixture is not None,
            "service_start_attempted": getattr(fixture, "managed_started", False),
            "native_stop": stop, "failure": getattr(fixture, "cleanup_failure", None),
            "complete": isinstance(stop, dict) and stop.get("record", {}).get("phase") == "complete"}
        report["ended_utc"] = datetime.now(timezone.utc).isoformat()
        report["elapsed_seconds"] = time.monotonic() - started
        with (output / "receipt.json").open("x", encoding="utf-8") as stream:
            json.dump(report, stream, indent=2)
            stream.write("\n")
    return report


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("--runtime-dir", type=Path, required=True)
    parser.add_argument("--catalog", type=Path, required=True)
    parser.add_argument("--catalog-sha256", required=True)
    parser.add_argument("--output", type=Path, required=True)
    parser.add_argument("--fixture-parent", type=Path, required=True)
    parser.add_argument("--timeout-per-model", type=int, default=7200)
    parser.add_argument("--consent-download-all-three", action="store_true", required=True)
    args = parser.parse_args()
    if os.name != "nt":
        parser.error("Actual acquisition event observation currently supports Windows only")
    if not 1 <= args.timeout_per_model <= 14400:
        parser.error("Per-model original deadline must be between 1 and 14400 seconds")
    catalog = pinned_catalog(args.catalog.resolve(strict=True), args.catalog_sha256)
    runtime = args.runtime_dir.resolve(strict=True)
    parent = args.fixture_parent.resolve(strict=True)
    if not parent.is_dir() or args.fixture_parent.absolute() != parent:
        parser.error("Fixture parent must be an existing unaliased approved directory")
    output = args.output.absolute()
    output.mkdir(exist_ok=False)
    # A unique retained child is intentional: never delete uncertain outcomes or
    # pay for a second 24 GiB transfer merely because an outer tool was dropped.
    import tempfile
    owner = Path(tempfile.mkdtemp(prefix="arbiter-provision-", dir=parent))
    report = qualify(runtime, catalog, output, owner, args.timeout_per_model)
    print(json.dumps({"receipt": str(output / "receipt.json"), "acquisition_passed": report["acquisition_passed"],
                      "native_load_and_call_exercised": False}))


if __name__ == "__main__":
    main()
