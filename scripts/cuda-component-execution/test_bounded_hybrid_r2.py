# SPDX-License-Identifier: Apache-2.0
"""Pure controls for the held CUDA R2 launch and refresh seams."""

import importlib.util
from pathlib import Path
import unittest

path = Path(__file__).with_name("bounded_hybrid_r2.py")
spec = importlib.util.spec_from_file_location("compact_hybrid_r2_test", path)
launcher = importlib.util.module_from_spec(spec)
spec.loader.exec_module(launcher)


class Controls(unittest.TestCase):
    def test_frozen_r1_source_has_exact_adapter_anchors(self):
        pin, raw = launcher.code(launcher.R1, launcher.R1_SHA)
        owner = launcher.reviewed_launcher(pin, raw, (1, None))
        self.assertNotEqual(owner.SCHEMA, "airk.private.compact-bounded-hybrid-plan/1")
        self.assertEqual(owner.outer_clock, (1, None))
        self.assertIn("AIRK_CUDA_HYBRID_CAPACITY", repr(owner.execute.__code__.co_consts))
        self.assertEqual(owner.previous_launcher, pin)

    def test_exact_predecessor_is_required_in_runtime_roster(self):
        runtime = Path(__file__).parent
        old = {"path": str(runtime / "original.exe"), "bytes": 1, "sha256": "first"}
        predecessor = {"path": str(runtime / "predecessor.exe"), "bytes": 2, "sha256": "second"}
        seed = {"runtime_files": [old], "runtime_root": str(runtime)}
        rows = launcher.runtime_pins(seed, predecessor)
        launcher.validate_runtime_rows([{"pin": old}, {"pin": predecessor}], rows)
        with self.assertRaisesRegex(ValueError, "complete original roster"):
            launcher.validate_runtime_rows([{"pin": old}], rows)
        with self.assertRaisesRegex(ValueError, "exact additive predecessor"):
            launcher.runtime_pins(seed, old)


if __name__ == "__main__":
    unittest.main()
