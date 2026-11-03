# SPDX-License-Identifier: Apache-2.0
import importlib.util
from pathlib import Path
import tempfile
import unittest

SPEC = importlib.util.spec_from_file_location("airk_skip_inventory", Path(__file__).parents[1] / "report-test-skips.py")
MODULE = importlib.util.module_from_spec(SPEC)
SPEC.loader.exec_module(MODULE)


class SkipInventoryTests(unittest.TestCase):
    def test_helpers_qualification_and_missing_reason_debt_stay_distinct(self):
        with tempfile.TemporaryDirectory() as temporary:
            root = Path(temporary)
            source = root / "crates/fictional/tests/owned.rs"
            source.parent.mkdir(parents=True)
            source.write_text('#[ignore = "owned subprocess only"]\nfn fixture() {}\n'
                              '#[ignore = "requires approved pair"]\nasync fn qualification() {}\n'
                              '#[ignore]\nfn unknown() {}\n', encoding="utf-8")
            config = root / ".config/nextest.toml"
            config.parent.mkdir()
            config.write_text('[profile.ci]\ndefault-filter = "not (test(long_one) | test(long_two))"\n', encoding="utf-8")
            rows, exclusions = MODULE.inventory(root)
            self.assertEqual([(r["name"], r["category"]) for r in rows],
                             [("fixture", "helper"), ("qualification", "qualification"), ("unknown", "debt")])
            self.assertEqual(exclusions, ["long_one", "long_two"])
            output = MODULE.render(rows, exclusions, "fixture-revision", "2026-11-03")
            self.assertIn("**3 literal Rust ignore attributes**", output)
            self.assertIn("**1 missing-reason debt entries**", output)
            self.assertIn("excludes **2**", output)
            self.assertIn("Never run helper entries standalone", output)
            self.assertIn("fixture-revision", output)

    def test_live_inventory_does_not_equate_attribute_count_with_runner_skips(self):
        rows, exclusions = MODULE.inventory()
        self.assertGreater(len(rows), 0)
        self.assertEqual(len({(row["path"], row["name"]) for row in rows}), len(rows))
        self.assertEqual(len(exclusions), 4)
        self.assertTrue(all(row["reason"] for row in rows))


if __name__ == "__main__":
    unittest.main()
