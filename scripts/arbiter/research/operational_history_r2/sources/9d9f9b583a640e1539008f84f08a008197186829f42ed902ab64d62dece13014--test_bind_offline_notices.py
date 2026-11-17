# SPDX-License-Identifier: Apache-2.0
"""Notice document binding controls, with synthetic file-only plan fixtures."""
import json
from pathlib import Path
import tempfile
import unittest

import bind_offline_notices as binding
import test_package_offline_bundles as fixtures


class NoticeBindingTests(unittest.TestCase):
    def inputs(self, root):
        plan, _, _, notices, _ = fixtures.OfflineArchiveTests().inputs(root)
        projected, docs = json.loads(plan.read_bytes()), json.loads(notices.read_bytes())
        files = {}
        for number, bundle in enumerate(projected["bundles"], 1):
            bundle["model"] = f"synthetic-model-{number}"
            files[bundle["model"]] = docs["files"][bundle["profile"]]
        plan.write_text(json.dumps(projected), encoding="utf-8")
        selection = root / "selection.private.json"
        selection.write_text(json.dumps({"schema": "airk.private-offline-notice-selections/1", "files": files}), encoding="utf-8")
        return plan, selection

    def run_bind(self, root, plan, selection):
        return binding.bind(plan, binding.package.owner.digest(plan), selection,
                            binding.package.owner.digest(selection), root / "mapping.private.json", root)

    def test_exact_final_identity_binding_with_full_docs_no_qualification_claim(self):
        with tempfile.TemporaryDirectory() as directory:
            root = Path(directory)
            plan, selection = self.inputs(root)
            receipt = self.run_bind(root, plan, selection)
            mapping = json.loads((root / "mapping.private.json").read_bytes())
            self.assertEqual(set(mapping["files"]), {f"{number:064x}" for number in range(1, 4)})
            self.assertEqual(mapping["plan_sha256"], binding.package.owner.digest(plan))
            self.assertEqual(receipt["profiles"], 3)
            self.assertFalse(receipt["approved"])
            self.assertFalse(receipt["qualified"])
            with self.assertRaisesRegex(ValueError, "fresh"):
                self.run_bind(root, plan, selection)

    def test_wrong_model_census_or_document_drift_refuses_before_mapping(self):
        for fault in ("model", "drift", "qualification", "identity"):
            with self.subTest(fault=fault), tempfile.TemporaryDirectory() as directory:
                root = Path(directory)
                plan, selection = self.inputs(root)
                if fault == "model":
                    value = json.loads(selection.read_bytes())
                    value["files"]["foreign-model"] = value["files"].pop("synthetic-model-1")
                    selection.write_text(json.dumps(value), encoding="utf-8")
                elif fault == "drift":
                    (root / "notice-1-LICENSE").write_bytes(b"not the pinned license")
                else:
                    value = json.loads(plan.read_bytes())
                    if fault == "qualification":
                        value["bundles"][0].pop("qualification")
                    else:
                        value["bundles"][1]["profile"] = value["bundles"][0]["profile"]
                    plan.write_text(json.dumps(value), encoding="utf-8")
                with self.assertRaises(ValueError):
                    self.run_bind(root, plan, selection)
                self.assertFalse((root / "mapping.private.json").exists())


if __name__ == "__main__":
    unittest.main(verbosity=2)
