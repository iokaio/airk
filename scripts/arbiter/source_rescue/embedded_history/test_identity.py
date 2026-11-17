# SPDX-License-Identifier: Apache-2.0
"""No recovered program imports, private-container reads or generated outputs."""
import ast
import hashlib
import importlib.util
import json
from pathlib import Path
import unittest
BASE = Path(__file__).resolve().parent
ROOT = BASE.parents[3]
MAP = json.loads((BASE / "source-provenance.json").read_bytes())
class EmbeddedIdentity(unittest.TestCase):
    def test_literal_roster_and_every_inverse_identity(self):
        self.assertEqual(len(MAP["files"]), 124)
        self.assertEqual(len(MAP["exclusions"]), 47)
        paths = set()
        for row in MAP["files"]:
            raw = (ROOT / row["destination"]).read_bytes()
            self.assertEqual((len(raw), hashlib.sha256(raw).hexdigest()), (row["maintained_bytes"], row["maintained_sha256"]))
            prefix = row["prefix"].encode()
            self.assertTrue(raw.startswith(prefix))
            original = raw[len(prefix):]
            self.assertEqual((len(original), hashlib.sha256(original).hexdigest()), (row["decoded_bytes"], row["decoded_sha256"]))
            self.assertTrue(row["json_pointer"].startswith("/"))
            self.assertEqual(len(row["container_sha256"]), 64)
            if row["disposition"] == "extracted-source":
                paths.add(row["destination"])
            if row["language"] == "python":
                ast.parse(raw, filename=row["destination"])
        self.assertEqual(len(paths), 101)
    def test_source_roster_and_spdx(self):
        expected = {ROOT / r["destination"] for r in MAP["files"]}
        expected |= {BASE / "extract.py", BASE / "test_identity.py", BASE / "source-provenance.json"}
        snapshots = json.loads((BASE / "snapshots/source-provenance.json").read_bytes())
        self.assertEqual(len(snapshots["files"]), 71)
        expected |= {ROOT / row["destination"] for row in snapshots["files"]}
        expected |= {BASE / "snapshot_plan.py", BASE / "test_snapshots.py", BASE / "snapshots/source-provenance.json"}
        for map_name in ("source-provenance.json", "encoded-source-provenance.json"):
            additional = json.loads((BASE / "supplemental" / map_name).read_bytes())
            expected |= {ROOT / row["destination"] for row in additional["files"]}
            expected.add(BASE / "supplemental" / map_name)
        expected |= {BASE / "supplemental/rescue.py", BASE / "test_supplemental.py", BASE / "classify_remainder.py", BASE / "classification-scope.json"}
        for path in BASE.rglob("*"):
            if path.is_dir():
                self.assertTrue((path / "README.md").is_file())
            elif path.name != "README.md":
                self.assertIn(path, expected)
                if path.suffix in {".py", ".sh", ".ps1"}:
                    self.assertIn(b"SPDX-License-Identifier: Apache-2.0", path.read_bytes().splitlines()[0])
    def test_pointer_escape_and_array_resolution(self):
        spec = importlib.util.spec_from_file_location("embedded_extract", BASE / "extract.py")
        tool = importlib.util.module_from_spec(spec)
        spec.loader.exec_module(tool)
        document = {"a/b": [{"~key": "synthetic code"}]}
        self.assertEqual(tool.pointer(document, "$/a~1b/0/~0key"), "synthetic code")
        with self.assertRaises(ValueError):
            tool.pointer(document, "/a~1b/0/~0key")
    def test_gap_and_upstream_disclosures(self):
        self.assertFalse(MAP["inspection_gaps"]["complete_source_coverage"])
        self.assertEqual(MAP["inspection_gaps"]["permission_errors"], 48021)
        self.assertEqual(MAP["inspection_gaps"]["status_counts"]["invalid_json_uninspected"], 25221)
        for row in MAP["exclusions"]:
            self.assertEqual(row["disposition"], "upstream_commit_patch")
            self.assertNotIn("/iokaio/airk/", row["evidence_url"])
if __name__ == "__main__":
    unittest.main(verbosity=2)
