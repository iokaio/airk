# SPDX-License-Identifier: Apache-2.0
"""Check the exact stopped R6 scope and custody without dispatching a service."""
import copy
import unittest
from unittest.mock import patch

import r6_capture as capture
import r7_handoff as handoff


class RetiredScope(unittest.TestCase):
    @classmethod
    def setUpClass(cls):
        r5 = capture.r5
        cls.plan = r5.linked(r5.pin(*handoff.ORIGINALS["plan"]))
        cls.epoch = cls.plan["candidate_service"]
        cls.anchor = r5.linked(cls.plan["installation"]["bindings"])
        cls.retired = r5.linked(r5.pin(*handoff.ORIGINALS["bindings"]))
        cls.ready = r5.linked(r5.pin(*handoff.ORIGINALS["ready"]))
        comparison = r5.linked(cls.plan["physical_handoff"]["comparison"])
        cls.search = r5.linked(comparison["fixture"]["enable-context.bindings.json"])

    def test_original_scope_transition_preserves_the_publication_and_members(self):
        handoff.verify_scope(self.anchor, self.search, self.retired, self.ready)
        for field in ("index_id", "publication", "window_scope_selection"):
            changed = copy.deepcopy(self.retired)
            changed["bindings"][0][field] = None
            with self.subTest(field=field), self.assertRaises(ValueError):
                handoff.verify_scope(self.anchor, self.search, changed, self.ready)
        stale = copy.deepcopy(self.ready)
        stale["health"]["catalog_clock"]["revision"] = 4
        with self.assertRaises(ValueError):
            handoff.verify_scope(self.anchor, self.search, self.retired, stale)

    def test_exact_retirement_is_reopened_before_control_custody(self):
        r5 = capture.r5
        link = r5.link
        future = handoff.R7 / "predecessor-control.json"
        with patch.object(r5, "link", side_effect=lambda path: {"path": str(future)} if path == future else link(path)):
            proof = handoff.evidence(self.plan, self.epoch, r5)
        self.assertEqual(proof["bindings"]["sha256"], handoff.ORIGINALS["bindings"][1])
        self.assertEqual(proof["control"]["path"], str(future))
        self.assertEqual(set(proof) - {"control"}, set(handoff.ORIGINALS))
