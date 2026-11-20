# SPDX-License-Identifier: Apache-2.0
"""Reproduce changing stored review bytes between parsing and custody."""
from pathlib import Path
import unittest

from focus_review1024 import bind
from focus_w8_reference1024 import ORIGINAL_REVIEW, original_assets


class SnapshotControls(unittest.TestCase):
    def exercise(self, changed=None, prior=None):
        root = Path(__file__).resolve().parent
        adapter = root / "focus_w8_reference1024.py"
        old = root / "focus-w8-review-r1.json"
        new = root / "focus-w8-static1024-review.json"
        known = dict(original_assets())
        known.update({old: ORIGINAL_REVIEW, new: "b" * 64, adapter: "c" * 64,
                      root / "focus-w8-reference-contract.md": "d" * 64})
        def pin(path):
            path = Path(path)
            return {"path": str(path), "sha256": known[path]}
        originals = []
        for i in range(10):
            path = root / f"synthetic-reviewed-{i}"
            known[path] = f"{i:064x}"
            originals.append(pin(path))
        def snapshot(path):
            digest = known[path]
            if path.name == changed:
                known[path] = "a" * 64
            return digest, {"files": originals if path == old else [pin(adapter)]}
        seals = {} if prior is None else {str(adapter): prior}
        bind(root, adapter, pin(adapter), seals, pin, snapshot)
        self.assertEqual(seals[str(old)], ORIGINAL_REVIEW)
        self.assertEqual(seals[str(new)], "b" * 64)
        for row in originals:
            self.assertEqual(seals[row["path"]], row["sha256"])

    def test_first_review_snapshots_and_original_roster_are_retained(self):
        self.exercise()

    def test_old_or_fresh_review_cannot_change_after_its_inventory_is_read(self):
        for changed in ("focus-w8-review-r1.json", "focus-w8-static1024-review.json"):
            with self.subTest(changed=changed), self.assertRaisesRegex(ValueError, "review changed"):
                self.exercise(changed)

    def test_review_cannot_overwrite_an_earlier_input_seal(self):
        with self.assertRaisesRegex(ValueError, "original input seal"):
            self.exercise(prior="e" * 64)


if __name__ == "__main__":
    unittest.main()
