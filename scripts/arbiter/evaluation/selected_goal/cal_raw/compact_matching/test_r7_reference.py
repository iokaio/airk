# SPDX-License-Identifier: Apache-2.0
"""Model-free checks for R7 CPU reference admission and provenance."""
from pathlib import Path
import sys
import unittest
from unittest.mock import patch

sys.path.insert(0, str(Path(__file__).resolve().parent))
import r7_reference as owner


class R7ReferenceAdmission(unittest.TestCase):
    def test_wrong_or_duplicate_worker_placement_refuses_before_loading_science(self):
        for cpus, priority in (([5, 5], 0x4000), ([5, 7], 0x20), ([5], 0x4000)):
            with (
                self.subTest(cpus=cpus, priority=priority),
                patch.object(owner, "place_optional_worker", return_value={"logical_cpus": cpus, "priority_class": priority}),
                patch.object(owner, "owners") as load,
            ):
                with self.assertRaisesRegex(ValueError, "two quiet physical cores"):
                    owner.checked_science([], "protected_matching", {}, 600.0, 17, None, {}, {})
                load.assert_not_called()

    def test_attempt_worker_does_not_receive_a_fresh_reference_clock(self):
        with patch.object(owner.time, "monotonic", return_value=27.0):
            owner.check_deadline(27.01)
            with self.assertRaisesRegex(ValueError, "Job deadline expired"):
                owner.check_deadline(27.0)

    def test_reference_identity_preserves_original_execution_and_graph(self):
        provenance = {
            "model_sha256": "a" * 64, "weight_sha256": "b" * 64, "matching_recipe_sha256": "c" * 64,
            "projection": {"sha256": "d" * 64},
        }
        result = {
            "provenance": provenance, "qualification": False, "candidate_graph": {"sha256": "a" * 64},
            "candidate_weights": {"sha256": "b" * 64}, "matching_recipe_sha256": "c" * 64,
        }
        with patch.object(owner, "reference_identity") as historical:
            owner.verify_identity(result, provenance)
            historical.assert_called_once_with(result)
            for bad in ({"qualification": True}, {"candidate_weights": {"sha256": "e" * 64}},
                        {"provenance": provenance | {"projection": {"sha256": "f" * 64}}}):
                with self.subTest(bad=bad), self.assertRaisesRegex(ValueError, "same protected identity"):
                    owner.verify_identity(result | bad, provenance)


if __name__ == "__main__":
    unittest.main()
