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
    def test_restart_identity_detects_revision_and_citation_changes_hidden_by_content_projection(self):
        import copy
        result = {"publication": {"token": "original-token"}, "generation": 7, "collection": "fixture",
                  "truncated": False, "hits": [{"citation": {"path": "a.rs", "location": {"Lines": {"start": 1, "end": 1}},
                      "content_hash": "original-source-hash"}, "text": "original fixture", "truncated": False}]}
        original = measurement.restart_identity(result)
        for field in ("token", "content_hash"):
            changed = copy.deepcopy(result)
            if field == "token":
                changed["publication"]["token"] = "different-token"
            else:
                changed["hits"][0]["citation"][field] = "different-source-hash"
            self.assertEqual(measurement.evidence(result), measurement.evidence(changed))
            self.assertNotEqual(original, measurement.restart_identity(changed))
        result.pop("publication")
        with self.assertRaisesRegex(AssertionError, "exact publication token"):
            measurement.restart_identity(result)

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

    def test_confirmed_exited_client_is_not_a_live_missing_observation(self):
        rows = {-1: {"parent": os.getpid(), "name": "airk", "rss": 100},
                -2: {"parent": os.getpid(), "name": "airk", "rss": None,
                     "exited_before_rss": True}}
        with tempfile.TemporaryDirectory() as directory:
            sampler = measurement.ResourceSampler(Path(directory))
            with patch.object(measurement, "process_snapshot", return_value=rows):
                sampler.sample()
            self.assertEqual(sampler.missing_rss, 0)
            self.assertEqual(sampler.peak_processes, 1)
            self.assertEqual(sampler.peak_rss, 100)


if __name__ == "__main__":
    unittest.main()
