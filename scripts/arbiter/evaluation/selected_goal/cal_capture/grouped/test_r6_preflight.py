# SPDX-License-Identifier: Apache-2.0
"""Protect the historical default and the prospective common-clock decision."""
import hashlib
import json
from pathlib import Path
import unittest
from unittest.mock import Mock, patch

import r6_preflight as r6


class CommonClock(unittest.TestCase):
    def test_default_and_setup(self):
        self.assertEqual(r6.selected_clock(None), 10_800_000)
        self.assertEqual(r6.common_deadline(1_000_000, 10_800_000, 545_806), 11_254_194)
        self.assertEqual(r6.common_deadline(1_000_000, 11_700_000, 545_806), 12_154_194)

    def test_rejects_other_limits_and_setup(self):
        for limit, debit in [(10_799_999, 545_806), (11_700_001, 545_806), (11_700_000, 0), (True, 545_806)]:
            with self.subTest(limit=limit, debit=debit), self.assertRaises(ValueError):
                r6.common_deadline(100, limit, debit)

    def test_unsanctioned_document_never_read(self):
        path = Mock(spec=Path)
        with patch.object(r6, "APPROVED_R6_DECISION_SHA256", None), self.assertRaises(ValueError):
            r6.selected_clock(path)
        path.read_bytes.assert_not_called()

    def test_only_exact_reviewed_attempt_and_scope_can_select_extended_limit(self):
        row = {"attempt": "R6", "common_ms": 11_700_000, "predecessor_plan": r6.PLAN,
               "scope": "whole-attempt-clock-only"}
        def encoded(value):
            return json.dumps(value, sort_keys=True).encode()
        raw = encoded(row)
        path = Mock(spec=Path)
        path.read_bytes.return_value = raw
        with patch.object(r6, "APPROVED_R6_DECISION_SHA256", hashlib.sha256(raw).hexdigest()):
            self.assertEqual(r6.selected_clock(path), 11_700_000)
            path.read_bytes.return_value = raw + b" "
            with self.assertRaises(ValueError):
                r6.selected_clock(path)
        for bad in [dict(row, attempt="R5"), dict(row, scope="all-clocks"), dict(row, common_ms=12_600_000)]:
            raw = encoded(bad)
            path.read_bytes.return_value = raw
            with patch.object(r6, "APPROVED_R6_DECISION_SHA256", hashlib.sha256(raw).hexdigest()), self.assertRaises(ValueError):
                r6.selected_clock(path)
