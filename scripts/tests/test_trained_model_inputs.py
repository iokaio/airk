# SPDX-License-Identifier: Apache-2.0
"""Independent current-original acquisition and retained-v1 identity regressions."""
import copy
import hashlib
import importlib.util
import json
from pathlib import Path
import tempfile
import unittest
from unittest.mock import patch

ROOT = Path(__file__).resolve().parents[2]
SPEC = importlib.util.spec_from_file_location("trained_model_provision", ROOT / "scripts/provision-local-model.py")
MODULE = importlib.util.module_from_spec(SPEC)
SPEC.loader.exec_module(MODULE)


class TrainedModelInputs(unittest.TestCase):
    def test_trained_catalog_has_new_identities_and_no_false_upstream_asset_urls(self):
        profiles = MODULE.CATALOG["profiles"]
        self.assertEqual([p["id"] for p in profiles], ["minilm-l6-v2-u8-v3", "minilm-l6-v2-s8-v3"])
        self.assertEqual([p["manifest"]["model_sha256"] for p in profiles], [
            "b941bf19f1f1283680f449fa6a7336bb5600bdcd5f84d10ddc5cd72218a0fd21",
            "4278337fd0ff3c68bfb6291042cad8ab363e1d9fbc43dcb499fe91c871902474"])
        for profile in profiles:
            self.assertTrue(all(asset["url"] is None for asset in profile["assets"]))
            self.assertIsNone(profile.get("quantization"))
            self.assertEqual(profile["qualification"]["verified_platforms"], [])
            self.assertEqual(profile["manifest"]["license_sha256"], "cfc7749b96f63bd31c3c42b5c471bf756814053e847c10f3eb003417bc523d30")

    def test_retained_v1_profiles_preserve_their_original_runtime_and_hashes(self):
        original = json.loads((ROOT / "crates/airk-retrieval/src/model_catalog_v1.json").read_bytes())
        self.assertEqual([p["id"] for p in original["profiles"]], ["minilm-l6-v2-u8-v1", "minilm-l6-v2-s8-v1"])
        self.assertEqual([p["manifest"]["model_sha256"] for p in original["profiles"]], [
            "b941bf19f1f1283680f449fa6a7336bb5600bdcd5f84d10ddc5cd72218a0fd21",
            "4278337fd0ff3c68bfb6291042cad8ab363e1d9fbc43dcb499fe91c871902474"])
        self.assertEqual({p["runtime"] for p in original["profiles"]}, {"onnxruntime-cpu/1.22.1/airk-int8-b1-1"})

    def test_explicit_local_inputs_are_verified_from_copied_bytes_without_download(self):
        with tempfile.TemporaryDirectory() as temporary:
            source = Path(temporary) / "source"
            source.mkdir()
            destination = Path(temporary) / "destination"
            profile = copy.deepcopy(MODULE.PROFILE)
            for asset, field in zip(profile["assets"], ["model_sha256", "tokenizer_sha256", "license_sha256"]):
                content = ("original trained fixture " + asset["name"]).encode()
                (source / asset["name"]).write_bytes(content)
                asset.update(url=None, bytes=len(content), sha256=hashlib.sha256(content).hexdigest())
                profile["manifest"][field] = asset["sha256"]
            with patch.object(MODULE.urllib.request, "urlopen") as network:
                MODULE.provision(destination, profile=profile, source_directory=source)
                network.assert_not_called()
                for asset in profile["assets"]:
                    self.assertEqual((destination / asset["name"]).read_bytes(), (source / asset["name"]).read_bytes())
                corrupt = Path(temporary) / "corrupt-destination"
                (source / "model.onnx").write_bytes(b"foreign model")
                with self.assertRaises(ValueError):
                    MODULE.provision(corrupt, profile=profile, source_directory=source)
                self.assertFalse((corrupt / "model.json").exists())
                network.assert_not_called()


if __name__ == "__main__":
    unittest.main()
