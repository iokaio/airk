# SPDX-License-Identifier: Apache-2.0
"""Regression checks for full-population partitioning and legal-option margins."""
from pathlib import Path
import runpy
import unittest

owner = runpy.run_path(str(Path(__file__).resolve().parents[1] / "arbiter-fp16-execution/compact_train_batch_reference.py"))


class TrainBatchTests(unittest.TestCase):
    def test_partitions_preserve_every_result(self):
        roster = [{"case": str(i)} for i in range(15)]
        cohort = {"rows": [dict(row, state=["admitted", "budget_stop_zero", "unqualified_capture"][i % 3])
                           for i, row in enumerate(roster)]}
        cases = [[r["case"] for _, r in owner["batch_entries"](cohort, roster, n)] for n in range(1, 6)]
        self.assertEqual(cases[0], ["0", "1", "2"])
        self.assertEqual(cases[4], ["12", "13", "14"])
        self.assertEqual([item for batch in cases for item in batch], [str(i) for i in range(15)])
        cohort["rows"][-1]["state"] = "unobserved"
        self.assertEqual(len(owner["batch_entries"](cohort, roster, 1)), 3)
        with self.assertRaises(ValueError):
            owner["batch_entries"](cohort, roster, 5)

    def test_order_change_rejects(self):
        roster = [{"case": str(i)} for i in range(15)]
        cohort = {"rows": [dict(row, state="admitted") for row in reversed(roster)]}
        with self.assertRaises(ValueError):
            owner["batch_entries"](cohort, roster, 2)

    def test_masked_highest_and_threshold_edges(self):
        for value, release, narrow, wide in ((.014, True, True, True), (.020, False, True, True), (.140, False, False, True), (.141, False, False, False)):
            row = owner["bounds"]([23.0, value, 0], [False, True, True])
            self.assertEqual((row["winner"], row["within_0014"], row["within_0020"], row["within_0140"]), (1, release, narrow, wide))
        self.assertIsNone(owner["bounds"]([1.0], [True])["top_two_margin"])
        with self.assertRaises(ValueError):
            owner["bounds"]([1.0, 2.0], [True])


class OriginalFinalGroup(unittest.TestCase):
    def test_q543_future_reuse_binds_offered_choices_and_raw_capture(self):
        source = {"case": "TRAIN_Q543", "case_evidence": {"capture_sha256": owner["Q543_CAPTURE"]}}
        admission = {"state": "eligible", "offered_choices": ["a", "b"], "compiled": {"input_ids": [2]}}
        prior = admission | {"state": "scored", "matching_fp16": [2, 1], "original_fp32": [1, 2]}
        run = {"exit_code": 0, "owned_descendants_settled": True, "owned_job_final_active_processes": 0,
               "owned_job_final_member_pids": [], "timed_out": False, "owned_job_forced_termination": False}
        evidence = {"rows": [source | {"admissions": [prior]}], "batch": 5, "cohort": {"sha256": owner["ORIGINAL_PREFIX_SHA"]},
                    "reference_basis": "b", "runtime": "r", "graph_sha256": "g", "ragged_t65_constant": True,
                    "matching_t128_words_equal": True, "original_t128_max_abs": .0001}
        args = ({"complete": True, "owned": run, "result": evidence}, source | {"admissions": [admission]})
        rest = ({"candidate": {"graph": {"sha256": "g"}}}, "b", "r")
        self.assertEqual(owner["reuse_q543"](*args, admission, *rest), ([2, 1], [1, 2]))
        with self.assertRaisesRegex(ValueError, "identity"):
            owner["reuse_q543"](*args, admission | {"offered_choices": ["b", "a"]}, *rest)
    def test_q543_cutoff_does_not_count_later_attempts(self):
        roster = [{"case": str(i)} for i in range(12)] + [{"case": "TRAIN_Q" + n} for n in ("543", "564", "598")]
        rows = [row | {"state": "admitted"} for row in roster]
        rows[12]["capture_sha256"] = owner["Q543_CAPTURE"]
        rows[13]["state"], rows[14]["state"] = "unqualified_capture", "unobserved"
        self.assertEqual(owner["batch_entries"]({"rows": rows}, roster, 5, True), [(roster[12], rows[12])])
        with self.assertRaisesRegex(ValueError, "settled chronological"):
            owner["batch_entries"]({"rows": rows}, roster, 5)
        rows[12]["capture_sha256"] = "stale"
        with self.assertRaisesRegex(ValueError, "cutoff"):
            owner["batch_entries"]({"rows": rows}, roster, 5, True)


class OptionalWindowsWorker(unittest.TestCase):
    def test_excludes_busy_ideal_siblings_and_prefers_quiet_distinct_cores(self):
        pick = runpy.run_path(str(owner["AFFINITY"]))["pick_cpus"]
        sample = {"system_mask": "0xff", "threads": [{"ideal": [0, 1], "cpu_seconds": 2}],
                  "machine_cpu_busy_percent": [0, 0, 3, 2, 75, 1, 5, 4],
                  "physical_siblings": [[0, 1], [2, 3], [4, 5], [6, 7]]}
        self.assertEqual(pick(sample, 2), [3, 7])
        with self.assertRaisesRegex(ValueError, "insufficient distinct"):
            pick(sample, 4)


if __name__ == "__main__":
    unittest.main()
