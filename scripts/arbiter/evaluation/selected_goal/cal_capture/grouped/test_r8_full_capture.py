# SPDX-License-Identifier: Apache-2.0
"""Model-free checks for the fresh full calibration owner."""

from copy import deepcopy
import unittest

import r8_full_capture as capture


class FullPlanTests(unittest.TestCase):
    def test_exact_original_clock_order_and_two_lifetimes_required(self):
        plan = {"schema": capture.PLAN_SCHEMA, "common_ms": 10800000, "setup_elapsed_ms": 545806,
                "start_ms": 10000000, "deadline_ms": 20254194,
                "phase_groups": [list(range(7)), list(range(7, 13))],
                "phase_ids": [list(range(start, min(start + 8, 100))) for start in range(0, 100, 8)]}
        capture.validate_clock_and_census(plan)
        for changes in ({"deadline_ms": plan["deadline_ms"] + 1}, {"phase_groups": [list(range(13))]},
                        {"phase_ids": plan["phase_ids"][:-1]}, {"setup_elapsed_ms": 0}):
            with self.subTest(changes=list(changes)), self.assertRaises(ValueError):
                capture.validate_clock_and_census(plan | changes)
        repeated = deepcopy(plan)
        repeated["phase_ids"][-1][0] = repeated["phase_ids"][0][0]
        with self.assertRaises(ValueError):
            capture.validate_clock_and_census(repeated)


if __name__ == "__main__":
    unittest.main()
