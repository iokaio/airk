# SPDX-License-Identifier: Apache-2.0
"""Unsupported historical workload protocols fail before native activation."""
import hashlib
import importlib.util
import json
import os
from pathlib import Path
import tempfile
import unittest
from unittest.mock import patch


SCRIPTS = Path(__file__).resolve().parents[1]


def load(name, filename):
    spec = importlib.util.spec_from_file_location(name, SCRIPTS / filename)
    module = importlib.util.module_from_spec(spec)
    spec.loader.exec_module(module)
    return module


faults = load("admission_faults", "check-publication-faults.py")
quality = load("admission_quality", "check-retrieval-quality.py")


class WorkloadAdmissionTests(unittest.TestCase):
    def runtime(self, root):
        runtime = root / "runtime"
        runtime.mkdir()
        binary = runtime / ("airk.exe" if os.name == "nt" else "airk")
        binary.write_bytes(b"fictional native image; never executed")
        metadata = {"schema": 1, "version": "1.8.12", "os": "windows", "arch": "x86_64",
                    "airk_sha256": "a" * 64, "embed_sha256": "b" * 64}
        build = {"runtime": metadata, "source": {"sha256": "c" * 64}}
        digest = lambda value: hashlib.sha256(value).hexdigest()
        manifest = {"files": {}}
        for name, value in (("airk-runtime.json", metadata), ("airk-build.json", build)):
            raw = json.dumps(value).encode()
            (runtime / name).write_bytes(raw)
            manifest["files"][name] = digest(raw)
        raw = json.dumps(manifest).encode()
        (runtime / "airk-install.json").write_bytes(raw)
        # Inject fictional reviewed evidence, not a classifier/decision mock.
        contracts = {(digest(raw), build["source"]["sha256"]): {"version": "1.8.12", "authority": "catalog",
            "input_owner": "native_administrative", "migration_path": "reviewed_catalog",
            "historical_contract_review_revision": "d" * 40}}
        return binary, contracts

    def test_catalog_fault_workload_refuses_before_native_start_or_process_discovery(self):
        with tempfile.TemporaryDirectory() as temporary:
            root = Path(temporary)
            binary, contracts = self.runtime(root)
            base = root / "fixture"
            base.mkdir()
            with patch.dict(faults.clients.REVIEWED_RUNTIME_CONTRACTS, contracts), \
                 patch.object(faults.subprocess, "run") as run, \
                 patch.object(faults.subprocess, "Popen") as popen, \
                 patch.object(faults.processes, "_backend") as backend:
                with self.assertRaisesRegex(ValueError, "retained-child startup protocol"):
                    faults.run_campaign(binary, base, 1, None)
                run.assert_not_called()
                popen.assert_not_called()
                backend.assert_not_called()
            self.assertFalse((base / "install").exists())
            self.assertFalse((base / "state").exists())

    def test_catalog_additional_root_workload_refuses_before_activation(self):
        with tempfile.TemporaryDirectory() as temporary:
            root = Path(temporary)
            binary, contracts = self.runtime(root)
            base = root / "fixture"
            with patch.dict(quality.clients.REVIEWED_RUNTIME_CONTRACTS, contracts), \
                 patch.object(quality.subprocess, "run") as run, \
                 patch.object(quality.subprocess, "Popen") as popen:
                with self.assertRaisesRegex(ValueError, "Historical catalog source-root review protocol"):
                    quality.fixture(binary, base, 1, 1, None)
                run.assert_not_called()
                popen.assert_not_called()
            self.assertFalse((base / "install").exists())
            self.assertFalse((base / "state").exists())


if __name__ == "__main__":
    unittest.main()
