# SPDX-License-Identifier: Apache-2.0
import copy
import hashlib
import importlib.util
import io
import json
from contextlib import redirect_stderr
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
        self.assertEqual(MODULE.CATALOG["default_profile"], "minilm-l6-v2-fp16-v1")
        self.assertEqual([item["id"] for item in MODULE.CATALOG["profiles"]], ["minilm-l6-v2-fp16-v1"])
        self.assertEqual((profile["id"], profile["precision"]), ("minilm-l6-v2-fp16-v1", "float16"))
        self.assertEqual(MODULE.ASSETS, {
            "model.onnx": (None, 45_469_884, "f3c3a4eed373650ba1e2f4d3d53437f1ecf92d4c50e5994e07e163751923ee4a"),
            "tokenizer.json": (None, 466_247, "be50c3628f2bf5bb5e3a7f17b1f74611b2561a3a27eeab05e5aa30f411572037"),
            "LICENSE": (None, 11_358, "cfc7749b96f63bd31c3c42b5c471bf756814053e847c10f3eb003417bc523d30"),
        })
        manifest = (json.dumps(profile["manifest"], indent=2) + "\n").encode()
        self.assertEqual(
            hashlib.sha256(manifest).hexdigest(),
            "f311662cd7cc0f48dbf595b26d648537bd18e67a46d60b3f9f4e2150b2e63779",
        )
        self.assertEqual(
            MODULE.HISTORICAL_INT8_POLICY_SHA256,
            "9da184de7f3ae4ef93d9b4fabd6987677ffec0261c6c472f3e49387cd2dcb31d",
        )

    def test_historical_capture_is_authenticated_and_requires_explicit_offline_inputs(self):
        profile = {"id": "minilm-l6-v2-u8-v3", "manifest": {"schema": 2}}
        capture_bytes = b'{"catalog":{"profiles":[{"id":"minilm-l6-v2-u8-v3","manifest":{"schema":2}}]}}'
        digest = hashlib.sha256(capture_bytes).hexdigest()
        with tempfile.TemporaryDirectory() as directory:
            root = Path(directory)
            policy = root / "historical.json"
            policy.write_bytes(capture_bytes)
            source = root / "old"
            destination = root / "inputs"
            command = ["provision-local-model", "--destination", str(destination),
                       "--historical-policy", str(policy)]
            selector = ["--profile", "minilm-l6-v2-u8-v3"]
            local = ["--source-directory", str(source)]
            with patch.object(MODULE, "HISTORICAL_INT8_POLICY_SHA256", digest), \
                 patch.object(MODULE, "provision", return_value=profile["manifest"]) as provision, \
                 patch("builtins.print"):
                self.assertEqual(MODULE.historical_int8_profiles(policy), [profile])
                for invalid in ([], selector, local, selector + local + ["--download"]):
                    with self.subTest(invalid=invalid), patch("sys.argv", command + invalid), \
                         redirect_stderr(io.StringIO()), self.assertRaises(SystemExit) as error:
                        MODULE.main()
                    self.assertEqual(error.exception.code, 2)
                provision.assert_not_called()
                with patch("sys.argv", command + selector + local):
                    MODULE.main()
                provision.assert_called_once_with(destination / profile["id"], False, profile, source)
                policy.write_bytes(capture_bytes.replace(b"u8", b"s8"))
                with self.assertRaisesRegex(ValueError, "differs from the retained pin"):
                    MODULE.historical_int8_profiles(policy)

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
