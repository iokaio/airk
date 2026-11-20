# SPDX-License-Identifier: Apache-2.0
"""Controls for score reuse across two authority-scoped CAL captures."""

from copy import deepcopy
from pathlib import Path
import sys
import unittest
from unittest.mock import patch

sys.path.insert(0, str(Path(__file__).resolve().parent))
import r8_reuse as reuse


def row(scope):
    evidence = {"record": "record-" + scope, "kind": "hit", "result_pointer": "/hits/0",
                "utf8_start": 0, "utf8_end": 4}
    cards = [
        {"id": "read-" + scope, "action": "read", "assessments": [
            {"role": "supports", "relevance": "direct", "evidence": evidence}]},
        {"id": "stop-" + scope, "action": "stop", "assessments": []},
    ]
    roster = {"scope": scope, "input_digest": "digest-" + scope,
              "schema": "roster/1", "policy": "unchanged",
              "basis": {"source_units_presented": [evidence]}, "cards": cards}
    request = {
        "roster": roster, "question": "What changed?",
        "records": [{"id": evidence["record"], "operation": "search", "role": "primary", "arguments": {"q": "a"},
                     "canonical_result": {"hits": [{"text": "same", "citation": {"path": "source.rs"}}]}}],
    }
    capture = {"request": request, "Context_NN_dispatch": False}
    projection = {"id": "a", "ordinal": 0, "status": "eligible", "captures": [scope],
                  "roster": roster, "compiled": {"input_ids": [3, 4], "candidate_positions": [0, 1]}}
    return projection, capture


class ReuseTest(unittest.TestCase):
    def test_per_capture_ids_resolve_to_the_same_inputs(self):
        before, left = row("before")
        after, right = row("after")
        with patch.object(reuse, "read", side_effect={"before": left, "after": right}.__getitem__):
            reuse.verify_inputs(before, after)

    def test_rejects_reordered_slots_before_reading_captures(self):
        before, _ = row("before")
        after, _ = row("after")
        after["compiled"]["candidate_positions"].reverse()
        with patch.object(reuse, "read") as reader:
            with self.assertRaisesRegex(ValueError, "scoring tensors, slot positions"):
                reuse.verify_inputs(before, after)
            reader.assert_not_called()

    def test_rejects_same_tensors_if_cited_source_content_changed(self):
        before, left = row("before")
        after, right = row("after")
        changed = deepcopy(right)
        changed["request"]["records"][0]["canonical_result"]["hits"][0]["citation"]["path"] = "other.rs"
        with patch.object(reuse, "read", side_effect={"before": left, "after": changed}.__getitem__):
            with self.assertRaisesRegex(ValueError, "source evidence differs"):
                reuse.verify_inputs(before, after)


if __name__ == "__main__":
    unittest.main()
