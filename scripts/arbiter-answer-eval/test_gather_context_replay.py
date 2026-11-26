# SPDX-License-Identifier: Apache-2.0
"""Independent controls for identity and evidence freezing."""
from pathlib import Path
import unittest
from unittest.mock import patch

import common
from gather_context_replay import case_obligations, read_ceiling


class ReferenceFreezeTests(unittest.TestCase):
    def setUp(self):
        self.root = Path.cwd()
        self.repository = {"commit": "a" * 40,
                           "publication": "airk-r1:" + "b" * 32 + ":1:" + "c" * 64,
                           "index_id": "idx_test", "binding": "binding-test"}
        self.spec = {"ceiling_disposition": {}, "scenario_freeze": {"sha256": "d" * 64},
                     "repositories": dict.fromkeys(("bookbuilder", "munarium"), self.repository)}
        self.freeze = dict.fromkeys(("bookbuilder_commit", "munarium_commit"), "a" * 40)
        self.history = {"schema": "airk.private.answer-eval-ceiling-disposition/1",
                        "status": "below-threshold", "scenario_freeze": self.spec["scenario_freeze"], "cases": []}
        self.settled = [{"settled": True, "phase": phase, "repo": repo,
                         "publication_token": self.repository["publication"], "index_id": "idx_test",
                         "binding": "binding-test"}
                        for repo in self.spec["repositories"] for phase in ("start", "end")]

    def test_refuses_stale_settlement_before_comparing_cases(self):
        self.settled[-1]["publication_token"] = self.repository["publication"].replace(":1:", ":2:")
        with patch("gather_context_replay.load_pinned", return_value=self.history), \
                patch("gather_context_replay.historical_inputs", return_value=({}, self.settled)):
            with self.assertRaisesRegex(common.HarnessError, "end settlement missing or stale"):
                read_ceiling(self.root, self.spec, self.freeze)

    def test_refuses_missing_cases_even_when_scopes_are_settled(self):
        with patch("gather_context_replay.load_pinned", return_value=self.history), \
                patch("gather_context_replay.historical_inputs", return_value=({}, self.settled)):
            with self.assertRaisesRegex(common.HarnessError, "incorrect case roster"):
                read_ceiling(self.root, self.spec, self.freeze)

    def test_refuses_disagreement_with_independent_reference_before_reopening_source(self):
        note = {"mark": "essential", "id": "proof", "path": "src/original.rs", "lines": [1, 2]}
        span = {"essential_ids": ["proof"], "path": "src/other.rs", "start_line": 1, "end_line": 2}
        with patch("gather_context_replay.source_span") as source_span:
            with self.assertRaisesRegex(common.HarnessError, "independent freeze"):
                case_obligations({"notes": [note]}, self.root, {"spans": [span]})
        source_span.assert_not_called()

    def test_refuses_reconstructed_range_with_different_excerpt_bytes(self):
        note = {"mark": "essential", "id": "proof", "path": "src/original.rs", "lines": [1, 2]}
        span = {"essential_ids": ["proof"], "path": note["path"], "start_line": 1, "end_line": 2,
                "source_sha256": "a" * 64, "excerpt_sha256": "b" * 64}
        with patch("gather_context_replay.source_span", return_value=({"excerpt_sha256": "c" * 64}, 2)):
            with self.assertRaisesRegex(common.HarnessError, "excerpt digest differs"):
                case_obligations({"notes": [note]}, self.root, {"spans": [span]})


if __name__ == "__main__":
    unittest.main()
