# SPDX-License-Identifier: Apache-2.0
"""Source job identity regressions for mutable evaluation lifetimes."""
import unittest

from workspace_fixture_lifecycle import change_proof


class WritableScopeJobs(unittest.TestCase):
    def setUp(self):
        self.frozen = ("original", "frozen-job", "vectors")
        self.owned = ("allowed", "old-owned-job", "index")
        self.new = ("allowed", "new-owned-job", "vectors")
        self.before = {self.frozen, self.owned}

    def test_only_allowlisted_new_jobs_are_accepted(self):
        receipt = change_proof(self.before, self.before | {self.new}, {"allowed"})
        self.assertEqual(receipt["new_workspace_jobs"], [self.new])
        with self.assertRaisesRegex(ValueError, "non-workspace"):
            change_proof(self.before, self.before | {("another", "new", "index")}, {"allowed"})

    def test_missing_receipts_need_the_exact_native_retirement(self):
        with self.assertRaisesRegex(ValueError, "unretired"):
            change_proof(self.before, {self.frozen}, {"allowed"})
        result = change_proof(self.before, {self.frozen}, {"allowed"}, {"allowed"})
        self.assertEqual(result["receipts_removed_by_native_retirement"], [self.owned])
        with self.assertRaisesRegex(ValueError, "unretired"):
            change_proof(self.before, set(), {"allowed"}, {"allowed"})
