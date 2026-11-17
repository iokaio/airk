# SPDX-License-Identifier: Apache-2.0
"""Read-only source and path migration regressions; no models or fixture writes."""
import hashlib
import importlib.util
import json
import os
from pathlib import Path
import sys
import unittest
from unittest.mock import patch

BASE = Path(__file__).resolve().parent
ROOT = next(p for p in BASE.parents if (p / "Cargo.toml").is_file() and (p / "AGENTS.md").is_file())


def load(relative):
    path = BASE / relative
    spec = importlib.util.spec_from_file_location("migration_" + path.stem, path)
    module = importlib.util.module_from_spec(spec)
    with patch.object(sys, "path", [str(path.parent), *sys.path]):
        spec.loader.exec_module(module)
    return module


class MigrationTests(unittest.TestCase):
    def test_maintained_source_map_matches_all_46_original_files(self):
        rows = []
        for manifest in BASE.rglob("source-map.json"):
            rows.extend(json.loads(manifest.read_bytes())["files"])
        self.assertEqual(len(rows), 46)
        self.assertEqual(len({r["destination"] for r in rows}), 46)
        for row in rows:
            path = ROOT / row["destination"]
            self.assertTrue(path.resolve().is_relative_to(BASE))
            raw = path.read_bytes()
            self.assertEqual(hashlib.sha256(raw).hexdigest(), row["maintained_sha256"])
            compile(raw, str(path), "exec")
            self.assertFalse(row["historical_receipts_reused"])

    def test_preparer_data_root_is_explicit_and_independent_of_cwd(self):
        selected = ROOT / "artifacts" / "synthetic-not-created-models"
        old = Path.cwd()
        try:
            os.chdir(BASE)
            with patch.dict(os.environ, {"AIRK_RESEARCH_MODELS": str(selected)}):
                module = load("minilm/encoder_recovery_r1/prepare_train_r3.py")
            self.assertEqual(module.HERE, selected / "encoder-recovery-preparation-r1")
            self.assertFalse(selected.exists())
        finally:
            os.chdir(old)

    def test_finite_trainer_import_does_not_import_torch_and_finds_repo(self):
        before = "torch" in sys.modules
        module = load("minilm/encoder_recovery_r1/train_finite_r3.py")
        self.assertEqual(module.ROOT, ROOT)
        self.assertEqual("torch" in sys.modules, before)

    def test_private_roots_refuse_missing_tracked_and_external_locations(self):
        for relative in ("focus_lora_r8/make_plan.py", "focus_lora_r8/select_instruction.py", "minilm/encoder_recovery_r1/prepare_train_r3.py", "minilm/parent_bag_r1/prepare.py"):
            module = load(relative)
            for value in (None, "", BASE, ROOT.parent):
                with self.subTest(relative=relative, value=value), self.assertRaises(ValueError):
                    module.require_private_root(value)
            self.assertEqual(module.require_private_root(ROOT / "artifacts"), ROOT / "artifacts")

    def test_missing_or_tracked_models_root_refuses_before_private_reads(self):
        module = load("minilm/encoder_recovery_r1/prepare_train_r3.py")
        for value in (None, str(BASE)):
            env = {} if value is None else {"AIRK_RESEARCH_MODELS": value}
            with patch.dict(os.environ, env, clear=True), patch.object(Path, "read_bytes", side_effect=AssertionError("private read before refusal")):
                with self.assertRaises(ValueError):
                    module.main()

    def test_both_synthetic_workers_require_exact_maintained_loss_bytes(self):
        loss = BASE / "minilm/parent_bag_r1/bag_loss.py"
        raw = loss.read_bytes()
        pin = hashlib.sha256(raw).hexdigest()
        for name in ("check_equivalence.py", "resource_pilot.py"):
            module = load("minilm/encoder_recovery_r1/" + name)
            self.assertEqual(module.sealed_loss_source(pin), (loss, raw))
            for wrong in ("", "0" * 64):
                with self.assertRaisesRegex(ValueError, "maintained loss source pin"):
                    module.sealed_loss_source(wrong)
            with patch.object(Path, "read_bytes", return_value=b"changed but never executed"):
                with self.assertRaisesRegex(ValueError, "maintained loss source pin"):
                    module.sealed_loss_source(pin)

    def test_overlay_output_requires_explicit_private_root(self):
        module = load("focus_lora_r8/support/candidate_overlay.py")
        with patch.dict(os.environ, {}, clear=True):
            with self.assertRaisesRegex(ValueError, "AIRK_OVERLAY_OUTPUT_ROOT"):
                module.private_output_root()
        with patch.dict(os.environ, {"AIRK_OVERLAY_OUTPUT_ROOT": str(BASE)}):
            with self.assertRaisesRegex(ValueError, "private artifacts"):
                module.private_output_root()
        with patch.dict(os.environ, {"AIRK_OVERLAY_OUTPUT_ROOT": str(ROOT / "artifacts")}):
            self.assertEqual(module.private_output_root(), ROOT / "artifacts")


if __name__ == "__main__":
    unittest.main(verbosity=2)
