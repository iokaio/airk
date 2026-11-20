# SPDX-License-Identifier: Apache-2.0
"""Independent controls for byte geometry and withholding in the native reader."""

import hashlib
from pathlib import Path
import runpy
import unittest
from unittest.mock import Mock


SOURCE = Path(__file__).resolve().parents[1] / "arbiter/analysis/public_source_admission"
AUDIT = runpy.run_path(str(SOURCE / "verify_contextbench_native_source_reads.py"))
TAILWIND = "daea6623fd691d52e6ff1ce440f6a7d29b9d3adb"


class ContextBenchNativeReads(unittest.TestCase):
    def test_raw_lines_include_utf8_prefix_and_original_cr(self):
        span = AUDIT["raw_lines"]("é\na\r\nb\n".encode(), 2, 3)
        self.assertEqual((span["byte_start"], span["byte_end_exclusive"]), (3, 7))
        self.assertEqual(span["span_sha256"], hashlib.sha256(b"a\r\nb").hexdigest())

    def test_reject_range_after_trailing_newline(self):
        with self.assertRaisesRegex(ValueError, "beyond file end"):
            AUDIT["raw_lines"](b"a\n", 1, 2)

    def test_partial_case_preserves_all_withheld_ordinals(self):
        names = ["a.js", "b.js", "c.js"]
        case = {"repo": "org/repo", "base": TAILWIND, "source_paths": names + ["yarn.lock"]}
        sha = hashlib.sha256(b"x").hexdigest()
        files = [{"repo": "org/repo", "base": TAILWIND, "area": "sources", "path": name,
                  "relative_path": f"sources/{TAILWIND}/{name}", "bytes": 1, "sha256": sha} for name in names]
        manifest = {"cases": {"case": case}, "staged": files,
                    "decisions": [{"base": TAILWIND, "path": "yarn.lock", "staged": False}]}
        rows = [{"instance_id": "case", "ordinal": i, "status": "exact_declared_lines", "repo": "org/repo",
                 "base": TAILWIND, "path": names[i % 3] if i < 4 else "yarn.lock", "start": 1, "end": 1}
                for i in range(20)]
        class Files:
            def __truediv__(self, path):
                assert path in names
                return Mock(read_bytes=lambda: b"x")
        cases, refused, docs = AUDIT["revision_cases"](TAILWIND, manifest, {"rows": rows}, Files())
        self.assertEqual([r["selector"]["ordinal"] for r in cases], list(range(4)))
        self.assertEqual([r["ordinal"] for r in refused], list(range(4, 20)))
        self.assertEqual(set(docs), set(names))
        rows[0]["base"] = "foreign"
        with self.assertRaisesRegex(ValueError, "unverified declaration"):
            AUDIT["revision_cases"](TAILWIND, manifest, {"rows": rows}, Files())


if __name__ == "__main__":
    unittest.main()
