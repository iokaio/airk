# SPDX-License-Identifier: Apache-2.0
"""Model-free checks of the prospective D-only CUDA build boundary."""
import datetime as dt
import importlib.util
from pathlib import Path
import unittest
from unittest.mock import patch

PATH = Path(__file__).resolve().parents[1] / "cuda-static-runtime/build/fresh_windows_d.py"
SPEC = importlib.util.spec_from_file_location("cu18", PATH)
d = importlib.util.module_from_spec(SPEC)
SPEC.loader.exec_module(d)


class Limits(unittest.TestCase):
    def test_campaign_cutoff(self):
        start = d.END - dt.timedelta(minutes=1)
        self.assertEqual(d.cutoff(start, start), d.END)
        with self.assertRaises(ValueError):
            d.cutoff(d.END, start)

    def test_wall_is_clipped_even_when_campaign_has_more_time(self):
        start = d.END - dt.timedelta(hours=8)
        self.assertEqual(d.cutoff(start, start), start + dt.timedelta(hours=3))
        with self.assertRaises(ValueError):
            d.cutoff(start + dt.timedelta(seconds=10800), start)

    def test_volume_reserves_other_owners(self):
        free = d.VOLUME_BASE - 90 * d.GIB
        self.assertEqual(d.disk_admission(free, 10*d.GIB, 10*d.GIB), 90*d.GIB)
        with self.assertRaises(ValueError):
            d.disk_admission(free, 10*d.GIB, 20*d.GIB)

    def test_sibling_and_parent_refused(self):
        for root in (d.OUTPUT.parent, d.OUTPUT.with_name("cu19"), d.OUTPUT / "..", Path("cu18")):
            with self.subTest(root=root), self.assertRaises(ValueError):
                d.exact_output(root)

    def test_archive_input_must_be_original_leaf(self):
        with self.assertRaises(ValueError):
            d.verify_receipt({"path":str(d.OUTPUT.with_name("cu19") / "fresh-aggregate.json")}, 1)

    def test_original_native_source_and_clock_stay_unmodified(self):
        legacy = d.implementation("fresh_compile.py")
        _, prospective, _, _ = d.owners()
        self.assertEqual(legacy.CAMPAIGN_END, dt.datetime(2026,11,19,5,29,42,tzinfo=dt.timezone.utc))
        self.assertEqual(prospective.CAMPAIGN_END, d.END)
        self.assertNotEqual(legacy.ANCHOR, prospective.ANCHOR)
        self.assertEqual(legacy.build_commands(Path("root"), "cmake", {"parallel_workers":8}),
                         prospective.build_commands(Path("root"), "cmake", {"parallel_workers":8}))

    def test_physical_core_exclusion(self):
        import runpy
        supervisor = runpy.run_path(str(PATH.with_name("fresh_windows_d_job.py")))
        cores = [3 << (2*i) for i in range(32)]
        caches = [255 << (8*i) for i in range(8)]
        result = supervisor["choose_cores"](cores, caches, [0]*64, 1)
        self.assertEqual(result["excluded_siblings"], 3)
        self.assertEqual(result["excluded_l3_domains"], 255)
        self.assertEqual(result["logical_cpus"], list(range(8,24,2)))
        self.assertEqual(len(set(result["physical_siblings"])), 8)
        with self.assertRaises(ValueError):
            supervisor["choose_cores"](cores,caches,[0]*64,(1<<64)-1)

    def test_future_or_unrelated_review_refused(self):
        with self.assertRaises(ValueError):
            d.current_recipe("0"*40, "2026-11-19T12:00:00+00:00")

    def test_unexpected_recipe_refused(self):
        import json
        body = {"fresh_output":str(d.OUTPUT),"head":"0"*40,"reviewed_utc":"2026-11-20T01:00:00+00:00"}
        with patch.object(d, "read", return_value=json.dumps(body).encode()), patch.object(d, "current_recipe", return_value={}):
            with self.assertRaises(ValueError):
                d.check_recipe(Path("example"), "0"*64)


if __name__ == "__main__":
    unittest.main()
