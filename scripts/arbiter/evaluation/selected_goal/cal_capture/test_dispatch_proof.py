# SPDX-License-Identifier: Apache-2.0
"""Check full-lifetime native evidence and detect dropped or crossed records."""

import json
import unittest

from dispatch_proof import check_journal


class JournalTests(unittest.TestCase):
    def setUp(self):
        base = dict(schema="airk.private.native-dispatch/1", dispatch_identity=1, observer_id="fixture",
                    profile_manifest="focus", scope="fixed", task="airk.focus_rank.v1", input_digest="input",
                    tokens=32, question_utf8_sha256="question", original_deadline_unix_ms=10,
                    charge_identity="charge", charge_bytes=128)
        release = "actual_final_reservation_drop_after_native_scorer_and_source_teardown"
        self.rows = [dict(base, kind=kind, sequence=i + 1, **fields) for i, (kind, fields) in enumerate([
            ("dispatch_admitted", {}), ("score_returned", dict(outcome="success", error=None, finite_raw_scores=True)),
            ("job_settled", dict(return_observed=True)), ("profile_released", dict(proof=release)),
            ("charge_released", dict(proof=release))])]

    def verify(self, **summary_fields):
        raw = b"".join(json.dumps(row).encode() + b"\n" for row in self.rows)
        summary = dict(schema="airk.private.native-dispatch-recorder/1", complete=True, evaluation=True,
                       release_qualified=False, error=None, dropped=0, writer_failed=0, bytes=len(raw),
                       last_sequence=len(self.rows), records=len(self.rows))
        summary.update(summary_fields)
        return check_journal(raw, summary, "focus", setup_only=False)

    def test_one_focus_and_actual_release(self):
        self.assertEqual(self.verify()["successful_focus_returns"], 1)

    def test_missing_final_charge(self):
        self.rows.pop()
        with self.assertRaisesRegex(ValueError, "unsettled"):
            self.verify()

    def test_one_context_crossing(self):
        self.rows[1]["task"] = "airk.context_action.v1"
        with self.assertRaisesRegex(ValueError, "unexpected Context"):
            self.verify()

    def test_reported_drop(self):
        with self.assertRaisesRegex(ValueError, "dropped"):
            self.verify(dropped=1)

    def test_missing_return_observation(self):
        self.rows[2]["return_observed"] = False
        with self.assertRaisesRegex(ValueError, "not observed"):
            self.verify()


if __name__ == "__main__":
    unittest.main()
