# SPDX-License-Identifier: Apache-2.0
"""Synthetic tests for lg_lifetimes.py; no service, GPU or private input is used."""

import hashlib
import json
import tempfile
import unittest
from pathlib import Path

import lg_lifetimes as lg


def write(path, value):
    data = (json.dumps(value) + "\n").encode()
    Path(path).write_bytes(data)
    return hashlib.sha256(data).hexdigest()


class LifetimesTests(unittest.TestCase):
    def test_each_arm_gets_its_root_pinned_pair_and_measured_memory(self):
        with tempfile.TemporaryDirectory() as folder:
            root = Path(folder)
            pairs = {}
            for arm, rows in (("C", {"c": "context", "f": "focus"}), ("L", {"l": "context", "f": "focus"}),
                              ("F", {"c": "context", "f": "focus"})):
                path = root / f"{arm}.json"
                sha = write(path, {"profiles": {n: {"stage": s, "model": n} for n, s in rows.items()}})
                pairs[arm] = (str(path), sha)
            out = lg.build("05-gate-lg", root, root / "image", pairs, {"L": "05-gate-lg-L-base"})
            self.assertEqual((out["C"]["device_gib"], out["L"]["device_gib"], out["F"]["device_gib"]), (10, 18, 3))
            self.assertIn("--seed-cpu", out["C"]["start_args"])
            self.assertEqual(out["L"]["start_args"][-2:], ["--restart-from", "05-gate-lg-L-base"])
            self.assertEqual(len({row["fixtures"] for row in out.values()}), 3)
            # A changed pair or a pair without a CUDA Focus entry is refused.
            with self.assertRaises(ValueError):
                lg.build("s", root, root, dict(pairs, C=(pairs["C"][0], "0" * 64)), {})
            sha = write(root / "nofocus.json", {"profiles": {"c": {"stage": "context", "model": "compact"}}})
            with self.assertRaises(ValueError):
                lg.build("s", root, root, dict(pairs, C=(str(root / "nofocus.json"), sha)), {})


if __name__ == "__main__":
    unittest.main()
