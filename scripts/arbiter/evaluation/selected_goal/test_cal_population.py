# SPDX-License-Identifier: Apache-2.0
import hashlib
from pathlib import Path
import unittest

from replay_cal_population import check_partition, check_sources


class PopulationReplayTests(unittest.TestCase):
    def test_refuses_other_partitions_and_different_order(self):
        ids = [f"q{i}" for i in range(100)]
        scopes = {"experiment_partition": "CAL", "questions": [{"id": i, "split": "calibration"} for i in ids]}
        manifest = {"experiment_partition": "CAL", "goal_ids": [], "calibration_ids": ids}
        self.assertEqual([8] * 12 + [4], [len(p["questions"]) for p in check_partition(scopes, manifest)])
        for updates in ({"calibration_ids": ids[::-1]}, {"goal_ids": ["other"]}, {"experiment_partition": "DEV"}):
            with self.subTest(updates=list(updates)), self.assertRaises(ValueError):
                check_partition(scopes, manifest | updates)

    def test_refuses_source_escape_before_open(self):
        row = {"file": "../outside"}
        with self.assertRaisesRegex(ValueError, "outside CAL"):
            check_sources(Path.cwd(), [row], [row])

    def test_checks_raw_paragraph_slice(self):
        from unittest.mock import patch
        body = "é abc".encode()
        row = {"file": "sources/hotpot/doc.txt", "bytes": len(body), "sha256": hashlib.sha256(body).hexdigest()}
        paragraph = row | {"byte_start": 3, "byte_end": 6, "paragraph_sha256": hashlib.sha256(b"abc").hexdigest()}
        with patch("replay_cal_population.checked_bytes", return_value=body):
            self.assertEqual(check_sources(Path.cwd(), [row], [paragraph]), len(body))
            with self.assertRaisesRegex(ValueError, "paragraph"):
                check_sources(Path.cwd(), [row], [paragraph | {"byte_start": 2}])
