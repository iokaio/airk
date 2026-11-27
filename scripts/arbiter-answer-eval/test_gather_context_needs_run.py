# SPDX-License-Identifier: Apache-2.0
"""Model-free controls for the caller inputs of the needs runner."""

import unittest

import gather_context_needs_run as runner


class CallerInputsTest(unittest.TestCase):
    def test_needs_and_anchors_pass_through_in_order(self):
        written = {"a-C1": {"needs": ["first need", "second need"], "anchors": ["X_Y", "409"]}}
        self.assertEqual(runner.caller_inputs(written, "a-C1"),
                         {"needs": ["first need", "second need"], "anchors": ["X_Y", "409"]})

    def test_no_anchors_sends_no_anchor_field(self):
        self.assertEqual(runner.caller_inputs({"a-C1": {"needs": ["one"], "anchors": []}}, "a-C1"),
                         {"needs": ["one"]})

    def test_missing_empty_or_oversized_inputs_are_refused(self):
        for written in ({}, {"a-C1": {"needs": []}}, {"a-C1": {"needs": ["ok", "  "]}},
                        {"a-C1": {"needs": [f"need {n}" for n in range(9)]}},
                        {"a-C1": {"needs": ["ok"], "anchors": [f"a{n}" for n in range(17)]}}):
            with self.assertRaises(ValueError):
                runner.caller_inputs(written, "a-C1")


if __name__ == "__main__":
    unittest.main()
