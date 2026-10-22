# SPDX-License-Identifier: Apache-2.0
import hashlib
import copy
import importlib.util
from pathlib import Path
import tempfile
import unittest
from unittest.mock import patch

SPEC = importlib.util.spec_from_file_location("airk_model_provision", Path(__file__).parents[1] / "provision-local-model.py")
MODULE = importlib.util.module_from_spec(SPEC)
SPEC.loader.exec_module(MODULE)


class ModelProvisioningTests(unittest.TestCase):
    def test_provisioning_uses_the_native_catalog_without_changing_the_pinned_manifest(self):
        profile = MODULE.PROFILE
        self.assertEqual(profile["id"], MODULE.CATALOG["default_profile"])
        self.assertEqual(profile["precision"], "fp32")
        self.assertEqual(MODULE.ASSETS["model.onnx"][1], 90_405_214)
        self.assertEqual(MODULE.ASSETS["model.onnx"][2], profile["manifest"]["model_sha256"])
        self.assertEqual(profile["manifest"]["dimensions"], 384)

    def test_offline_verification_rejects_changed_assets_before_manifest(self):
        with tempfile.TemporaryDirectory() as directory:
            root = Path(directory)
            assets = {}
            for name in ("model.onnx", "tokenizer.json", "LICENSE"):
                content = ("original fixture " + name).encode()
                (root / name).write_bytes(content)
                assets[name] = ("https://example.invalid/unused", len(content), hashlib.sha256(content).hexdigest())
            profile = copy.deepcopy(MODULE.PROFILE)
            for name, field in (("model.onnx", "model_sha256"), ("tokenizer.json", "tokenizer_sha256"), ("LICENSE", "license_sha256")):
                profile["manifest"][field] = assets[name][2]
            with patch.object(MODULE, "ASSETS", assets), patch.object(MODULE, "PROFILE", profile), patch.object(MODULE.urllib.request, "urlopen") as request:
                manifest = MODULE.provision(root)
                self.assertEqual(manifest["schema"], 2)
                self.assertEqual(MODULE.provision(root), manifest)
                request.assert_not_called()
                (root / "tokenizer.json").write_bytes(b"altered")
                with self.assertRaises(ValueError):
                    MODULE.provision(root)
                request.assert_not_called()

    def test_catalog_hash_disagreement_fails_before_creating_or_downloading(self):
        with tempfile.TemporaryDirectory() as directory, patch.object(MODULE.urllib.request, "urlopen") as request:
            profile = copy.deepcopy(MODULE.PROFILE)
            profile["manifest"]["model_sha256"] = "0" * 64
            destination = Path(directory) / "not-created"
            with patch.object(MODULE, "PROFILE", profile), self.assertRaises(ValueError):
                MODULE.provision(destination, download=True)
            self.assertFalse(destination.exists())
            request.assert_not_called()

    def test_missing_assets_do_not_trigger_implicit_download(self):
        with tempfile.TemporaryDirectory() as directory, patch.object(MODULE.urllib.request, "urlopen") as request:
            with self.assertRaises(ValueError):
                MODULE.provision(Path(directory))
            request.assert_not_called()
            self.assertFalse((Path(directory) / "model.json").exists())


if __name__ == "__main__":
    unittest.main()
