# SPDX-License-Identifier: Apache-2.0
import importlib.util
from pathlib import Path
import unittest

SPEC = importlib.util.spec_from_file_location("usage_evidence", Path(__file__).parents[1] / "usage-evidence.py")
MODULE = importlib.util.module_from_spec(SPEC)
SPEC.loader.exec_module(MODULE)
MODEL = "a" * 64


class UsageEvidenceTests(unittest.TestCase):
    def test_provider_counters_and_cache_writes(self):
        session = MODULE.adapt_usage("openai-responses", [{"input_tokens": 100, "input_tokens_details": {"cached_tokens": 40, "cache_write_tokens": 0}, "output_tokens": 20}], MODEL)
        self.assertEqual((session["input_tokens"], session["cached_input_tokens"], session["output_tokens"]), (100, 40, 20))
        session = MODULE.adapt_usage("anthropic-messages", [{"input_tokens": 100, "cache_read_input_tokens": 40, "cache_creation_input_tokens": 10, "output_tokens": 20}], MODEL)
        self.assertEqual(session["input_tokens"], 150)
        self.assertIsNone(session["cached_input_tokens"], "cache-write rates cannot be priced as ordinary input")

    def test_missing_counts_never_become_zero_and_content_is_rejected(self):
        session = MODULE.adapt_usage("openai-responses", [{"input_tokens": 100}, {"output_tokens": 20}], MODEL)
        self.assertIsNone(session["input_tokens"])
        self.assertIsNone(session["output_tokens"])
        session = MODULE.adapt_usage("openai-responses", [{"input_tokens": 100, "input_tokens_details": {"cached_tokens": 40}}], MODEL)
        self.assertIsNone(session["cached_input_tokens"], "unknown cache writes must not be silently priced as ordinary input")
        for bad in [{"input_tokens": True}, {"input_tokens": -1}, {"prompt": "private"}, {"input_tokens_details": {"prompt": "private"}}]:
            with self.assertRaises(ValueError):
                MODULE.adapt_usage("openai-responses", [bad], MODEL)

    def test_pairing_keeps_failure_and_negative_differences(self):
        a = MODULE.adapt_usage("openai-responses", [{"input_tokens": 150, "output_tokens": 12}], MODEL, 400)
        b = MODULE.adapt_usage("openai-responses", [{"input_tokens": 100, "output_tokens": 10}], MODEL, 300)
        pair = MODULE.paired(a, b, "failed", "passed", True)
        summary = MODULE.compare([pair])
        self.assertEqual(summary["quality"]["only_alternative_passed"], 1)
        self.assertEqual(summary["all_pairs"]["input_tokens"]["total_reduction"], -50)
        self.assertIsNone(summary["both_passed_pairs"]["input_tokens"]["total_reduction"])
        self.assertIsNone(summary["all_pairs"]["tool_round_trips"]["total_reduction"])
        with self.assertRaises(ValueError):
            MODULE.paired(a, {**b, "model_revision_sha256": "b" * 64}, "passed", "passed", True)
        with self.assertRaises(ValueError):
            MODULE.paired(a, b, "passed", "passed", False)

    def test_ids_are_explicit_stable_and_bounded(self):
        kwargs = (None, "1" * 32, 1000, "codex", ["2" * 32])
        self.assertEqual(MODULE.measurement(*kwargs), MODULE.measurement(*kwargs))
        with self.assertRaises(ValueError):
            MODULE.measurement(None, "1" * 32, 1000, "codex", ["2" * 32] * 2)


if __name__ == "__main__":
    unittest.main()
