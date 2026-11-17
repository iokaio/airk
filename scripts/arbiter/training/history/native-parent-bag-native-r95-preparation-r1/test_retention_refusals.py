# SPDX-License-Identifier: Apache-2.0
"""Inert compiler-receipt refusals, before artifact selection or native execution."""
from pathlib import Path
import json
import tempfile
import unittest
from unittest.mock import patch
import retain_compiler as owner


class CompilerReceiptRefusals(unittest.TestCase):
    def test_failed_or_unknown_compile_cannot_create_retained_artifacts(self):
        with tempfile.TemporaryDirectory() as directory:
            root = Path(directory)
            receipt = root / "receipt.json"
            log = root / "never-read.log"
            out = root / "artifacts/advanced-split/native-harnesses/fresh"
            good = {"exit_code":0,"inputs_unchanged":True,"runner_unchanged":True,
                    "runtime_unchanged":True,"command_inputs_unchanged":True}
            for field, value in [("exit_code",1),("inputs_unchanged",False),
                                 ("runner_unchanged",False),("runtime_unchanged",False),
                                 ("command_inputs_unchanged",None)]:
                receipt.write_text(json.dumps({**good,field:value}))
                with patch.object(owner,"ROOT",root), self.assertRaises(ValueError):
                    owner.retain(log,receipt,out)
                self.assertFalse(out.exists())

    def test_foreign_and_existing_output_refuse_before_reading_receipt(self):
        with tempfile.TemporaryDirectory() as directory:
            root = Path(directory)
            foreign = root / "foreign"
            with patch.object(owner,"ROOT",root), self.assertRaises(ValueError):
                owner.retain(root/"absent",root/"absent",foreign)
            out = root / "artifacts/advanced-split/native-harnesses/existing"
            out.mkdir(parents=True)
            with patch.object(owner,"ROOT",root), self.assertRaises(ValueError):
                owner.retain(root/"absent",root/"absent",out)
            self.assertEqual(list(out.iterdir()),[])


if __name__ == "__main__":
    unittest.main()
