# SPDX-License-Identifier: Apache-2.0
"""Independent synthetic native byte projection controls, never model qualification."""
import hashlib
import json
from pathlib import Path
import tempfile
import unittest
import stage_release_parts as stage


def sha(raw):
    return hashlib.sha256(raw).hexdigest()


class ReleasePartTests(unittest.TestCase):
    def inputs(self, root):
        bundles, files, parts, pins, expected = [], {}, {}, {}, {}
        for number in range(1, 4):
            identity = f"{number:064x}"
            data = b"ABCDEF" + str(number).encode()
            source = root / f"logical-{number}.bin"
            source.write_bytes(data)
            bundles.append({"profile": identity, "qualification": "c" * 64,
                            "directory": identity, "files": [{"name": "logical.bin", "bytes": len(data), "sha256": sha(data)}]})
            files[identity] = {"logical.bin": str(source)}
            for part, (offset, raw) in enumerate(((0, data[:3]), (3, data[3:])), 1):
                name = f"model-{number}.part{part:02}"
                parts[name] = {"profile": identity, "asset_name": "logical.bin", "offset": offset,
                               "bytes": len(raw), "sha256": sha(raw)}
                pins[name] = {"bytes": len(raw), "sha256": sha(raw)}
                expected[name] = raw
        plan = root / "plan.json"
        plan.write_text(json.dumps({"schema": "airk.arbiter.offline-bundle-plan/1", "tag": "v0.0.0-synthetic",
                                   "catalog_sha256": "a" * 64, "promotion_decision_sha256": "b" * 64, "bundles": bundles}), encoding="utf-8")
        publication = root / "publication.json"
        publication.write_text(json.dumps({"schema": "airk.arbiter.publication/1", "tag": "v0.0.0-synthetic",
                                          "compiled_catalog_sha256": "a" * 64, "profiles": list(files), "assets": pins}), encoding="utf-8")
        sources = root / "sources.private.json"
        sources.write_text(json.dumps({"schema": "airk.private-offline-bundle-sources/1", "plan_sha256": sha(plan.read_bytes()),
                                      "publication_sha256": sha(publication.read_bytes()), "files": files, "parts": parts}), encoding="utf-8")
        return plan, sources, publication, expected

    def run_stage(self, root, plan, sources, publication):
        return stage.stage(plan, sha(plan.read_bytes()), sources, sha(sources.read_bytes()), publication,
                           sha(publication.read_bytes()), root / "output", root)

    def test_exact_parts_and_public_metadata_only_no_source_changes(self):
        with tempfile.TemporaryDirectory() as directory:
            root = Path(directory)
            plan, sources, publication, expected = self.inputs(root)
            before = {path.name: path.read_bytes() for path in root.glob("logical-*.bin")}
            receipt = self.run_stage(root, plan, sources, publication)
            self.assertTrue(receipt["complete"])
            self.assertEqual(receipt["source_owner_sha256"], stage.owner.digest(stage.owner.__file__))
            self.assertEqual({path.name: path.read_bytes() for path in (root / "output/parts").iterdir()}, expected)
            self.assertEqual({path.name for path in (root / "output").iterdir()}, {"parts", "publication.json", "staging-receipt.json"})
            self.assertNotIn(str(root), (root / "output/staging-receipt.json").read_text())
            self.assertEqual(before, {path.name: path.read_bytes() for path in root.glob("logical-*.bin")})
            self.assertTrue(all(path.stat().st_nlink == 1 for path in (root / "output/parts").iterdir()))

    def test_wrong_publication_closure_refuses_before_output(self):
        with tempfile.TemporaryDirectory() as directory:
            root = Path(directory)
            plan, sources, publication, _ = self.inputs(root)
            value = json.loads(sources.read_bytes())
            value["publication_sha256"] = "0" * 64
            sources.write_text(json.dumps(value))
            with self.assertRaises(ValueError):
                self.run_stage(root, plan, sources, publication)
            self.assertFalse((root / "output").exists())

    def test_gaps_foreign_slices_and_boolean_sizes_refuse_before_output(self):
        for fault in ("gap", "foreign", "boolean", "over_one_gib"):
            with self.subTest(fault=fault), tempfile.TemporaryDirectory() as directory:
                root = Path(directory)
                plan, sources, publication, _ = self.inputs(root)
                value = json.loads(sources.read_bytes())
                part = next(iter(value["parts"].values()))
                if fault == "gap":
                    part["offset"] = 1
                elif fault == "foreign":
                    part["asset_name"] = "credentials.bin"
                elif fault == "boolean":
                    part["bytes"] = True
                else:
                    part["bytes"] = (1 << 30) + 1
                sources.write_text(json.dumps(value))
                with self.assertRaises(ValueError):
                    self.run_stage(root, plan, sources, publication)
                self.assertFalse((root / "output").exists())

    def test_source_hash_drift_refuses_before_output_preserving_current_source(self):
        with tempfile.TemporaryDirectory() as directory:
            root = Path(directory)
            plan, sources, publication, _ = self.inputs(root)
            source = root / "logical-1.bin"
            source.write_bytes(b"CHANGED")
            with self.assertRaises(ValueError):
                self.run_stage(root, plan, sources, publication)
            self.assertEqual(source.read_bytes(), b"CHANGED")
            self.assertFalse((root / "output").exists())

    def test_wrong_slice_hash_never_produces_completion_receipt(self):
        with tempfile.TemporaryDirectory() as directory:
            root = Path(directory)
            plan, sources, publication, _ = self.inputs(root)
            value, public = json.loads(sources.read_bytes()), json.loads(publication.read_bytes())
            name = next(iter(value["parts"]))
            value["parts"][name]["sha256"] = "0" * 64
            public["assets"][name]["sha256"] = "0" * 64
            publication.write_text(json.dumps(public))
            value["publication_sha256"] = sha(publication.read_bytes())
            sources.write_text(json.dumps(value))
            with self.assertRaises(ValueError):
                self.run_stage(root, plan, sources, publication)
            self.assertFalse((root / "output/staging-receipt.json").exists())
            self.assertEqual((root / "logical-1.bin").read_bytes(), b"ABCDEF1")

    def test_source_hardlink_and_existing_destination_are_never_overwritten(self):
        with tempfile.TemporaryDirectory() as directory:
            root = Path(directory)
            plan, sources, publication, _ = self.inputs(root)
            (root / "alias.bin").hardlink_to(root / "logical-1.bin")
            with self.assertRaises(ValueError):
                self.run_stage(root, plan, sources, publication)
            self.assertFalse((root / "output").exists())
            target = root / "foreign.bin"
            target.write_bytes(b"foreign")
            with self.assertRaises((FileExistsError, ValueError)):
                stage.copy_slice(root / "logical-2.bin", target, 0, 3, sha(b"ABC"), root)
            self.assertEqual(target.read_bytes(), b"foreign")

    def test_uncovered_distinct_logical_asset_refuses_before_output(self):
        with tempfile.TemporaryDirectory() as directory:
            root = Path(directory)
            plan, sources, publication, _ = self.inputs(root)
            private, public = json.loads(sources.read_bytes()), json.loads(publication.read_bytes())
            for name in ("model-3.part01", "model-3.part02"):
                private["parts"].pop(name)
                public["assets"].pop(name)
            publication.write_text(json.dumps(public), encoding="utf-8")
            private["publication_sha256"] = sha(publication.read_bytes())
            sources.write_text(json.dumps(private), encoding="utf-8")
            with self.assertRaisesRegex(ValueError, "omits"):
                self.run_stage(root, plan, sources, publication)
            self.assertFalse((root / "output").exists())

    def test_identical_native_deduplicated_logical_asset_accepted(self):
        with tempfile.TemporaryDirectory() as directory:
            root = Path(directory)
            plan, sources, publication, _ = self.inputs(root)
            projected = json.loads(plan.read_bytes())
            third = projected["bundles"][2]["files"][0]
            third.update(projected["bundles"][0]["files"][0])
            (root / "logical-3.bin").write_bytes(b"ABCDEF1")
            plan.write_text(json.dumps(projected), encoding="utf-8")
            private, public = json.loads(sources.read_bytes()), json.loads(publication.read_bytes())
            for name in ("model-3.part01", "model-3.part02"):
                private["parts"].pop(name)
                public["assets"].pop(name)
            publication.write_text(json.dumps(public), encoding="utf-8")
            private["plan_sha256"] = sha(plan.read_bytes())
            private["publication_sha256"] = sha(publication.read_bytes())
            sources.write_text(json.dumps(private), encoding="utf-8")
            receipt = self.run_stage(root, plan, sources, publication)
            self.assertEqual(len(receipt["parts"]), 4)


if __name__ == "__main__":
    unittest.main()
