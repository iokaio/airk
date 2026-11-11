# SPDX-License-Identifier: Apache-2.0
"""The airkctl evidence-read port must equal its script's output (split 3a.2c)."""
import importlib.util
from pathlib import Path
import unittest

ROOT = Path(__file__).resolve().parents[2]
SPEC = importlib.util.spec_from_file_location(
    "split_3a2c_port_evidence", ROOT / "scripts" / "dev" / "split_3a2c_port_evidence.py")
PORT = importlib.util.module_from_spec(SPEC)
SPEC.loader.exec_module(PORT)


class EvidencePort(unittest.TestCase):
    def test_committed_port_is_the_script_output(self):
        self.assertEqual(PORT.main(["--check"]), 0)

    def test_every_reviewed_variant_and_mapping_is_ported(self):
        text = PORT.render(PORT.SOURCE.read_text(encoding="utf-8"))
        for name in PORT.VARIANTS:
            self.assertIn(f"\n    {name} {{\n", text, name)
            self.assertIn(f"EvidenceCommand::{name} {{", text, name)
        self.assertIn("pub fn query(command: EvidenceCommand)", text)
        self.assertNotIn("Command::Status", text.replace("EvidenceCommand::Status", ""))

    def test_a_missing_source_item_is_refused(self):
        with self.assertRaises(ValueError):
            PORT.render("fn unrelated() {}\n")


if __name__ == "__main__":
    unittest.main()
