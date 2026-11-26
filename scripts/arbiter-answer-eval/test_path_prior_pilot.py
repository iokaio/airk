# SPDX-License-Identifier: Apache-2.0
"""Independent decision examples for the query-time path-prior gate."""

import copy
import sys
import unittest
from pathlib import Path

HERE = Path(__file__).resolve().parent
if str(HERE) not in sys.path:
    sys.path.insert(0, str(HERE))

import common
import path_prior_pilot as pilot


class Selection(unittest.TestCase):
    def pair(self):
        data = {"schema": pilot.SCHEMA, "identity": {"search_profile": pilot.CONTROL,
                "service_sha256": "same-service", "cli_sha256": "same-cli",
                "git_head": "same-source", "indexes": {"bookbuilder": "bb", "munarium": "mn"}},
                "route": "cpu", "reference_sha256": "closed", "cohort_sha256": "frozen",
                "freeze_sha256": "heldout", "rows": []}
        for repo in ("bookbuilder", "munarium"):
            for number in range(10):
                data["rows"].append({"repo": repo, "key": f"{repo}/{number}",
                                     "request_sha256": f"request-{repo}-{number}",
                                     "top10": ["N1"], "top40": ["N1"],
                                     "ordinary": [{"elapsed_ms": 100, "incomplete": True, "truncated": False}] * 3,
                                     "wide": {"hit_sha256": ["first", "second"],
                                              "incomplete": False, "truncated": False}})
        trial = copy.deepcopy(data)
        trial["identity"]["search_profile"] = pilot.TRIAL
        for repo_start in (0, 10):
            for offset in range(3):
                trial["rows"][repo_start + offset]["top40"].append("N2")
        return data, trial

    def test_six_gains_need_unchanged_top_ten_and_each_repository_cost(self):
        original, trial = self.pair()
        self.assertTrue(pilot.compare(original, trial)["qualifies"])
        trial["rows"][0]["top10"] = ["N2"]
        rejected = pilot.compare(original, trial)
        self.assertEqual(rejected["net_top40"], 6)
        self.assertFalse(rejected["qualifies"])
        self.assertEqual(rejected["repositories"]["bookbuilder"]["changes"]["top10_lost"],
                         [{"key": "bookbuilder/0", "id": "N1"}])
        trial["rows"][0]["top10"] = ["N1"]
        trial["rows"][10]["ordinary"] = [{"elapsed_ms": 111, "incomplete": True, "truncated": False}] * 3
        rejected = pilot.compare(original, trial)
        self.assertAlmostEqual(rejected["repositories"]["munarium"]["query_p95_ratio"], 1.11)
        self.assertFalse(rejected["qualifies"])

    def test_different_image_is_refused(self):
        original, trial = self.pair()
        trial["identity"]["service_sha256"] = "different-service"
        with self.assertRaisesRegex(common.HarnessError, "differ on more"):
            pilot.compare(original, trial)
