# SPDX-License-Identifier: Apache-2.0
"""Synthetic controls for the private development capture verifier."""

from copy import deepcopy
import io
import json
from pathlib import Path
import sys
import unittest
from unittest.mock import patch

sys.path.insert(0, str(Path(__file__).resolve().parent))
import verify_card_capture as verify

OWNER = "S-1-5-21-11-22-33-1002"
PROFILE, COMPILED, INPUT, ROSTER = (letter * 64 for letter in "abcd")
TASK = "airk.context_coverage.v1"
SCOPE = {
    "catalog": "test-catalog",
    "index_id": "example-index",
    "configuration_revision": 2,
    "publication": {"revision": "synthetic-revision"},
    "installation": "e" * 64,
    "authorization": "f" * 64,
}


def encoded(value):
    return json.dumps(value, separators=(",", ":")).encode() + b"\n"


def fixture():
    evaluation = {"evaluation": True, "release_qualified": False}
    identity = {"control_root": "private-control"}
    ready = dict(evaluation, **identity, schema="airk.private-arbiter-fixture/1", profiles=[{"manifest": PROFILE}],
                 health={"user": OWNER, "process_id": 111, "catalog_clock": {"namespace": "test-catalog"},
                         "instance": "native-instance", "executable_sha256": "1" * 64})
    closed = dict(evaluation, **identity, schema="airk.private-arbiter-fixture-closed/1", instance="native-instance",
                  shutdown_completed=True, service_lease_reacquired=True)
    journal_work = {"input_digest": COMPILED, "input_tokens": 10, "queue_us": 1, "compile_us": 2, "inference_us": 3}
    journal_binding = {
        "schema": "airk.private.native-dispatch/1", "unix_ms": 20, "observer_id": "test-observer",
        "dispatch_identity": 1, "profile_manifest": PROFILE, "task": TASK, "input_digest": COMPILED,
        "question_utf8_sha256": "2" * 64, "original_deadline_unix_ms": 999, "tokens": 10,
        "charge_identity": "test-observer:1", "charge_bytes": 100, "scope": SCOPE,
    }
    events = [
        dict(journal_binding, kind="dispatch_admitted", sequence=1),
        dict(journal_binding, kind="score_returned", sequence=2, return_observed=True,
             outcome="success", raw_scores=[1.0, -0.0], raw_score_bits=[0x3f800000, 0x80000000],
             finite_raw_scores=True, work=journal_work),
        dict(journal_binding, kind="job_settled", sequence=3, return_observed=True,
             proof="actual_job_compute_delivery_and_per_job_resident_permit_dropped"),
        dict(journal_binding, kind="profile_released", sequence=4,
             proof="actual_final_reservation_drop_after_native_scorer_and_source_teardown"),
        dict(journal_binding, kind="charge_released", sequence=5,
             proof="actual_final_reservation_drop_after_native_scorer_and_source_teardown"),
    ]
    row = {
        "schema": "airk.private.development-card-call/1", "decision_mode": "top_score",
        "request_id": "example-attempt", "max_sub_needs": 4, "roster_digests": [ROSTER],
        "segments": [{"ordinal": 5, "card_slot": 1, "stage": "coverage",
                      "cards": [{"symbol": "A", "text": "private"}, {"symbol": "B", "text": "private"}]}],
        "raw_score_bits": [[0x3f800000, 0x80000000]],
        "advice": {"schema": "airk.arbiter.advice/3", "profile": PROFILE, "task": TASK, "scope": SCOPE,
                   "card_slot": 1, "input_digest": INPUT, "observed_input_tokens": 10,
                   "work": {key: value for key, value in journal_work.items() if key != "input_digest"},
                   "segments": [{"ordinal": 5, "raw_scores": [1.0, -0.0],
                                 "outcome": {"status": "accepted", "decision": {"kind": "loop_card", "value": ROSTER}}}]},
        "score_evidence": {"compiled_input_digest": COMPILED, "scored_input_digest": COMPILED},
    }
    refusal = deepcopy(row)
    refusal["segments"][0]["card_slot"] = refusal["advice"]["card_slot"] = 2
    refusal["raw_score_bits"] = [[]]
    refusal["advice"]["segments"][0] = {
        "ordinal": 5, "raw_scores": [],
        "outcome": {"status": "unavailable", "reason": "The native lane refused to queue this card call."},
    }
    refusal["advice"]["observed_input_tokens"] = 0
    refusal["score_evidence"] = {"compiled_input_digest": None, "scored_input_digest": None}
    return ready, closed, events, [row, refusal]


def package(ready, closed, events, rows):
    capture, journal = (b"".join(map(encoded, values)) for values in (rows, events))
    common = {"complete": True, "evaluation": True, "release_qualified": False, "dropped": 0}
    return {
        "capture": capture, "journal": journal, "ready": encoded(ready), "closed": encoded(closed),
        "closure": encoded(dict(common, schema="airk.private.development-card-capture-closure/1",
                                service_completed=True, writer_failed=False, records=len(rows),
                                bytes=len(capture), sha256=verify.digest(capture))),
        "journal-closure": encoded(dict(common, schema="airk.private.native-dispatch-recorder/1", error=None,
                                        writer_failed=0, records=len(events), bytes=len(journal),
                                        last_sequence=len(events))),
    }


class CardCaptureVerification(unittest.TestCase):
    def test_verified_counts_keep_refusal_identity_and_hide_private_payloads(self):
        report = verify.verify(package(*fixture()), OWNER)
        self.assertEqual(report["records"], 2)
        self.assertEqual(report["tasks"][TASK], {
            "calls": 2, "segments": 2, "mode_top_score": 2, "scored_witnessed": 1,
            "without_score": 1, "outcome_accepted": 1, "outcome_unavailable": 1,
            "refusal_queue_refused": 1,
        })
        self.assertEqual(report["profiles"][PROFILE], report["tasks"][TASK])
        self.assertEqual(report["native_score_journal_counts"], {"card_success": 1})
        self.assertEqual(report["unmatched_native_card_successes"], 0)
        for secret in ("example-attempt", "example-index", "private-control", OWNER):
            self.assertNotIn(secret, json.dumps(report))

    def test_missing_reacquisition_foreign_owner_and_loss_refuse(self):
        data = package(*fixture())
        with self.assertRaisesRegex(verify.EvidenceError, "owner differs"):
            verify.verify(data, "S-1-5-21-1003")
        for name, field, value in (
            ("closed", "service_lease_reacquired", False), ("closed", "instance", "other-instance"),
            ("closure", "dropped", 1), ("closure", "sha256", "0" * 64),
            ("journal-closure", "writer_failed", 1), ("journal-closure", "last_sequence", 8),
        ):
            with self.subTest(name=name, field=field), self.assertRaises(verify.EvidenceError):
                changed = {**data, name: encoded({**json.loads(data[name]), field: value})}
                verify.verify(changed, OWNER)

    def test_exact_join_refuses_wrong_bits_digest_scope_or_dispatch_custody(self):
        for mutation in ("bits", "digest", "scope", "release", "ordinal"):
            with self.subTest(mutation=mutation), self.assertRaises(verify.EvidenceError):
                ready, closed, events, rows = fixture()
                row = rows[0]
                if mutation == "bits":
                    row["raw_score_bits"][0][1] = 0
                    row["advice"]["segments"][0]["raw_scores"][1] = 0.0
                elif mutation == "digest":
                    row["score_evidence"] = dict(compiled_input_digest=INPUT, scored_input_digest=INPUT)
                elif mutation == "scope":
                    row["advice"]["scope"] = {**SCOPE, "configuration_revision": 3}
                elif mutation == "release":
                    events.pop()
                else:
                    row["advice"]["segments"][0]["ordinal"] = 6
                verify.verify(package(ready, closed, events, rows), OWNER)

    def test_one_native_success_cannot_be_reused_or_omit_score_provenance(self):
        for mutation in ("duplicate", "reuse", "missing-digest"):
            with self.subTest(mutation=mutation), self.assertRaises(verify.EvidenceError):
                ready, closed, events, rows = fixture()
                if mutation == "missing-digest":
                    rows[0]["score_evidence"]["scored_input_digest"] = None
                else:
                    repeated = deepcopy(rows[0])
                    if mutation == "reuse":
                        repeated["request_id"] = "other-attempt"
                    rows.append(repeated)
                verify.verify(package(ready, closed, events, rows), OWNER)

    def test_compiled_dispatch_can_witness_unobserved_compiler_callback(self):
        ready, closed, events, rows = fixture()
        rows[0]["score_evidence"]["compiled_input_digest"] = None
        summary = verify.verify(package(ready, closed, events, rows), OWNER)
        self.assertEqual(summary["tasks"][TASK]["compiled_callback_unobserved"], 1)

    def test_external_pin_refuses_changed_bytes(self):
        content = b"pinned synthetic"
        with (patch.object(Path, "resolve", lambda path, **_: path),
              patch.object(Path, "is_file", return_value=True),
              patch.object(Path, "open", side_effect=lambda *_: io.BytesIO(content))):
            self.assertEqual(verify.read_pinned(Path("synthetic"), "evidence", 64, verify.digest(content)), content)
            with self.assertRaisesRegex(verify.EvidenceError, "SHA-256 differs"):
                verify.read_pinned(Path("synthetic"), "evidence", 64, "0" * 64)


if __name__ == "__main__":
    unittest.main()
