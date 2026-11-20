# SPDX-License-Identifier: Apache-2.0
"""Check which native identities must survive the existing-installation restart."""
import copy
import unittest

from capture import make_plan
from lifetime import binding_identity


class PhaseContracts(unittest.TestCase):
    def setUp(self):
        self.binding = {"bindings":[{"index_id":"index", "publication":"original", "policy":{"kind":"exact"},
                                   "status_result":{"structuredContent":{"files":753, "indexed_vectors":767,
                                       "publication":{"token":"original"}, "semantic_readiness":{"state":"ready"},
                                       "read_scope":{"source":"named"}, "scope_observation":{
                                           "fingerprint":"1"*64, "members":[{"source_root_id":"root"}]}}}}]}

    def test_native_incarnation_fingerprint_can_change(self):
        new = copy.deepcopy(self.binding)
        new["bindings"][0]["status_result"]["structuredContent"]["scope_observation"]["fingerprint"] = "2"*64
        self.assertEqual(binding_identity(new), binding_identity(self.binding))

    def test_native_publication_and_membership_cannot_change(self):
        for item in ("publication","source_root_id"):
            with self.subTest(item=item):
                changed = copy.deepcopy(self.binding)
                status = changed["bindings"][0]["status_result"]["structuredContent"]
                if item == "publication":
                    status["publication"]["token"] = "another"
                else:
                    status["scope_observation"]["members"][0][item] = "other"
                self.assertNotEqual(binding_identity(changed), binding_identity(self.binding))

    def test_private_anchor_debits_full_setup_and_predeclares_original_census(self):
        plan = make_plan(123456)
        self.assertEqual(plan["setup_elapsed_ms"], 545806)
        self.assertEqual(plan["deadline_ms"] - plan["start_ms"] + plan["setup_elapsed_ms"], 10800000)
        self.assertEqual(list(map(len, plan["phase_ids"])), [8]*12 + [4])
        self.assertEqual(len({q for phase in plan["phase_ids"] for q in phase}),100)


if __name__ == "__main__":
    unittest.main()
