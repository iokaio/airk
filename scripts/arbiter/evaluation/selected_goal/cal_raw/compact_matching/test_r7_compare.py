# SPDX-License-Identifier: Apache-2.0
"""Model-free checks for joining protected Compact GPU and reference vectors."""
from pathlib import Path
import sys
import unittest
from unittest.mock import patch

sys.path.insert(0, str(Path(__file__).resolve().parent))
import r7_compare as comparison


class ReferenceSourceCompatibility(unittest.TestCase):
    def test_old_live_runner_still_requires_the_identical_scoring_sources(self):
        directory = Path(comparison.__file__).resolve().parent
        previous = {str(directory / name): pin for name, pin in comparison.LIVE_SOURCE_PINS.items()}
        scoring = str(directory / "science.py")
        current = {path: "new implementation" for path in previous} | {scoring: "unchanged scoring"}
        historical = previous | {scoring: current[scoring]}
        self.assertTrue(comparison.compatible_sources(current, current))
        self.assertTrue(comparison.compatible_sources(historical, current))
        self.assertFalse(comparison.compatible_sources(historical | {scoring: "other scoring"}, current))
        self.assertFalse(comparison.compatible_sources(previous, current))


class ProtectedGpuJoin(unittest.TestCase):
    def setUp(self):
        self.proof = {"model_sha256": "m", "weight_sha256": "w", "typed_graph_sha256": "t",
                      "launch_audit_sha256": "a"}
        self.graph = {"image": {"sha256": "i"}, "build": {"sha256": "b"}, "runtime_manifest": {"sha256": "r"}}
        self.model = {"path": "compact.json", "sha256": "c"}
        self.rows = [{"ordinal": i, "id": f"input-{i}", "status": "eligible", "compiled": {"input_digest": f"d{i}"},
                      "captures": [], "roster": {"cards": ["first", "second"]}, "remaining_ms": 100 + i}
                     for i in range(8)]
        native_rows = [row | {"status": "raw_scores_only", "input_digest": row["compiled"]["input_digest"],
                              "scores": [float(row["ordinal"]), 0.5]} for row in self.rows]
        cal = {"rows": native_rows, "phase": 0, "original_projection": self.model,
               "launch_audit": {"sha256": "a"}, "calibration_fitted": False, "qualification": False}
        self.receipt = self.graph | {"mode": "cal", "complete": True, "owned": {},
                                     "result": self.proof | {"cal_native_forwards": 8, "cal": cal}}

    def vectors(self, receipt):
        return comparison.native_vectors(receipt, self.rows[2:4], 0, self.proof, self.graph, self.model)

    def test_preserves_exact_ordinals_and_vectors(self):
        with patch.object(comparison.parent, "settled", return_value=True):
            self.assertEqual(self.vectors(self.receipt), [[2.0, 0.5], [3.0, 0.5]])

    def test_changed_roster_digest_or_vector_extent_refuses(self):
        for change in ({"roster": {"cards": ["second", "first"]}}, {"input_digest": "other"}, {"scores": [2.0]}):
            with self.subTest(change=change), patch.object(comparison.parent, "settled", return_value=True):
                rows = list(self.receipt["result"]["cal"]["rows"])
                rows[2] = rows[2] | change
                result = self.receipt["result"] | {"cal": self.receipt["result"]["cal"] | {"rows": rows}}
                with self.assertRaisesRegex(ValueError, "original input or card roster"):
                    self.vectors(self.receipt | {"result": result})

    def test_unsettled_gpu_job_refuses(self):
        with patch.object(comparison.parent, "settled", return_value=False):
            with self.assertRaisesRegex(ValueError, "exact graph, phase or original inputs"):
                self.vectors(self.receipt)


if __name__ == "__main__":
    unittest.main()
