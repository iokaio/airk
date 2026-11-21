# SPDX-License-Identifier: Apache-2.0
"""Offline reclassification controls with synthetic receipts and no device or campaign access."""
from pathlib import Path
import sys
import unittest

sys.path.insert(0, str(Path(__file__).resolve().parent))
from capacity_ballast_worker import GIB
from replay_capacity_rule import RETAINED, classify


def pair(rows, *, tier=12, native_complete=True, pressure_complete=True):
    """Rows are (dedicated, shared, nonlocal) samples of one owned candidate under a 77,594,624 B control."""
    series = []
    for index, (dedicated, shared, nonlocal_) in enumerate(rows):
        tick = index + 1
        series.append({"process":{"Dedicated Usage":dedicated, "Shared Usage":shared},
                       "segment_bytes":{"Local Usage":dedicated, "Non Local Usage":nonlocal_},
                       "counter_filetime_100ns":{"process/Dedicated Usage":tick, "process/Shared Usage":tick,
                                                 "segment_bytes/Local Usage":tick, "segment_bytes/Non Local Usage":tick},
                       "end_monotonic_ns":tick})
    limit = {"process_memory_limit_bytes":12*GIB, "job_memory_limit_bytes":12*GIB}
    native = {"complete":native_complete, "mode":"pressure-fit", "image":{"sha256":"a"*64},
              "error":None if native_complete else "ValueError: wrapper refused",
              "owned":{"owned_leader_pid":7, "exit_code":0, "suspended_assignment":{"observed_limits":limit},
                       "before_GO":{"wddm":{"pid":7, "adapter_luid":"luid", "samples":series}}}}
    outer = {"complete":pressure_complete, "tier_gib":tier,
             "sample_error":None if pressure_complete else "ballast shared usage increased during allocation or hold",
             "admission":{"candidate_pid":7, "snapshot":{"dxgi":{"luid":"luid"}},
                          "sample":{"process":{"Shared Usage":77_594_624}}},
             "trial":{"mode":"pressure-fit", "candidate_pid":7, "capacity_admitted":False,
                      "observed_outcome":"observed_shared_usage", "image":{"sha256":"a"*64},
                      "probe_source_sha256":"c"*64, "host_limit_bytes":12*GIB}}
    return outer, native


class ReplayControls(unittest.TestCase):
    LIVE = (6_706_245_632, 79_691_776, 79_691_776)

    def test_retained_roster_is_the_five_compact_fits_and_the_one_large_fit(self):
        self.assertEqual([row[:3] for row in RETAINED], [
            ("compact", 24, "compact-capacity-protected-24-gib-pressure-fit-r1"),
            ("compact", 16, "compact-capacity-protected-16-gib-pressure-fit-r2"),
            ("compact", 12, "compact-capacity-protected-12-gib-pressure-fit-r1"),
            ("compact", 8, "compact-capacity-protected-8-gib-pressure-fit-r2"),
            ("compact", 6, "compact-capacity-protected-6-gib-pressure-fit-r1"),
            ("large", 24, "large-capacity-direct-pressure-24-gib-fit-r1")])

    def test_clean_fit_two_mib_over_the_control_is_reclassified_as_admitted(self):
        row = classify(*pair([self.LIVE]*3), "compact", 12)
        self.assertEqual((row["reclassified_outcome"], row["reclassified_admitted"]), ("admitted", True))
        self.assertEqual((row["historical_outcome"], row["historical_capacity_admitted"]),
                         ("observed_shared_usage", False))
        self.assertEqual((row["capacity_rule"]["shared_excess_bytes"],
                          row["capacity_rule"]["dedicated_plus_reserve_bytes"]), (2_097_152, 8_853_729_280))

    def test_memory_can_fit_while_the_native_wrapper_refusal_still_blocks(self):
        row = classify(*pair([self.LIVE]*3, tier=16, native_complete=False), "compact", 16)
        self.assertEqual((row["reclassified_outcome"], row["reclassified_admitted"], row["capacity_rule"]["satisfied"]),
                         ("native_run_incomplete", False, True))
        self.assertEqual(row["native_failure"], "ValueError: wrapper refused")

    def test_rule_refusals_and_lost_pressure_are_reported_as_refusals(self):
        row = classify(*pair([(6_714_634_240, 79_691_776, 79_691_776)]*3, tier=8), "compact", 8)
        self.assertEqual((row["reclassified_outcome"], row["reclassified_admitted"]),
                         ("dedicated_usage_exceeds_tier_reserve", False))
        row = classify(*pair([(6_572_290_048, 213_909_504, 213_909_504)]*3, tier=6, pressure_complete=False),
                       "compact", 6)
        self.assertEqual((row["reclassified_outcome"], row["pressure_run_complete"], row["pressure_run_failure"]),
                         ("shared_usage_exceeds_control_margin", False,
                          "ballast shared usage increased during allocation or hold"))
        row = classify(*pair([self.LIVE]*3, pressure_complete=False), "compact", 12)
        self.assertEqual((row["reclassified_outcome"], row["reclassified_admitted"], row["capacity_rule"]["satisfied"]),
                         ("pressure_run_incomplete", False, True))

    def test_mismatched_outer_and_native_receipts_are_refused(self):
        def broken(change):
            outer, native = pair([self.LIVE]*3)
            change(outer, native)
            return outer, native
        cases = [lambda outer, native: outer.update(tier_gib=16),
                 lambda outer, native: outer["trial"].update(candidate_pid=8),
                 lambda outer, native: outer["admission"].update(candidate_pid=8),
                 lambda outer, native: outer["trial"].update(image={"sha256":"b"*64}),
                 lambda outer, native: outer["trial"].update(host_limit_bytes=16*GIB),
                 lambda outer, native: outer["trial"].update(model="large"),
                 lambda outer, native: native.update(mode="pressure-fixed")]
        for index, change in enumerate(cases):
            with self.subTest(index=index), self.assertRaisesRegex(ValueError, "receipts differ"):
                classify(*broken(change), "compact", 12)


if __name__ == "__main__":
    unittest.main()
