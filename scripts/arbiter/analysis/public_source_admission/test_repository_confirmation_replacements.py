# SPDX-License-Identifier: Apache-2.0
"""Focused fail-closed checks for the replacement metadata screen."""

import runpy
from pathlib import Path
import unittest

reasons = runpy.run_path(str(Path(__file__).with_name("screen_repository_confirmation_replacements.py")))["reasons"]


class ReplacementScreenTests(unittest.TestCase):
    def setUp(self):
        self.row = dict(complete_public_lineage=True, postmodel_issue_and_ordered_merge=True,
                        base_object_resolved=True, base_tied_to_pull_ref_or_first_merge_parent=True,
                        known_contextbench_overlap=False, original_roster_family_overlap=False,
                        within_tranche_family_overlap=False,
                        license_at_base=dict(status="exact_root_blob_matched", permissive_classifier=True))

    def test_later_snapshot_still_needs_independent_chronology_review(self):
        self.row["base_date_no_later_than_original_issue"] = False
        self.assertEqual(reasons(self.row), [])

    def test_missing_history_or_shared_fork_or_unknown_chronology_withheld(self):
        del self.row["known_contextbench_overlap"]
        self.row["within_tranche_family_overlap"] = True
        self.row["original_roster_family_overlap"] = True
        self.row["postmodel_issue_and_ordered_merge"] = None
        self.assertCountEqual(reasons(self.row), ["postmodel_issue_and_ordered_merge", "known_contextbench_overlap",
                                                  "original_roster_family_overlap", "within_tranche_family_overlap"])

    def test_license_requires_both_revision_match_and_classification(self):
        for license in (dict(status="base_root_blob_not_matched", permissive_classifier=True),
                        dict(status="exact_root_blob_matched", permissive_classifier=False)):
            with self.subTest(license=license):
                self.row["license_at_base"] = license
                self.assertEqual(reasons(self.row), ["pinned_permissive_root_license"])


if __name__ == "__main__":
    unittest.main()
