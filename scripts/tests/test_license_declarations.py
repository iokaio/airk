# SPDX-License-Identifier: Apache-2.0
"""Pinned declaration provenance must remain distinct from recovered license text."""
import hashlib
import importlib.util
from pathlib import Path
import tempfile
import shutil
import tomllib
import unittest

SCRIPT = Path(__file__).resolve().parents[1] / "license-notices.py"
SPEC = importlib.util.spec_from_file_location("license_notices", SCRIPT)
MODULE = importlib.util.module_from_spec(SPEC)
SPEC.loader.exec_module(MODULE)


class DeclarationTests(unittest.TestCase):
    def test_trained_model_notices_retain_all_four_companions_and_exact_catalog(self):
        root = Path(__file__).resolve().parents[2]
        expected = {'minilm-v2-LICENSE.txt':'LICENSE',
                    'minilm-v2-TRAINING-NOTICES.md':'TRAINING-NOTICES.md',
                    'minilm-v2-provenance.json':'training-provenance.json',
                    'CC-BY-4.0.txt':'CC-BY-4.0.txt'}
        with tempfile.TemporaryDirectory() as temporary:
            output = Path(temporary)/'notices'
            MODULE.stage_model_notices(root, output)
            staged = output/'all-MiniLM-L6-v2'
            self.assertEqual({p.name for p in staged.iterdir()}, {*expected.values(),'provenance.json'})
            for source, destination in expected.items():
                self.assertEqual((staged/destination).read_bytes(),(root/'third_party/models'/source).read_bytes())
            self.assertEqual((staged/'provenance.json').read_bytes(),
                             (root/'crates/airk-retrieval/src/model_catalog.json').read_bytes())
            self.assertIn(b'Requests', (staged/'LICENSE').read_bytes())

    def test_any_mutated_companion_refuses_before_model_notice_output(self):
        root = Path(__file__).resolve().parents[2]
        for changed in ['minilm-v2-LICENSE.txt','minilm-v2-TRAINING-NOTICES.md',
                        'minilm-v2-provenance.json','CC-BY-4.0.txt']:
            with self.subTest(changed=changed), tempfile.TemporaryDirectory() as temporary:
                fixture = Path(temporary)/'source'
                models = fixture/'third_party/models'
                models.mkdir(parents=True)
                catalog = fixture/'crates/airk-retrieval/src/model_catalog.json'
                catalog.parent.mkdir(parents=True)
                shutil.copyfile(root/'crates/airk-retrieval/src/model_catalog.json',catalog)
                for name in MODULE.MODEL_COMPANIONS:
                    shutil.copyfile(root/'third_party/models'/name, models/name)
                with (models/changed).open('ab') as output:output.write(b'changed')
                destination = Path(temporary)/'output'
                with self.assertRaisesRegex(ValueError,'digest mismatch'):
                    MODULE.stage_model_notices(fixture,destination)
                self.assertFalse(destination.exists())

    def test_asn1_impl_uses_actual_texts_at_its_published_source_revision(self):
        import json
        root = Path(__file__).resolve().parents[2]
        records = json.loads((root / "third_party/licenses/sources.json").read_text(encoding="utf-8"))
        record = next(row for row in records if (row["name"],row["version"]) == ("asn1-rs-impl","0.2.0"))
        locked = next(row for row in tomllib.loads((root / "Cargo.lock").read_text(encoding="utf-8"))["package"]
                      if (row["name"],row["version"]) == ("asn1-rs-impl","0.2.0"))
        self.assertEqual(record["revision"], "a20e5f7319c896737ad0f2557037817b91ad854f")
        self.assertEqual(record["revision_basis"]["crate_sha256"], locked["checksum"])
        self.assertEqual(record["revision_basis"]["path_in_vcs"], "impl")
        self.assertTrue(record["revision_basis"]["upstream_manifest_matches_published_original"])
        self.assertFalse(record["missing_text"])
        self.assertNotIn("release_disposition", record, "Actual texts must not masquerade as a human declaration exception")
        directory = root / "third_party/licenses/asn1-rs-impl-0.2.0"
        self.assertEqual({row["file"] for row in record["files"]}, {"LICENSE-MIT","LICENSE-APACHE"})
        for item in record["files"]:
            self.assertEqual(item["url"], f'https://raw.githubusercontent.com/rusticata/asn1-rs/{record["revision"]}/{item["file"]}')
            self.assertEqual(hashlib.sha256((directory / item["file"]).read_bytes()).hexdigest(), item["sha256"])
        self.assertIn("Copyright (c) 2017 Pierre Chifflier", (directory / "LICENSE-MIT").read_text())

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



class ReleaseDispositionTests(unittest.TestCase):
    def test_disposition_applies_only_to_its_exact_version_and_revision(self):
        record = {"name": "fictional", "version": "1.0.0", "revision": "a" * 40,
                  "release_disposition": {"date": "2026-10-30", "decision": "accepted-publisher-declaration",
                                          "version": "1.0.0", "revision": "a" * 40, "evidence": "fixture",
                                          "decided_by": "maintainer"}}
        self.assertIsNotNone(MODULE.release_disposition(record, "1.0.0"))
        self.assertIsNone(MODULE.release_disposition(record, "1.0.1"), "a changed version needs re-review")
        self.assertIsNone(MODULE.release_disposition(dict(record, revision="b" * 40), "1.0.0"))
        self.assertIsNone(MODULE.release_disposition({"name": "fictional", "version": "1.0.0"}, "1.0.0"))

    def test_checked_in_unresolved_cases_have_complete_dispositions(self):
        import json
        sources = json.loads((Path(__file__).resolve().parents[2] / "third_party" / "licenses" / "sources.json").read_text(encoding="utf-8"))
        lock = (Path(__file__).resolve().parents[2] / "Cargo.lock").read_text(encoding="utf-8")
        # Historical records (for example htmlescape) left the graph and need no current disposition.
        current = [item for item in sources if item.get("missing_text")
                   and f'name = "{item["name"]}"\nversion = "{item["version"]}"' in lock]
        self.assertEqual(len(current), 3)
        for record in current:
            self.assertIsNotNone(MODULE.release_disposition(record, record["version"]), record["name"])

if __name__ == "__main__":
    unittest.main()
