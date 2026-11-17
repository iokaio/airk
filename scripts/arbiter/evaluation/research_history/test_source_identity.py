# SPDX-License-Identifier: Apache-2.0
"""Read-only historical source identity; no worker imports or private-data reads."""
import ast
import hashlib
import json
from pathlib import Path
import unittest
BASE = Path(__file__).resolve().parent
ROOT = next(p for p in BASE.parents if (p / "Cargo.toml").is_file())
MAP = json.loads((BASE / "source-provenance.json").read_bytes())
class SourceIdentity(unittest.TestCase):
    def test_every_maintained_hash_and_exact_copy_claim(self):
        seen = {}
        for row in MAP["files"]:
            path = ROOT / row["destination"]
            raw = path.read_bytes()
            self.assertEqual(hashlib.sha256(raw).hexdigest(), row["maintained_sha256"])
            self.assertEqual(len(raw), row["maintained_bytes"])
            if row["disposition"] in {"byte-identical-historical-source", "spdx-prefix-historical-source"}:
                original = raw
                if row["disposition"] == "spdx-prefix-historical-source":
                    self.assertEqual(len(row["changes"]), 1)
                    change = row["changes"][0]
                    self.assertEqual(change["kind"], "spdx-prefix")
                    prefix = change["prefix"].encode()
                    self.assertIn(prefix, [marker + ending for marker in [b"# SPDX-License-Identifier: Apache-2.0", b"// SPDX-License-Identifier: Apache-2.0"] for ending in [b"\n", b"\r\n"]])
                    self.assertTrue(raw.startswith(prefix))
                    original = raw[len(prefix):]
                self.assertEqual(hashlib.sha256(original).hexdigest(), row["original_sha256"])
                self.assertEqual(len(original), row["bytes"])
                self.assertEqual(seen.setdefault(row["original_sha256"], row["destination"]), row["destination"])
            else:
                owner = json.loads((ROOT / row["existing_owner_map"]).read_bytes())
                self.assertTrue(any(r.get("original_sha256") == row["original_sha256"] and r.get("destination") == row["destination"] and r.get("maintained_sha256") == row["maintained_sha256"] for r in owner["files"]))
    def test_source_only_roster_and_parsing(self):
        expected = {ROOT / r["destination"] for r in MAP["files"] if r["disposition"] in {"byte-identical-historical-source", "spdx-prefix-historical-source"}}
        expected |= {BASE / "test_source_identity.py", BASE / "source-provenance.json", BASE / "source-disposition.json", BASE / "SECTION7.md"}
        for path in BASE.rglob("*"):
            if path.is_dir():
                self.assertTrue((path / "README.md").is_file())
            elif path.name != "README.md":
                self.assertIn(path, expected)
                if path.suffix in {".py", ".ps1", ".cjs"}:
                    self.assertIn(b"SPDX-License-Identifier: Apache-2.0", path.read_bytes().splitlines()[0])
                if path.suffix == ".py":
                    ast.parse(path.read_bytes(), filename=str(path))
    def test_literal_exact_and_prefix_counts(self):
        exact = {r["destination"] for r in MAP["files"] if r["disposition"] == "byte-identical-historical-source"}
        prefixed = {r["destination"] for r in MAP["files"] if r["disposition"] == "spdx-prefix-historical-source"}
        self.assertEqual((len(exact), len(prefixed)), (134, 270))
        self.assertFalse(exact & prefixed)
        self.assertEqual(MAP["copy_transform_counts"], {"unique_destinations": 404, "byte_identical": 134, "spdx_prefix_only": 270})

    def test_selected_orchestration_versions_are_preserved(self):
        expected = {"goal_rubric.py": "ba9e791f79d33f0631ffd4c5543ec52891b4fe922c2f108ef2828bf5a27e64dc", "goal_trial.py": "cd0d8d0a1ee225ed44bef68f5764216dffb9f0ce518dc0abc5953fc2925a21b2", "grade_native_retrieval.py": "41c418c223c136f95a09aeed1815fade89dab3e87e0b460ebf0dfed6213e029f", "native_goal_client.py": "f09bed61db04f382665157525de99f9b9dab175873a2cdef16bc8e6b723e3c0b", "native_policy.py": "35b034526a78c2a1aacd5de9abdcd36d527381be34a3b1ffddd12ba462f1284d", "os_measurement.py": "c81d48c34df43b8c6adc3b07d782bfca4bb518cfd707287dc938f0fd9c67b0bc"}
        for name, digest in expected.items():
            self.assertTrue(any(Path(r["source"]).name == name and r["original_sha256"] == digest for r in MAP["files"]))
        self.assertEqual(len(MAP["excluded_vendor_sources"]), 3)
if __name__ == "__main__":
    unittest.main(verbosity=2)
