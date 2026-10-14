# SPDX-License-Identifier: Apache-2.0
"""Exercise benchmark process accounting and child failure handling."""
import importlib.util
from pathlib import Path
import platform
import sys
import tempfile
import unittest

SPEC = importlib.util.spec_from_file_location("benchmark_scale", Path(__file__).parents[1] / "benchmark-scale.py")
BENCHMARK = importlib.util.module_from_spec(SPEC)
SPEC.loader.exec_module(BENCHMARK)


class ScaleMeasurementTests(unittest.TestCase):
    def test_child_resources_are_observed_without_a_usage_ledger(self):
        with tempfile.TemporaryDirectory() as directory:
            response, metrics = BENCHMARK.measured(
                [sys.executable, "-c", "import json,time; values=list(range(100000)); time.sleep(0.12); print(json.dumps({'sum':sum(values)}))"],
                Path(directory), "child")
            self.assertEqual(response, {"sum": 4_999_950_000})
            self.assertEqual(metrics["exit_code"], 0)
            self.assertGreater(metrics["wall_ms"], 0)
            if platform.system() in ("Windows", "Linux"):
                self.assertGreater(metrics["cpu_us"], 0)
                self.assertGreater(metrics["peak_rss_bytes"], 0)

    @unittest.skipUnless(platform.system() == "Linux", "Linux fork/exec RSS accounting")
    def test_launcher_memory_is_not_reported_as_executable_rss(self):
        # Keep the parent resident through launch so wait4 can expose its
        # inherited high-water mark even though exec discards this allocation.
        launcher_buffer = bytearray(128 * 1024 * 1024)
        try:
            with tempfile.TemporaryDirectory() as directory:
                _, metrics = BENCHMARK.measured(
                    [sys.executable, "-c", "import time; time.sleep(0.15); print('{}')"],
                    Path(directory), "small-child")
            self.assertGreater(metrics["rss_samples"], 0)
            self.assertLess(metrics["peak_rss_bytes"], len(launcher_buffer) // 2)
            self.assertGreater(metrics["child_lifetime_peak_rss_bytes"], metrics["peak_rss_bytes"])
        finally:
            del launcher_buffer

    def test_failed_child_cannot_be_reported_as_success(self):
        with tempfile.TemporaryDirectory() as directory:
            with self.assertRaisesRegex(RuntimeError, "deliberate fixture failure"):
                BENCHMARK.measured(
                    [sys.executable, "-c", "import sys; print('deliberate fixture failure', file=sys.stderr); sys.exit(7)"],
                    Path(directory), "failure")


if __name__ == "__main__":
    unittest.main()
