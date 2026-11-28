# SPDX-License-Identifier: Apache-2.0
"""Joint-evidence protection and attribution tests."""
import unittest
from types import SimpleNamespace

from finalist_joint_evidence import Evidence, attach


def record(need, score, parent, scored):
    return {"need": need, "score": score, "returned_at": parent, "scored_at": scored}


class JointEvidenceTests(unittest.TestCase):
    def setUp(self):
        self.repo = SimpleNamespace(texts={"a": ["a", "b", "c", "d"], "b": ["é"]})
        self.seed = [Evidence("a:1-2", "a\nb\n")]

    def apply(self, records, frame=lambda p: (b"wire", p), limit=100, seed=None):
        return attach(self.repo, self.seed if seed is None else seed, records, 2, frame, passage_limit=limit)

    def test_independent_needs_can_share_one_excerpt(self):
        entries = [record(n, 0.6, "a:1-4", "a:1-1") for n in (1, 2)]
        _, selected, kept = self.apply(entries)
        self.assertEqual([p.at for p in selected], ["a:1-2"])
        self.assertEqual([s["need"] for s in selected[0].supports], [1, 2])
        self.assertEqual([s["kind"] for s in kept], ["existing", "existing"])

    def test_renderer_dropping_core_for_metadata_cannot_change_result(self):
        def render(p):
            return b"wire", p[:-1] if any(x.supports for x in p) else p
        _, selected, kept = self.apply([record(1, 0.95, "a:1-4", "a:1-1")], render)
        self.assertEqual(selected, self.seed)
        self.assertFalse(kept)

    def test_no_low_score_or_partial_overlap_links(self):
        entries = [record(1, 0.5, "a:1-2", "a:1-1"), record(2, 0.99, "a:2-4", "a:2-3")]
        _, selected, kept = self.apply(entries)
        self.assertEqual(selected, self.seed)
        self.assertFalse(kept)

    def test_exact_budget_for_distinct_nonascii_lead(self):
        entries = [record(1, 0.95, "b:1-1", "b:1-1")]
        self.assertFalse(self.apply(entries, limit=6)[2])
        _, selected, accepted = self.apply(entries, limit=7)
        self.assertEqual(selected[-1].text, "é\n")
        self.assertEqual(accepted[0]["kind"], "new")

    def test_seventh_file_cannot_be_added(self):
        seed = [Evidence(f"f{i}:1-1", "x\n") for i in range(6)]
        self.assertFalse(self.apply([record(1, 0.95, "b:1-1", "b:1-1")], seed=seed)[2])

    def test_scored_location_must_belong_to_proposed_source(self):
        with self.assertRaisesRegex(ValueError, "outside"):
            self.apply([record(1, 0.7, "a:1-1", "b:1-1")])


if __name__ == "__main__":
    unittest.main()
