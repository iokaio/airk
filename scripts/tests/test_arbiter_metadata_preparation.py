# SPDX-License-Identifier: Apache-2.0
"""Opaque-input and refusal controls; model semantics remain native Rust."""
from pathlib import Path
import hashlib
import importlib.util
import json
import tempfile
import unittest

SCRIPT = Path(__file__).with_name("prepare_arbiter_metadata.py")
if not SCRIPT.exists():
    SCRIPT = Path(__file__).resolve().parents[1] / "dev" / "prepare-arbiter-metadata.py"
spec = importlib.util.spec_from_file_location("prepare_arbiter_metadata", SCRIPT)
helper = importlib.util.module_from_spec(spec)
spec.loader.exec_module(helper)


class MetadataInputTests(unittest.TestCase):
    def setUp(self):
        self.temp = tempfile.TemporaryDirectory()
        self.addCleanup(self.temp.cleanup)
        self.root = Path(self.temp.name)
        self.catalog = self.root / "native-catalog.json"
        self.catalog.write_bytes(b'{"future_native_field":{"opaque":[1,2,3]}}\n')
        self.receipt = self.root / "original-failed-provision.json"
        self.receipt.write_bytes(b'{"status":"failed","original_error":"preserved"}\n')
        self.census = self.root / "saved-census.json"
        self.census.write_bytes(b'{"independent_saved_asset_census":true}\n')
        self.build = self.root / "native-build.json"
        self.build.write_bytes(b'{"opaque_original_native_build_receipt":true}')
        self.sources = self.root / "source.rs"
        self.sources.write_text("// exact frozen native source\n", encoding="utf-8")
        self.assets = self.root / "owned-assets"
        self.assets.mkdir()
        self.request = self.root / "frozen-request.json"
        self.output = self.root / "fresh-native-output"

    def freeze(self, **changes):
        values = dict(repository=self.root, catalog=self.catalog,
            acquisition_receipt=self.receipt, asset_census=self.census,
            native_build_receipt=self.build, asset_root=self.assets, request=self.request, output=self.output,
            source_files=[self.sources], source_commit="a" * 40)
        values.update(changes)
        return helper.build_request(**values)

    def test_preserves_opaque_native_inputs_and_original_failure(self):
        before = {path: path.read_bytes() for path in
                  [self.catalog, self.receipt, self.census, self.build, self.sources]}
        result = self.freeze()
        request = json.loads(self.request.read_text(encoding="utf-8"))
        self.assertEqual(request["source_catalog"]["sha256"],
                         hashlib.sha256(before[self.catalog]).hexdigest())
        self.assertEqual(request["acquisition_receipt"]["sha256"],
                         hashlib.sha256(before[self.receipt]).hexdigest())
        self.assertNotIn("profiles", request)
        self.assertFalse(result["metadata_regenerated"])
        self.assertFalse(result["public_source_available"])
        self.assertFalse(self.output.exists())
        for path, content in before.items():
            self.assertEqual(path.read_bytes(), content)

    def test_existing_output_refuses_before_request_write(self):
        self.output.mkdir()
        with self.assertRaisesRegex(ValueError, "fresh"):
            self.freeze()
        self.assertFalse(self.request.exists())

    def test_existing_request_is_preserved(self):
        self.request.write_bytes(b"unrelated bytes")
        with self.assertRaisesRegex(ValueError, "fresh"):
            self.freeze()
        self.assertEqual(self.request.read_bytes(), b"unrelated bytes")

    def test_request_cannot_live_inside_not_yet_created_native_output(self):
        with self.assertRaisesRegex(ValueError, "outside"):
            self.freeze(request=self.output / "request.json")
        self.assertFalse(self.output.exists())

    def test_output_cannot_escape_authorized_workspace(self):
        with self.assertRaisesRegex(ValueError, "authorized repository"):
            self.freeze(output=self.root.parent / "escaped-output")
        self.assertFalse(self.request.exists())

    def test_duplicate_source_inputs_refuse_before_write(self):
        with self.assertRaisesRegex(ValueError, "Distinct"):
            self.freeze(source_files=[self.sources, self.sources])
        self.assertFalse(self.request.exists())


if __name__ == "__main__":
    unittest.main()
