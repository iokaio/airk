# SPDX-License-Identifier: Apache-2.0
"""Independent controls for native Gather Context delivery normalization."""

import copy
import hashlib
from io import BytesIO
import json
from pathlib import Path
import unittest
from unittest.mock import Mock, patch

from gather_context_native import capture_case, capture_failure, verify_observation


SOURCE = b"first\r\ncaf\xc3\xa9\r\nlast\n"
ORIGINAL = "docs/owner.md"
CASE = {
    "id": "bookbuilder-C1", "repo": "bookbuilder", "source_commit": "a" * 40,
    "publication": "publication", "index_id": "index",
}
DIGEST = hashlib.sha256(SOURCE).hexdigest()


def span(pointer, start=1, end=3, **values):
    return {"path": ORIGINAL, "content_hash": DIGEST, "pointer": pointer,
            "start_line": start, "end_line": end, "truncated": False, **values}


def response(spans):
    return {
        "schema": "airk.arbiter.context/3",
        "scope": {"index_id": "index", "publication": {"token": "publication"}},
        "records": [{"id": "record", "operation": "read_file", "canonical_result": {
            "projection": "airk.arbiter.loop-evidence/1", "revision": "publication", "spans": spans,
        }}],
        "record_uses": [{"record": "record", "part": 1, "round": 1}],
        "work": {"calls": 1, "rounds": 1},
        "model_work": {"focus_calls": 0, "context_calls": 0, "focus_pairs": 0, "focus_profile": None},
    }


class NativeCaptureTests(unittest.TestCase):
    def capture(self, body, *, delivered=None, source_bytes=SOURCE):
        mocked_path = Mock()
        mocked_path.open.side_effect = lambda _: BytesIO(source_bytes)
        with patch("gather_context_native.reopen_exact", return_value=mocked_path) as opened:
            result = capture_case(CASE, body, json.dumps(body) if delivered is None else delivered,
                                  elapsed_ms=17, source_root=Path("export"))
            if body.get("records", [{}])[0].get("canonical_result", {}).get("spans"):
                opened.assert_called_once_with(Path("export"), ORIGINAL)
        return result

    def test_verifies_delivered_utf8_at_original_line_and_does_not_infer_actions(self):
        body = response([span("/hits/0", 2, 2, text="café\r\n")])
        row = self.capture(body)
        self.assertEqual(row["delivered_passages"], [{
            "path": ORIGINAL, "start_line": 2, "end_line": 2, "source_sha256": DIGEST,
            "excerpt_sha256": hashlib.sha256(b"caf\xc3\xa9\r\n").hexdigest(),
            "record": "record", "pointer": "/hits/0",
        }])
        self.assertEqual(row["actions"], [])
        self.assertFalse(row["action_trace_complete"])
        self.assertEqual(row["retained_record_uses"], body["record_uses"])
        self.assertEqual(row["costs"]["reported_native_work_calls"], 1)
        self.assertIsNone(row["costs"]["native_files_scanned"])
        self.assertEqual(row["costs"]["native_transport_elapsed_ms"], 17)

    def test_same_as_uses_the_referenced_delivered_carrier_and_original_ranges(self):
        link = {"record": "record", "pointer": "/hits/3"}
        rows = [
            span("/hits/0", 2, 2, same_as=link, within=True),
            span("/hits/1", 2, 2, text="café\n"),
            {**span("/hits/2", 2, 2, text="café\r\n"), "truncated": True},
            span("/hits/3", text=SOURCE.decode()),
            span("/hits/4", 1, 1, same_as=link),
        ]
        captured = self.capture(response(rows))
        self.assertEqual([row["pointer"] for row in captured["delivered_passages"]], ["/hits/0", "/hits/3"])
        self.assertEqual([row["reason"] for row in captured["unverified_passages"]],
                         ["source_bytes_unavailable_or_different", "truncated", "unverified_shared_carrier"])

    def test_mutated_export_cannot_supply_a_witness(self):
        captured = self.capture(response([span("/hits/0", text=SOURCE.decode())]), source_bytes=b"different")
        self.assertEqual(captured["delivered_passages"], [])
        self.assertEqual(len(captured["unverified_passages"]), 1)

    def test_rejects_mismatched_delivered_text_scope_revision_and_model_use(self):
        valid = response([])
        with self.assertRaisesRegex(ValueError, "delivered JSON text"):
            self.capture(valid, delivered='{"records":[],"schema":"airk.arbiter.context/3"}')
        for edit, reason in [
            (lambda b: b["scope"].update(index_id="other"), "index or publication"),
            (lambda b: b["records"][0]["canonical_result"].update(revision="stale"), "revision differs"),
            (lambda b: b["model_work"].update(focus_calls=1), "optional model work"),
            (lambda b: b.update(ticket={"id": "queued"}), "terminal native /3"),
        ]:
            with self.subTest(reason=reason), self.assertRaisesRegex(ValueError, reason):
                body = copy.deepcopy(valid)
                edit(body)
                self.capture(body)

    def test_terminal_deadline_and_explicit_cancelled_receipts_stay_in_the_roster(self):
        body = response([])
        body["partial"] = {"reason": "deadline", "stage": "retrieve"}
        row = self.capture(body)
        self.assertEqual(row["disposition"], "timeout")
        self.assertEqual(row["terminal_detail"], body["partial"])
        cancelled = capture_failure(CASE, "cancelled", {"code": "cancelled"}, elapsed_ms=5)
        self.assertEqual(cancelled["id"], CASE["id"])
        self.assertEqual(cancelled["disposition"], "cancelled")
        self.assertEqual(cancelled["delivered_passages"], [])
        with self.assertRaisesRegex(ValueError, "terminal failure"):
            capture_failure(CASE, "cancelled", {}, elapsed_ms=None)

    def test_settled_observation_and_actual_disabled_states_are_required(self):
        expected = {**CASE, "binding": "binding"}
        observed = {"repo": "bookbuilder", "binding": "binding", "index_id": "index",
                    "publication_token": "publication", "settled": True, "semantic_state": "ready",
                    "served_mode": "hybrid", "execution_incomplete": False}
        off = {"focus": None, "context": None}
        verify_observation(expected, observed, off)
        for update, reason in [
            ({"publication_token": "stale"}, "publication_token differs"),
            ({"binding": None}, "binding differs"),
            ({"served_mode": "lexical"}, "not settled"),
        ]:
            with self.subTest(update=update), self.assertRaisesRegex(ValueError, reason):
                verify_observation(expected, {**observed, **update}, off)
        for stages in (None, {}, {"focus": None}, {"focus": "active", "context": None}):
            with self.subTest(stages=stages), self.assertRaisesRegex(ValueError, "observed off"):
                verify_observation(expected, observed, stages)
