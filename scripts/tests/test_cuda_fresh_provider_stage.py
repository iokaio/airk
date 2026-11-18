# SPDX-License-Identifier: Apache-2.0
"""Pure custody, copying and receipt controls for fresh CUDA provider staging."""

from collections import Counter
import io
import json
from pathlib import Path
from types import SimpleNamespace
import unittest
from unittest.mock import Mock, call, patch

from scripts.tests import test_cuda_fresh_projection as fixtures

SCRIPTS = Path(__file__).resolve().parents[1]
REPO = SCRIPTS.parent
stage = fixtures.load(SCRIPTS / "cuda-provider-staging.py", "fresh_provider_stage_controls")
env = fixtures.load(SCRIPTS / "cuda-runtime-environment.py", "fresh_provider_stage_env_controls")
runtime = fixtures.load(SCRIPTS / "cuda-static-runtime.py", "fresh_provider_stage_runtime_controls")


class FreshProviderStage(unittest.TestCase):
    def setUp(self):
        self.root = REPO / "inert-fresh-stage"
        self.output = self.root / "gpu-provider-stage"
        self.original = self.root / "fresh-compile.json"
        self.aggregate = self.root / "fresh-aggregate.json"
        self.recipe_path = self.root / "sealed-configure-recipe.json"
        self.shared = self.root / "b/Release/onnxruntime_providers_shared.dll"
        self.cuda = self.root / "b/Release/onnxruntime_providers_cuda.dll"
        self.receipt = self.output / "staging-receipt.json"
        self.guard_source = SCRIPTS / "cuda-component-execution.py"
        self.recipe = {
            "schema": "airk.private.cuda-fresh-configure-recipe/1",
            "repository_root": str(REPO), "fresh_output": str(self.root),
            "execution_authorized": True, "qualification": False,
            "checked_guard": fixtures.pin(SCRIPTS / "dev/assert-human-execution.ps1"),
            "powershell": fixtures.pin(self.root / "toolchain/pwsh.exe"),
            "owners": {"guards": fixtures.pin(self.guard_source)},
        }
        self.inputs = {
            self.original: b'{"actual":"compile"}\n',
            self.aggregate: b'{"actual":"aggregate"}\n',
            self.recipe_path: json.dumps(self.recipe).encode("utf-8"),
            self.shared: b"MZinert shared bytes",
            self.cuda: b"MZinert cuda bytes",
        }
        self.aggregate_pin = fixtures.pin(self.aggregate, self.inputs[self.aggregate])
        self.build = {
            "schema": "airk.private.cuda-fresh-static-build-proof/1", "qualification": False,
            "configure_recipe": fixtures.pin(self.recipe_path, self.inputs[self.recipe_path]),
            "build_receipt": fixtures.pin(self.original, self.inputs[self.original]),
            "provider_dlls": [fixtures.pin(path, self.inputs[path]) for path in (self.shared, self.cuda)],
        }
        self.handoff = {
            "schema": "airk.cuda-static-environment/6", "activation_pins_available": False,
            "request": {"destination": str(self.root), "aggregate_receipt": self.aggregate_pin},
            "build": self.build,
            "aggregate": {"qualification": False, "receipt_sha256": self.aggregate_pin["sha256"]},
        }
        self.deadline = 900
        self.clock = 100
        self.events = []
        self.created = set()
        self.writes = {}
        self.read_counts = Counter()
        self.changed_read = None
        self.late_read = None
        self.core = SimpleNamespace(plain=lambda path, **_: Path(path))
        self.guard = Mock(side_effect=self.audit_output)
        self.guard_many = Mock(side_effect=self.audit_sources)
        self.owner = SimpleNamespace(SID="verified-human", guard=self.guard, guard_many=self.guard_many)
        self.owner_loaded = Mock(return_value=self.owner)
        self.verifier = Mock(return_value=self.handoff)

    def audit_output(self, recipe, path, created, *, deadline):
        self.events.append(("output-audit", path, created))

    def audit_sources(self, recipe, paths, created, *, deadline):
        self.events.append(("source-audit", tuple(paths), created))

    def mkdir(self, path):
        self.assertEqual(path, self.output)
        self.assertNotIn(path, self.created)
        self.events.append(("mkdir", path))
        self.created.add(path)

    def open_path(self, path, mode):
        self.events.append(("open", path, mode))
        case = self
        if mode == "rb":
            self.read_counts[path] += 1
            number = self.read_counts[path]
            data = bytes(self.writes[path]) if path in self.writes else self.inputs[path]
            if self.changed_read and self.changed_read[:2] == (path, number):
                data = self.changed_read[2]

            class Reader(io.BytesIO):
                def read(self, size):
                    case.events.append(("read", path, size))
                    if case.late_read == (path, number):
                        case.clock = case.deadline
                    return super().read(size)

            return Reader(data)
        self.assertEqual(mode, "xb")
        self.assertIn(path.parent, self.created)
        if path in self.writes:
            raise FileExistsError(path)
        self.writes[path] = bytearray()

        class Writer(io.BytesIO):
            def write(self, data):
                case.events.append(("write", path, len(data)))
                case.writes[path].extend(data)
                return super().write(data)

        return Writer()

    def run_stage(self, output=None):
        output = self.output if output is None else output
        modules = {"cuda-static-runtime": runtime, "cuda-runtime-environment": env}
        with patch.object(stage, "load", side_effect=modules.__getitem__), \
                patch.object(runtime, "load", return_value=self.core), \
                patch.object(env, "fresh_handoff_value", self.verifier), \
                patch.object(env, "_fresh_owner", self.owner_loaded), \
                patch.object(stage.time, "monotonic", side_effect=lambda: self.clock), \
                patch.object(stage, "FRESH_STAGE_IO_CHUNK", 3), \
                patch.object(Path, "mkdir", autospec=True, side_effect=self.mkdir), \
                patch.object(Path, "exists", autospec=True, side_effect=self.created.__contains__), \
                patch.object(Path, "open", autospec=True, side_effect=self.open_path):
            return stage.stage_fresh_providers(self.root, self.aggregate_pin, output, REPO, deadline=self.deadline)

    def test_stages_exact_pair_and_original_receipt_with_one_replay_and_bounded_copy(self):
        result = self.run_stage()
        self.verifier.assert_called_once_with(self.root, self.aggregate_pin, REPO, deadline=900)
        self.owner_loaded.assert_called_once_with(
            self.guard_source, self.recipe["owners"]["guards"], "fresh_provider_stage_guard",
            self.core, runtime, 900,
        )
        paths = [self.aggregate, self.original, self.shared, self.cuda]
        self.assertEqual(self.guard_many.call_args_list, [call(self.recipe, paths, True, deadline=900)] * 2)
        expected_writes = {self.output / source.name: self.inputs[source] for source in paths[1:]}
        self.assertEqual({path: bytes(raw) for path, raw in self.writes.items() if path != self.receipt}, expected_writes)
        output_calls = [call(self.recipe, path, created, deadline=900)
                        for path in [self.output, *expected_writes, self.receipt] for created in (False, True)]
        self.assertEqual(self.guard.call_args_list, output_calls)
        self.assertEqual(json.loads(self.writes[self.receipt]), result)
        self.assertEqual(result["build_receipt"], self.build["build_receipt"])
        self.assertEqual(result["aggregate_receipt"], self.aggregate_pin)
        self.assertEqual(result["producer_proof_sha256"], fixtures.identity(self.build))
        self.assertEqual(result["aggregate_proof_sha256"], fixtures.identity(self.handoff["aggregate"]))
        self.assertEqual(result["files"], [fixtures.pin(path.name, raw) for path, raw in expected_writes.items()])
        self.assertEqual(result["status"], "PASS_PRIVATE_PROVIDER_COPY_ONLY_INCOMPLETE_UNAPPROVED")
        self.assertEqual(result["schema"], "airk.private.gpu-provider-stage/4")
        for flag in ("qualification", "activation_pins_available", "complete_cuda_runtime"):
            self.assertIs(result[flag], False)
        self.assertTrue(all(event[2] <= 3 for event in self.events
                            if event[0] == "read" and event[1] != self.recipe_path))
        for path in expected_writes:
            self.assertLess(self.events.index(("output-audit", path, False)), self.events.index(("open", path, "xb")))
            self.assertLess(self.events.index(("output-audit", path, True)), self.events.index(("open", path, "rb")))

    def test_refuses_existing_or_nonexact_child_before_verification(self):
        with self.assertRaisesRegex(ValueError, "exact fresh provider stage"):
            self.run_stage(self.root / "other")
        self.created.add(self.output)
        with self.assertRaisesRegex(ValueError, "existing or aliased"):
            self.run_stage()
        self.verifier.assert_not_called()
        self.assertFalse(self.writes)

    def test_refuses_wrong_provider_root_and_wrong_original_bytes_before_creation(self):
        self.build["provider_dlls"][1] = fixtures.pin(self.output / self.cuda.name, self.inputs[self.cuda])
        with self.assertRaisesRegex(ValueError, "exact fresh provider stage FilePin"):
            self.run_stage()
        self.build["provider_dlls"][1] = fixtures.pin(self.cuda, self.inputs[self.cuda])
        self.inputs[self.original] = b"wrong"
        with self.assertRaisesRegex(ValueError, "truncated fresh provider stage"):
            self.run_stage()
        self.assertFalse(self.created)
        self.assertFalse(self.writes)

    def test_keeps_failure_receipt_and_no_pass_if_local_source_changes_during_copy(self):
        self.changed_read = (self.shared, 2, self.inputs[self.shared] + b"extra")
        with self.assertRaisesRegex(ValueError, "fresh provider stage file bytes differ"):
            self.run_stage()
        result = json.loads(self.writes[self.receipt])
        self.assertEqual(result["status"], "FAILED_PRESERVED_NO_RETRY")
        self.assertEqual(result["files"], [fixtures.pin(self.original.name, self.inputs[self.original])])
        self.assertIn(("output-audit", self.output / self.shared.name, True), self.events)
        self.assertNotIn(self.output / self.cuda.name, self.writes)
        self.verifier.assert_called_once()

    def test_refuses_post_copy_source_drift_even_if_all_staged_bytes_match(self):
        self.changed_read = (self.aggregate, 1, self.inputs[self.aggregate] + b"extra")
        with self.assertRaisesRegex(ValueError, "fresh provider stage file bytes differ"):
            self.run_stage()
        result = json.loads(self.writes[self.receipt])
        self.assertEqual(result["status"], "FAILED_PRESERVED_NO_RETRY")
        self.assertEqual(len(result["files"]), 3)
        self.verifier.assert_called_once()

    def test_preserves_primary_audit_failure_when_retention_also_fails(self):
        def fail(recipe, path, created, *, deadline):
            if path == self.output / self.shared.name:
                raise ValueError("provider parent refused")
            if path == self.receipt:
                raise OSError("receipt retention refused")
            self.audit_output(recipe, path, created, deadline=deadline)

        self.guard.side_effect = fail
        with self.assertRaisesRegex(ValueError, "provider parent refused") as raised:
            self.run_stage()
        self.assertIn("Secondary staging receipt retention failure: receipt retention refused", raised.exception.__notes__)
        self.assertNotIn(self.output / self.shared.name, self.writes)
        self.assertNotIn(self.receipt, self.writes)

    def test_expired_or_late_copy_never_refreshes_original_deadline(self):
        self.clock = self.deadline
        with self.assertRaisesRegex(ValueError, "unexpired fresh provider staging deadline"):
            self.run_stage()
        self.verifier.assert_not_called()
        self.clock = 100
        self.late_read = (self.original, 2)
        with self.assertRaisesRegex(TimeoutError, "original fresh handoff verification deadline") as raised:
            self.run_stage()
        self.assertEqual(self.writes, {self.output / self.original.name: b""})
        self.assertTrue(any("Secondary staging receipt retention failure" in note for note in raised.exception.__notes__))
        self.verifier.assert_called_once()


if __name__ == "__main__":
    unittest.main()
