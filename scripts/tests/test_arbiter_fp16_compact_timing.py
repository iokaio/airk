# SPDX-License-Identifier: Apache-2.0
"""Acceptance and refusal cases for the unprofiled Compact timing populations."""
from pathlib import Path
import runpy
import unittest

OWNER = runpy.run_path(str(Path(__file__).resolve().parents[1] / "arbiter-fp16-execution/timing_control.py"))


class TimingTests(unittest.TestCase):
    def rows(self, widths):
        sealed = {width: {"input_digest": str(width), "raw_scores": list(range(8))} for width in set(widths)}
        rows = []
        for i, width in enumerate(widths):
            rows.append(dict(sealed[width], tokens=width, cycle=0 if len(widths) == 21 else 1+i//5,
                             sample_kind="warmup" if len(widths) == 21 and i == 0 else "sample",
                             forward_synchronized=True, within_deadline=True,
                             native_call_elapsed_us=10+i, elapsed_us=100+i))
        return sealed, rows

    def test_fixed_nearest_ranks_exclude_warmup(self):
        sealed, rows = self.rows([8192]*21)
        rows[0]["elapsed_us"] = 119999999
        summary = OWNER["timing_rows"]("qkv-resource20", rows, sealed)
        self.assertEqual(summary, {"warm_samples": 20, "nearest_rank_p50_us": 110,
                                   "nearest_rank_p95_us": 119, "maximum_us": 120})

    def test_mixed_short_returns_and_exact_score_words(self):
        sealed, rows = self.rows([32,128,1024,8192,32] * 3)
        self.assertEqual(OWNER["timing_rows"]("qkv-mixed", rows, sealed)["warm_samples"], 15)
        rows[4] = dict(rows[4], raw_scores=[-0.0,1,2,3,4,5,6,7])
        with self.assertRaisesRegex(ValueError, "score"):
            OWNER["timing_rows"]("qkv-mixed", rows, sealed)
        rows[4] = dict(rows[0], tokens=8192)
        with self.assertRaisesRegex(ValueError, "width"):
            OWNER["timing_rows"]("qkv-mixed", rows, sealed)

    def test_journal_uses_warmup_and_every_real_sample_ordinal(self):
        validate = runpy.run_path(str(Path(__file__).resolve().parents[1] /
                                     "arbiter-fp16-execution/probe_compact.py"))["validate_development_operations"]
        for mode, widths in (("qkv-resource20", [8192]*21), ("qkv-mixed", [32,128,1024,8192,32]*3)):
            _, rows = self.rows(widths)
            events = [{"identity": "image"}, {"observed_monotonic_ns": 1}]
            for i, row in enumerate(rows):
                for step, phase in enumerate(("started", "returned")):
                    count = 2*i+step+3
                    events.append({"schema": "airk.arbiter.native-operation/1", "sequence": count, "identity": "image",
                                   "kind": row["sample_kind"], "ordinal": i if mode == "qkv-resource20" else i+1,
                                   "phase": phase, "clock": "windows_performance_counter",
                                   "operation_input_digest": row["input_digest"], "expires_monotonic_ns": 100,
                                   "observed_monotonic_ns": count, "started_monotonic_ns": count-step,
                                   "native_success": True if step else None,
                                   "native_call_elapsed_us": row["native_call_elapsed_us"] if step else None})
            value = {"rows": rows, "original_expires_monotonic_ns": 100}
            checker = {"validate_operations": lambda *_, **kwargs: self.assertTrue(kwargs["load_only"])}
            validate(events, value, checker, mode)
            events[-1]["kind"] = "numeric"
            with self.assertRaisesRegex(ValueError, "operation identity"):
                validate(events, value, checker, mode)

    def test_refuses_missing_sample(self):
        sealed, rows = self.rows([8192]*21)
        with self.assertRaisesRegex(ValueError, "length"):
            OWNER["timing_rows"]("qkv-resource20", rows[:-1], sealed)


if __name__ == "__main__":
    unittest.main()
