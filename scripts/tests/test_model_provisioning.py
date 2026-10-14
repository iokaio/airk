# SPDX-License-Identifier: Apache-2.0
import hashlib
import importlib.util
from pathlib import Path
import tempfile
import unittest
from unittest.mock import patch

SPEC = importlib.util.spec_from_file_location("airk_model_provision", Path(__file__).parents[1] / "provision-local-model.py")
MODULE = importlib.util.module_from_spec(SPEC)
SPEC.loader.exec_module(MODULE)


class ModelProvisioningTests(unittest.TestCase):
    def test_offline_verification_rejects_changed_assets_before_manifest(self):
        with tempfile.TemporaryDirectory() as directory:
            root = Path(directory)
            assets = {}
            for name in ("model.onnx", "tokenizer.json", "LICENSE"):
                content = ("original fixture " + name).encode()
                (root / name).write_bytes(content)
                assets[name] = ("https://example.invalid/unused", len(content), hashlib.sha256(content).hexdigest())
            with patch.object(MODULE, "ASSETS", assets), patch.object(MODULE.urllib.request, "urlopen") as request:
                manifest = MODULE.provision(root)
                self.assertEqual(manifest["schema"], 2)
                self.assertEqual(MODULE.provision(root), manifest)
                request.assert_not_called()
                (root / "tokenizer.json").write_bytes(b"altered")
                with self.assertRaises(ValueError):
                    MODULE.provision(root)
                request.assert_not_called()

    def test_missing_assets_do_not_trigger_implicit_download(self):
        with tempfile.TemporaryDirectory() as directory, patch.object(MODULE.urllib.request, "urlopen") as request:
            with self.assertRaises(ValueError):
                MODULE.provision(Path(directory))
            request.assert_not_called()
            self.assertFalse((Path(directory) / "model.json").exists())


if __name__ == "__main__":
    unittest.main()
