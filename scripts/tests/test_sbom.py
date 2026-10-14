# SPDX-License-Identifier: Apache-2.0
import importlib.util
import json
from pathlib import Path
import unittest

spec = importlib.util.spec_from_file_location("notices", Path(__file__).parents[1] / "license-notices.py")
notices = importlib.util.module_from_spec(spec)
spec.loader.exec_module(notices)


class SbomContracts(unittest.TestCase):
    def test_graph_has_public_identity_without_build_paths(self):
        local = "path+file:///private/fictional-owner/workspace#airk@0.1.0"
        dep = "registry+https://github.com/rust-lang/crates.io-index#fixture@1.2.3"
        metadata = {"packages": [
            {"id": local, "name": "airk", "version": "0.1.0", "license": "Apache-2.0", "source": None},
            {"id": dep, "name": "fixture", "version": "1.2.3", "license": "MIT", "source": "registry+https://github.com/rust-lang/crates.io-index"}],
            "resolve": {"nodes": [{"id": local, "deps": [{"pkg": dep}]}, {"id": dep, "deps": []}]}}
        bom = notices.software_bill_of_materials(metadata)
        encoded = json.dumps(bom)
        self.assertNotIn("fictional-owner", encoded)
        self.assertNotIn("file:///", encoded)
        refs = {item["bom-ref"] for item in bom["components"]}
        self.assertEqual(len(refs), 2)
        for edge in bom["dependencies"]:
            self.assertIn(edge["ref"], refs)
            self.assertTrue(set(edge["dependsOn"]).issubset(refs))


if __name__ == "__main__":
    unittest.main()
