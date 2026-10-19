# SPDX-License-Identifier: Apache-2.0
"""Pinned declaration provenance must remain distinct from recovered license text."""
import hashlib
import importlib.util
from pathlib import Path
import tempfile
import unittest

SCRIPT = Path(__file__).resolve().parents[1] / "license-notices.py"
SPEC = importlib.util.spec_from_file_location("license_notices", SCRIPT)
MODULE = importlib.util.module_from_spec(SPEC)
SPEC.loader.exec_module(MODULE)


class DeclarationTests(unittest.TestCase):
    def test_original_manifest_is_authenticated_and_tampering_is_rejected(self):
        with tempfile.TemporaryDirectory() as directory:
            source = Path(directory)
            original = b'[package]\nname="fictional"\nlicense="MIT"\n'
            (source / "Cargo.toml.orig").write_bytes(original)
            (source / "Cargo.toml").write_text("normalized Cargo manifest", encoding="utf-8")
            record = {"name": "fictional", "revision": "a" * 40,
                      "declaration": {"sha256": hashlib.sha256(original).hexdigest()},
                      "standalone_text_review": {"result": "unresolved"}, "missing_text": True}
            result = MODULE.declaration_provenance(source, record)
            self.assertEqual(result["standalone_text_review"]["result"], "unresolved")
            self.assertNotIn("license_files", result)
            (source / "Cargo.toml.orig").write_bytes(original + b"changed")
            with self.assertRaisesRegex(ValueError, "digest mismatch"):
                MODULE.declaration_provenance(source, record)

    def test_old_archive_uses_its_unnormalized_manifest(self):
        with tempfile.TemporaryDirectory() as directory:
            source = Path(directory)
            original = b"original manifest"
            (source / "Cargo.toml").write_bytes(original)
            record = {"name": "fictional", "revision": "b" * 40,
                      "declaration": {"sha256": hashlib.sha256(original).hexdigest()}}
            self.assertEqual(MODULE.declaration_provenance(source, record)["revision"], "b" * 40)
            self.assertEqual(MODULE.declaration_provenance(source, None), {})


if __name__ == "__main__":
    unittest.main()
