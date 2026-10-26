# SPDX-License-Identifier: Apache-2.0
"""Cutover measurements must isolate fixture ancestry and expose missing RSS."""
import importlib.util
import os
from pathlib import Path
import tempfile
import unittest
from unittest.mock import patch

SPEC = importlib.util.spec_from_file_location(
    "storage_cutover_measurement", Path(__file__).parents[1] / "benchmark-storage-cutover.py")
measurement = importlib.util.module_from_spec(SPEC)
SPEC.loader.exec_module(measurement)


class CutoverMeasurement(unittest.TestCase):
    def test_only_fixture_native_descendants_contribute_to_peak_rss(self):
        parent = os.getpid()
        rows = {parent: {"parent": 0, "name": "python", "rss": 9000},
                -1: {"parent": parent, "name": "airk.exe", "rss": 100},
                -2: {"parent": -1, "name": "airk-embed.exe", "rss": 200},
                -3: {"parent": parent, "name": "unrelated", "rss": 8000},
                -4: {"parent": 0, "name": "airk.exe", "rss": 7000}}
        with tempfile.TemporaryDirectory() as directory:
            data = Path(directory)
            (data / "current.airk").write_bytes(b"original fixture")
            sampler = measurement.ResourceSampler(data)
            with patch.object(measurement, "process_snapshot", return_value=rows):
                sampler.sample()
                rows[-1]["rss"] = 50
                sampler.sample()
            self.assertEqual(sampler.peak_rss, 300)
            self.assertEqual(sampler.peak_processes, 2)
            self.assertEqual(sampler.peak_disk, len(b"original fixture"))
            self.assertEqual(sampler.missing_rss, 0)

    def test_missing_child_rss_invalidates_measurement(self):
        rows = {-1: {"parent": os.getpid(), "name": "airk", "rss": None}}
        with tempfile.TemporaryDirectory() as directory:
            sampler = measurement.ResourceSampler(Path(directory))
            with patch.object(measurement, "process_snapshot", return_value=rows):
                sampler.sample()
            # finish normally joins a started thread; emulate its already-stopped state.
            sampler.thread.start()
            with self.assertRaisesRegex(RuntimeError, "Incomplete native process memory"):
                sampler.finish()


if __name__ == "__main__":
    unittest.main()
