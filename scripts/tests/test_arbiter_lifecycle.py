# SPDX-License-Identifier: Apache-2.0
"""The acquisition harness never mistakes acceptance for completed native bytes."""
import hashlib
import importlib.util
import json
from pathlib import Path
import tempfile
import time
import unittest
from unittest.mock import Mock, patch

SPEC = importlib.util.spec_from_file_location("arbiter_lifecycle", Path(__file__).parents[1] / "check-arbiter-lifecycle.py")
LIFECYCLE = importlib.util.module_from_spec(SPEC)
SPEC.loader.exec_module(LIFECYCLE)


class AcquisitionTests(unittest.TestCase):
    def profile(self):
        return {"stage": "focus", "model": "arbiter-focus-qwen3-reranker-06b", "manifest": "a" * 64,
                "assets": [{"name": "model.onnx", "bytes": 5, "sha256": hashlib.sha256(b"hello").hexdigest()}]}

    def status(self, availability):
        profile = self.profile()
        return {"focus": {"profiles": [{"model": profile["model"], "profile": profile["manifest"],
                                       "availability": availability, "reason": "Observed native state"}]}}

    def test_accepted_requested_state_waits_for_native_published_files(self):
        read = Mock(side_effect=[self.status("requested"), self.status("downloading"), self.status("requested")])
        observer, record = Mock(), Mock()
        present = Mock(side_effect=[False, False, True])
        result = LIFECYCLE.observe(read, self.profile(), observer, time.monotonic() + 100, record, present)
        self.assertEqual(result, self.status("requested"))
        self.assertEqual(observer.wait.call_count, 2)
        self.assertEqual(record.call_count, 3)

    def test_last_file_event_cannot_lose_the_final_in_memory_transition(self):
        read = Mock(side_effect=[self.status("downloading"), self.status("requested")])
        observer = Mock()
        result = LIFECYCLE.observe(read, self.profile(), observer, time.monotonic() + 100, Mock(), Mock(return_value=True))
        self.assertEqual(result, self.status("requested"))
        observer.wait.assert_not_called()

    def test_native_failure_stops_without_another_submission_or_wait(self):
        read, observer = Mock(return_value=self.status("failed")), Mock()
        with self.assertRaisesRegex(RuntimeError, "acquisition refused"):
            LIFECYCLE.observe(read, self.profile(), observer, time.monotonic() + 100, Mock(), Mock())
        observer.wait.assert_not_called()
        self.assertEqual(read.call_count, 1)

    def test_foreign_running_profile_is_not_a_pinned_acquisition(self):
        status = self.status("requested")
        status["focus"]["profiles"][0]["profile"] = "b" * 64
        with self.assertRaisesRegex(ValueError, "pinned native profile"):
            LIFECYCLE.profile_status(status, self.profile())

    def test_only_final_logical_asset_names_are_censused_and_hashes_must_match(self):
        with tempfile.TemporaryDirectory() as temporary:
            owner = Path(temporary)
            profile = self.profile()
            root = owner / "arbiter-v3" / "assets" / profile["manifest"]
            stage = root / "stage-1"
            stage.mkdir(parents=True)
            (stage / "model.onnx").write_bytes(b"hello")
            self.assertFalse(LIFECYCLE.files_present(owner, profile))
            (root / "model.onnx").write_bytes(b"hello")
            self.assertTrue(LIFECYCLE.files_present(owner, profile))
            census = LIFECYCLE.verify_assets(owner, profile)
            self.assertEqual(census[0]["bytes"], 5)
            (root / "model.onnx").write_bytes(b"wrong")
            with self.assertRaisesRegex(ValueError, "digest differs"):
                LIFECYCLE.verify_assets(owner, profile)

    def test_fixture_construction_failure_still_retains_a_receipt(self):
        with tempfile.TemporaryDirectory() as temporary:
            output = Path(temporary)
            (output / "airk-runtime.json").write_text("{}", encoding="utf-8")
            (output / "airk-install.json").write_text("{}", encoding="utf-8")
            clients = Mock()
            clients.Fixture.side_effect = ValueError("Rejected fixture input")
            with patch.object(LIFECYCLE, "freeze_helpers", return_value=(output, {})), \
                 patch.object(LIFECYCLE, "module", return_value=clients), \
                 self.assertRaisesRegex(ValueError, "Rejected fixture"):
                LIFECYCLE.qualify(output, [], output, output, 1)
            receipt = json.loads((output / "receipt.json").read_bytes())
            self.assertFalse(receipt["acquisition_passed"])
            self.assertFalse(receipt["cleanup"]["fixture_constructed"])
            self.assertFalse(receipt["cleanup"]["complete"])
            self.assertIsNone(receipt["cleanup"]["native_stop"])

    def test_helper_imports_use_exact_frozen_bytes_after_live_source_changes(self):
        with tempfile.TemporaryDirectory() as temporary:
            root = Path(temporary)
            source, output = root / "source", root / "output"
            source.mkdir()
            output.mkdir()
            (source / "sample.py").write_text("VALUE = 1", encoding="utf-8")
            frozen, evidence = LIFECYCLE.freeze_helpers(output, source, names=("sample.py",))
            (source / "sample.py").write_text("VALUE = 2", encoding="utf-8")
            self.assertEqual(LIFECYCLE.module("frozen_sample", frozen / "sample.py").VALUE, 1)
            self.assertEqual(evidence["files"]["sample.py"], LIFECYCLE.sha256(frozen / "sample.py"))

    def test_relocated_split_pair_closure_constructs_and_stages_without_repo_inputs(self):
        with tempfile.TemporaryDirectory() as temporary:
            root = Path(temporary)
            output, runtime, owner = root / "output", root / "runtime", root / "owner"
            for path in [output, runtime, owner]:
                path.mkdir()
            frozen, _ = LIFECYCLE.freeze_helpers(output)
            clients = LIFECYCLE.module("frozen_actual_clients", frozen / "check-service-clients.py")
            metadata = {"schema": 3, "version": "1.8.206", "os": "windows", "arch": "x86_64",
                "airk_sha256": hashlib.sha256(b"service").hexdigest(),
                "airkctl_sha256": hashlib.sha256(b"client").hexdigest(),
                "i1_protocol": 1, "i2_contract": 1, "admin_inventory_sha256": "c" * 64}
            (runtime / "airk.exe").write_bytes(b"service")
            (runtime / "airkctl.exe").write_bytes(b"client")
            (runtime / "airk-runtime.json").write_text(json.dumps(metadata), encoding="utf-8")
            (runtime / "notice.txt").write_bytes(b"notice")
            inventory = {path.name: LIFECYCLE.sha256(path) for path in runtime.iterdir()}
            (runtime / "airk-install.json").write_text(json.dumps({"schema": 1, "version": "1.8.206",
                "os": "windows", "arch": "x86_64", "files": inventory}), encoding="utf-8")
            fixture = clients.Fixture(runtime, owner)
            try:
                self.assertTrue(fixture.managed_installation)
                staged = clients.split_fixture_payload(runtime, owner / "payload")
                self.assertEqual((staged / "airk.exe").read_bytes(), b"service")
                self.assertEqual(clients.classify_runtime(staged)["topology"], "single_image")
            finally:
                fixture.close()
            self.assertFalse((output / "frozen-source" / "Cargo.toml").exists())
            self.assertFalse((output / "frozen-source" / "fixtures").exists())

    def test_catalog_requires_exact_native_bytes_and_three_independent_models(self):
        with tempfile.TemporaryDirectory() as temporary:
            path = Path(temporary) / "catalog.json"
            rows = [{"model": model} for model in LIFECYCLE.MODELS]
            path.write_text(json.dumps({"profiles": rows}), encoding="utf-8")
            self.assertEqual(len(LIFECYCLE.pinned_catalog(path, LIFECYCLE.sha256(path))), 3)
            with self.assertRaisesRegex(ValueError, "SHA-256 differs"):
                LIFECYCLE.pinned_catalog(path, "0" * 64)
            path.write_text(json.dumps({"profiles": rows + [rows[0]]}), encoding="utf-8")
            with self.assertRaisesRegex(ValueError, "exactly the three"):
                LIFECYCLE.pinned_catalog(path, LIFECYCLE.sha256(path))


if __name__ == "__main__":
    unittest.main()
