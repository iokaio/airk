# SPDX-License-Identifier: Apache-2.0
"""Inert controls for the verified fresh CUDA static environment handoff."""

from copy import deepcopy
import hashlib
import io
import json
from pathlib import Path
from types import SimpleNamespace
import unittest
from unittest.mock import Mock, patch

from scripts.tests import test_cuda_fresh_projection as fixtures

SCRIPTS = Path(__file__).resolve().parents[1]
REPO = SCRIPTS.parent
BUILD = SCRIPTS / "cuda-static-runtime/build"
runtime = fixtures.load(SCRIPTS / "cuda-static-runtime.py", "fresh_handoff_runtime_controls")
handoff = fixtures.load(SCRIPTS / "cuda-runtime-environment.py", "fresh_handoff_controls")


class FreshHandoff(unittest.TestCase):
    def setUp(self):
        fixture = fixtures.FreshProjection()
        fixture.setUp()
        self.root = fixture.root
        self.facts = fixture.facts
        self.pin = fixture.aggregate_pin
        self.facts["aggregate"]["output"]["bytes"] = 100000
        self.facts["archive"]["bytes"] = 100000
        sources = {"pin_reader": SCRIPTS / "cuda-static-incremental.py", "projection": BUILD / "fresh_projection.py"}
        self.reads = {path: path.read_bytes() for path in sources.values()}
        self.facts["aggregate_recipe"]["owners"] = {
            name: fixtures.pin(path, self.reads[path]) for name, path in sources.items()
        }
        self.handoff_path = self.root / "environment.json"
        self.verify_receipt = Mock(return_value=self.facts)
        self.clock = 100
        self.deadline = 900
        self.core = SimpleNamespace(plain=lambda path, **_: Path(path), MAX_JSON=16 << 20)
        self.verifier = SimpleNamespace(verify_receipt=self.verify_receipt)
        self.loaded = Mock(side_effect=self.load_owner)

    def load_owner(self, path, name):
        if path == BUILD / "fresh_verify.py":
            return self.verifier
        if path == SCRIPTS / "provision-static-onnx-runtime.py":
            return self.core
        raise AssertionError(f"unexpected owner {name} at {path}")

    def open_read(self, path, mode):
        self.assertEqual(mode, "rb")
        return io.BytesIO(self.reads[path])

    def observe(self, operation):
        env_loader = {"cuda-static-runtime": runtime}
        with patch.object(handoff, "_load", side_effect=env_loader.__getitem__), \
                patch.object(runtime, "load", self.loaded), \
                patch.object(handoff.time, "monotonic", side_effect=lambda: self.clock), \
                patch.object(Path, "open", autospec=True, side_effect=self.open_read):
            return operation()

    def construct(self):
        return self.observe(lambda: handoff.fresh_handoff_value(self.root, self.pin, REPO, deadline=self.deadline))

    def reverify(self, value, *, line_drift=False, duplicate=False):
        original = json.dumps(value, ensure_ascii=False).encode("utf-8")
        if duplicate:
            original = original.replace(b'"request": {', b'"request": {"destination": "duplicate", ', 1)
        self.core.digest = lambda _: hashlib.sha256(original).hexdigest()
        self.core.read_json = lambda _: deepcopy(value)
        self.reads[self.handoff_path] = original + b"\n" if line_drift else original
        return self.observe(lambda: handoff.verify_handoff(
            self.handoff_path, self.core.digest(self.handoff_path), REPO, deadline=self.deadline,
        ))

    def test_constructs_exact_request_and_environment_after_one_audit(self):
        value = self.construct()
        self.assertEqual(set(value), {"schema", "request", "build", "aggregate", "environment", "activation_pins_available"})
        self.assertEqual(value["schema"], "airk.cuda-static-environment/6")
        self.assertEqual(value["request"], {"destination": str(self.root), "aggregate_receipt": self.pin})
        self.assertEqual(value["environment"], {
            "LIBONNXRUNTIME_NO_PKG_CONFIG": "1", "ORT_SKIP_DOWNLOAD": "1", "ORT_PREFER_DYNAMIC_LINK": "0",
            "ORT_LIB_LOCATION": str(self.root / "gpu-aggregate"), "ORT_LIB_PROFILE": "Release",
        })
        self.assertIs(value["activation_pins_available"], False)
        self.verify_receipt.assert_called_once_with(self.pin, 900)

    def test_reopens_same_receipt_and_preserves_exact_c7_projection(self):
        value = self.construct()
        self.verify_receipt.reset_mock()
        result = self.reverify(value)
        self.verify_receipt.assert_called_once_with(self.pin, 900)
        self.assertEqual(result, {
            "environment": value["environment"],
            "static_ort": {
                "build_receipt_sha256": value["build"]["build_receipt"]["sha256"],
                "target": "x86_64-pc-windows-msvc", "static_link_closure_sha256": fixtures.identity(value["aggregate"]),
                "static_link_supplement_sha256": None, "static_link_recovery_sha256": None,
                "verified_inputs_sha256": fixtures.identity({"gpu_build": value["build"], "gpu_aggregate": value["aggregate"]}),
            },
            "gpu_build": value["build"], "gpu_aggregate": value["aggregate"], "activation_pins_available": False,
        })

    def test_refuses_mixed_request_relative_destination_and_mismatched_filepin_before_audit(self):
        value = self.construct()
        self.verify_receipt.reset_mock()
        variants = (
            {"producer_file": "old-producer.json"},
            {"destination": "relative-build"},
            {"aggregate_receipt": {**self.pin, "path": str(self.root / "other/fresh-aggregate.json")}},
            {"aggregate_receipt": {**self.pin, "bytes": True}},
        )
        for fields in variants:
            bad = deepcopy(value)
            bad["request"].update(fields)
            with self.subTest(fields=fields), self.assertRaises(runtime.GpuProofUnavailable):
                self.reverify(bad)
        self.verify_receipt.assert_not_called()

    def test_refuses_stale_proof_or_environment_even_when_handoff_sha_matches(self):
        value = self.construct()
        for section in ("build", "aggregate", "environment"):
            bad = deepcopy(value)
            bad[section]["different"] = True
            with self.subTest(section=section), self.assertRaisesRegex(runtime.GpuProofUnavailable, "no longer matches"):
                self.reverify(bad)

    def test_rejects_missing_and_late_deadlines_without_refresh(self):
        value = self.construct()
        self.verify_receipt.reset_mock()
        self.deadline = None
        with self.assertRaisesRegex(ValueError, "explicit finite"):
            self.reverify(value)
        self.verify_receipt.assert_not_called()
        self.deadline = 900

        def late(*_):
            self.clock = self.deadline
            return self.facts

        self.verify_receipt.side_effect = late
        with self.assertRaisesRegex(TimeoutError, "original fresh handoff"):
            self.reverify(value)
        self.verify_receipt.assert_called_once_with(self.pin, 900)

    def test_refuses_changed_consumed_handoff_and_projection_source(self):
        value = self.construct()
        self.verify_receipt.reset_mock()
        with self.assertRaisesRegex(runtime.GpuProofUnavailable, "consumed fresh handoff bytes"):
            self.reverify(value, line_drift=True)
        self.verify_receipt.assert_not_called()
        self.reads[BUILD / "fresh_projection.py"] = b"raise AssertionError('unauthenticated code executed')"
        with self.assertRaisesRegex(runtime.GpuProofUnavailable, "consumed fresh handoff owner bytes"):
            self.reverify(value)

    def test_rejects_oversized_handoff_before_streaming_legacy_digest(self):
        self.core.digest = Mock()
        with patch.object(Path, "is_file", return_value=True), \
                patch.object(Path, "stat", return_value=SimpleNamespace(st_size=self.core.MAX_JSON + 1)), \
                self.assertRaisesRegex(runtime.GpuProofUnavailable, "oversized GPU environment handoff"):
            self.observe(lambda: handoff.verify_handoff(self.handoff_path, "e" * 64, REPO, deadline=self.deadline))
        self.core.digest.assert_not_called()
        self.verify_receipt.assert_not_called()

    def test_refuses_duplicate_json_fields_with_an_exact_reviewed_sha(self):
        value = self.construct()
        self.verify_receipt.reset_mock()
        with self.assertRaisesRegex(runtime.GpuProofUnavailable, "duplicate fresh handoff field"):
            self.reverify(value, duplicate=True)
        self.verify_receipt.assert_not_called()

    def test_refuses_cross_build_archive_from_the_verifier(self):
        value = self.construct()
        self.facts["archive"]["path"] = str(self.root / "other/gpu-aggregate/onnxruntime.lib")
        with self.assertRaisesRegex(runtime.GpuProofUnavailable, "verified fresh receipt or destination differs"):
            self.reverify(value)


if __name__ == "__main__":
    unittest.main()
