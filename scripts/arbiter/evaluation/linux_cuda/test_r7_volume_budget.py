# SPDX-License-Identifier: Apache-2.0
"""Exercise the independent C:, guest and VHD admissions for the signed GNU gate."""
from datetime import datetime, timezone
import unittest

from r7_volume_budget import check_monitor, revised_capacity


class R7VolumeBudget(unittest.TestCase):
    def setUp(self):
        self.sample = {"guest_free": 953_439_313_920, "backing_free": 2_643_244_216_320,
                       "vhd_length": 79_086_747_648}
        self.baseline = {"guest_free": 967_378_755_584}
        self.monitor = {"utc": "2026-11-20T14:05:00+00:00", "backing_start": 2_669_960_904_704,
                        "vhd_start": 60_649_635_840, "backing_budget": 120_000_000_000,
                        "backing_free": 2_643_244_216_320,
                        "previous_backing_budget": 25_769_803_776, "free_floor": 107_374_182_400,
                        "original_alert_log_sha256": "9cfbf2ed1877be97f859d5a5a696a3aa9aa90dcb06989f477c642e75fa452cda",
                        "within_budget": True, "has_headroom": True, "backing_writes_held": False}
        self.now = datetime(2026, 11, 20, 14, 5, 30, tzinfo=timezone.utc).timestamp()

    def admits(self, sample, *, initial=None, admitting=True):
        return revised_capacity(self.baseline, sample, self.monitor, initial or sample,
                                536_870_912, admitting=admitting)

    def test_original_guest_and_vhd_fit_revised_c_allowance(self):
        check_monitor(self.monitor, self.now)
        self.assertTrue(self.admits(self.sample))
        host_growth = self.monitor["backing_start"] - self.sample["backing_free"]
        self.assertGreater(host_growth, 25_769_803_776)

    def test_separate_guest_and_vhd_growth_refuse(self):
        self.assertFalse(self.admits(self.sample | {"guest_free": 942_000_000_000}))
        self.assertFalse(self.admits(self.sample | {"vhd_length": 86_000_000_000}))

    def test_swap_reservation_and_both_c_observations_apply(self):
        low = 2_560_000_000_000
        self.assertFalse(self.admits(self.sample | {"backing_free": low}))
        self.monitor["backing_free"] = low
        self.assertFalse(self.admits(self.sample))

    def test_active_run_cannot_exceed_its_storage_reservation(self):
        grown = self.sample | {"guest_free": self.sample["guest_free"] - 536_870_913}
        self.assertFalse(self.admits(grown, initial=self.sample, admitting=False))
        grown = self.sample | {"vhd_length": self.sample["vhd_length"] + 536_870_913}
        self.assertFalse(self.admits(grown, initial=self.sample, admitting=False))

    def test_stale_held_or_changed_monitor_refuses(self):
        with self.assertRaises(ValueError):
            check_monitor(self.monitor, self.now + 16)
        for changed in ({"backing_writes_held": True}, {"within_budget": False},
                        {"previous_backing_budget": 120_000_000_000}):
            with self.subTest(changed=changed), self.assertRaises(ValueError):
                check_monitor(self.monitor | changed, self.now)


if __name__ == "__main__":
    unittest.main()
