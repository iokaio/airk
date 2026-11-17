# SPDX-License-Identifier: Apache-2.0
"""Synthetic file-projection tests only; no Profiles, model runs or qualification."""
import hashlib
import json
from pathlib import Path
import tempfile
import unittest
import stage_offline_bundles as stage


class OfflineProjectionTests(unittest.TestCase):
    def inputs(self, root):
        bundles, files = [], {}
        for number in range(1, 4):
            identity = f"{number:064x}"
            source = root / f"source-{number}.bin"
            source.write_bytes(b"licensed synthetic payload " + str(number).encode())
            asset = {"name": "payload.bin", "bytes": source.stat().st_size, "sha256": stage.digest(source)}
            bundles.append({"profile": identity, "qualification": "c" * 64, "directory": identity, "files": [asset]})
            files[identity] = {"payload.bin": str(source)}
        plan = root / "native-plan.json"
        plan.write_text(json.dumps({"schema": "airk.arbiter.offline-bundle-plan/1", "tag": "v0.0.0-synthetic-test",
                                   "catalog_sha256": "a" * 64, "promotion_decision_sha256": "b" * 64,
                                   "bundles": bundles}))
        sources = root / "sources.private.json"
        sources.write_text(json.dumps({"schema": "airk.private-offline-bundle-sources/1", "plan_sha256": stage.digest(plan), "files": files}))
        return plan, sources

    def run_stage(self, root, plan, sources):
        return stage.stage(plan, stage.digest(plan), sources, stage.digest(sources), root / "candidate", root)

    def test_exact_clean_payload_and_native_projection_no_private_source_map(self):
        with tempfile.TemporaryDirectory() as directory:
            root = Path(directory)
            plan, sources = self.inputs(root)
            before = {path.name: path.read_bytes() for path in root.glob("source-*.bin")}
            receipt = self.run_stage(root, plan, sources)
            self.assertTrue(receipt["complete"])
            output = root / "candidate"
            self.assertEqual({path.name for path in output.iterdir()}, {"models", "offline-bundle-plan.json", "IMPORT.txt", "staging-receipt.json"})
            self.assertEqual((output / "offline-bundle-plan.json").read_bytes(), plan.read_bytes())
            for bundle in receipt["bundles"]:
                child = output / bundle["directory"]
                self.assertEqual({path.name for path in child.iterdir()}, {"payload.bin"})
                self.assertEqual(stage.digest(child / "payload.bin"), bundle["files"][0]["sha256"])
                self.assertEqual((child / "payload.bin").stat().st_nlink, 1)
            self.assertEqual(before, {path.name: path.read_bytes() for path in root.glob("source-*.bin")})
            self.assertNotIn(str(root), (output / "staging-receipt.json").read_text())
            with self.assertRaisesRegex(ValueError, "fresh"):
                self.run_stage(root, plan, sources)

    def test_wrong_native_pin_or_missing_decision_refuses_before_output(self):
        with tempfile.TemporaryDirectory() as directory:
            root = Path(directory)
            plan, sources = self.inputs(root)
            with self.assertRaises(ValueError):
                stage.stage(plan, "0" * 64, sources, stage.digest(sources), root / "candidate", root)
            value = json.loads(plan.read_text())
            value.pop("promotion_decision_sha256")
            plan.write_text(json.dumps(value))
            with self.assertRaises(ValueError):
                self.run_stage(root, plan, sources)
            self.assertFalse((root / "candidate").exists())

    def test_missing_final_qualification_pin_refuses_before_copy(self):
        with tempfile.TemporaryDirectory() as directory:
            root = Path(directory)
            plan, sources = self.inputs(root)
            value = json.loads(plan.read_text())
            value["bundles"][0].pop("qualification")
            plan.write_text(json.dumps(value))
            value = json.loads(sources.read_text())
            value["plan_sha256"] = stage.digest(plan)
            sources.write_text(json.dumps(value))
            with self.assertRaises(ValueError):
                self.run_stage(root, plan, sources)
            self.assertFalse((root / "candidate").exists())

    def test_extra_private_file_or_path_escape_refuses_without_copy(self):
        with tempfile.TemporaryDirectory() as directory:
            root = Path(directory)
            plan, sources = self.inputs(root)
            value = json.loads(sources.read_text())
            value["files"][f"{1:064x}"]["profile.json"] = str(plan)
            sources.write_text(json.dumps(value))
            with self.assertRaisesRegex(ValueError, "closure"):
                self.run_stage(root, plan, sources)
            self.assertFalse((root / "candidate").exists())
            with self.assertRaisesRegex(ValueError, "anchor"):
                stage.no_alias(root / ".." / "foreign", root)

    def test_mutated_payload_preserves_source_and_never_writes_completion(self):
        with tempfile.TemporaryDirectory() as directory:
            root = Path(directory)
            plan, sources = self.inputs(root)
            source = root / "source-1.bin"
            original = source.read_bytes()
            source.write_bytes(b"x" * len(original))
            with self.assertRaisesRegex(ValueError, "digest"):
                self.run_stage(root, plan, sources)
            self.assertEqual(source.read_bytes(), b"x" * len(original))
            self.assertFalse((root / "candidate/staging-receipt.json").exists())

    def test_source_hardlink_refuses_before_output(self):
        with tempfile.TemporaryDirectory() as directory:
            root = Path(directory)
            plan, sources = self.inputs(root)
            (root / "alias.bin").hardlink_to(root / "source-1.bin")
            with self.assertRaisesRegex(ValueError, "hardlinked"):
                self.run_stage(root, plan, sources)
            self.assertFalse((root / "candidate").exists())

    def test_exclusive_copy_never_replaces_a_foreign_destination(self):
        with tempfile.TemporaryDirectory() as directory:
            root = Path(directory)
            source, target = root / "source.bin", root / "foreign.bin"
            source.write_bytes(b"expected")
            target.write_bytes(b"preserve unrelated user bytes")
            with self.assertRaisesRegex(ValueError, "fresh"):
                stage.copy_exact(source, target, source.stat().st_size, stage.digest(source), root)
            self.assertEqual(target.read_bytes(), b"preserve unrelated user bytes")


if __name__ == "__main__":
    unittest.main(verbosity=2)
