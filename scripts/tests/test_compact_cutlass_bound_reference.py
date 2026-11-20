# SPDX-License-Identifier: Apache-2.0
"""Original width and source epoch must agree before the protected CPU model can load."""
from pathlib import Path
import runpy
from types import SimpleNamespace
import unittest


M = runpy.run_path(str(Path(__file__).resolve().parents[1] / "arbiter-fp16-execution/compact_cutlass_bound_reference.py"))


class BoundSources(unittest.TestCase):
    def bind(self, expected_case, digest, measured_width=32):
        row = {"input_ids": [0] * measured_width, "option_mask": [True] * 8}
        source = {"input": row, "producer": "producer", "fixture": "original", "compiler": "compiler",
                  "resource_policy": "budget", "name": "original", "original_fp32": [1.0] * 8}
        def read(path, seal, check):
            self.assertEqual(path.name, "compact-qkv-fp32-t32-reference.json")
            self.assertEqual(seal, M["NARROW"])
            return {"reference": source}, {"sha256": seal}
        return M["bind"](SimpleNamespace(CAMPAIGN=Path("campaign")), SimpleNamespace(read_pinned_json=read),
                         expected_case, digest, 1, lambda: None)

    def test_exact_width_and_original_publisher_vector(self):
        case, census, capture, source, row = self.bind("CONTROL_T32", M["NARROW"])
        self.assertEqual((case, census, capture), ("CONTROL_T32", {"sha256": M["NARROW"]}, {"sha256": M["NARROW"]}))
        self.assertEqual(source["original_fp32"], [1.0] * 8)
        self.assertEqual(len(row["input_ids"]), 32)

    def test_loaded_entry_requires_the_named_adapter_and_exact_parent_digest(self):
        here = M["HERE"]
        cutlass = runpy.run_path(str(here / "compact_cutlass_reference.py"))
        replay = SimpleNamespace(**runpy.run_path(str(here / "replay_layer0.py")))
        validate = runpy.run_path(str(here / "verify_compact.py"))["loaded_runtime"]
        path = here / "compact_cutlass_bound_reference.py"
        check = lambda: None
        real = replay.file_pin(path, check)
        pin = cutlass["bound_entry_pin"](path, real["sha256"], replay, check)
        module = SimpleNamespace(__file__=str(path))
        with self.assertRaisesRegex(ValueError, "outside sealed closure"):
            validate({}, replay.file_pin, check, [module])
        validate({pin["path"]: pin}, replay.file_pin, check, [module])
        with self.assertRaisesRegex(ValueError, "changed"):
            cutlass["bound_entry_pin"](path, "0" * 64, replay, check)
        with self.assertRaisesRegex(ValueError, "unknown"):
            cutlass["bound_entry_pin"](here / "compact_cutlass_reference.py", real["sha256"], replay, check)

    def test_wrong_width_or_source_refused(self):
        with self.assertRaisesRegex(ValueError, "width"):
            self.bind("CONTROL_T32", M["NARROW"], 31)
        with self.assertRaisesRegex(ValueError, "source"):
            self.bind("TRAIN_Q543", M["WIDE"])


if __name__ == "__main__":
    unittest.main()
