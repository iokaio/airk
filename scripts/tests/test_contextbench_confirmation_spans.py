# SPDX-License-Identifier: Apache-2.0
"""Focused controls for the confirmation source-span join."""

import hashlib
import io
from pathlib import Path
import runpy
import unittest
from unittest.mock import Mock

import pyarrow as pa
import pyarrow.parquet as pq


SOURCE = Path(__file__).resolve().parents[1] / "arbiter/analysis/public_source_admission"
AUDIT = runpy.run_path(str(SOURCE / "audit_contextbench_confirmation_spans.py"))
OWNER = runpy.run_path(str(SOURCE / "contextbench_blind_metadata_census.py"))


class ConfirmationSpans(unittest.TestCase):
    def test_bound_paths_to_exact_declared_repo(self):
        relative = AUDIT["relative_file"]
        self.assertEqual(relative("/workspace/Org__Repo__0.1/f/a.py", "org/repo"), "f/a.py")
        self.assertEqual(relative("/testbed/f/a.py", "org/repo"), "f/a.py")
        for path in ("/workspace/other__repo__0.1/f", "../f", "/f", "f//a", "f\\a", "d:f"):
            self.assertIsNone(relative(path, "org/repo"))

    def test_exact_ranges_keep_whitespace_and_never_relocate(self):
        compare = AUDIT["compare"]
        raw = b"first\r\n  second\r\nthird\r\n"
        self.assertEqual(compare("  second", 2, 2, raw), "exact_declared_lines")
        self.assertEqual(compare("second", 2, 2, raw), "declared_lines_differ")
        self.assertEqual(compare("third", 2, 2, raw), "declared_lines_differ")
        self.assertEqual(compare("third", 3, 4, raw), "out_of_bounds")

    def test_number_format_requires_every_correct_line_number(self):
        compare = AUDIT["compare"]
        raw = b"a\nb\n"
        self.assertEqual(compare("1\ta\n2\tb\n", 1, 2, raw), "exact_numbered_declared_lines")
        self.assertEqual(compare("2\ta\n3\tb", 1, 2, raw), "declared_lines_differ")

    def test_filter_before_python_gold_decode(self):
        stream = io.BytesIO()
        pq.write_table(pa.table({"original_inst_id": ["withheld", "chosen"],
                                 "gold_context": ["invalid json", '[{"example":1}]'],
                                 "patch": ["never requested", "never requested"]}), stream)
        raw = stream.getvalue()
        regular = Mock()
        regular.return_value.open.return_value = io.BytesIO(raw)
        owner = {"regular": regular, "archive_state": lambda _: (1, 2, len(raw), 3, 4)}
        selected = AUDIT["selected_annotations"](owner, Path("unused"), hashlib.sha256(raw).hexdigest(),
                                                 [{"original_inst_id": "chosen"}])
        self.assertEqual(selected, {"chosen": [{"example": 1}]})

    def test_training_repo_leaf_and_commit_exclusions(self):
        def row(key, repo, base):
            return dict(original_inst_id=key, instance_id=key, repo=repo,
                        repo_url="https://github.com/" + repo, base_commit=base * 40)
        rows = [row("train", "old/core", "a"), row("alias", "different/core", "b"),
                row("commit", "other/lib", "a"), row("good", "new/mod", "c")]
        self.assertEqual([r["original_inst_id"] for r in AUDIT["select"](OWNER, rows, {"train"})], ["good"])


if __name__ == "__main__":
    unittest.main()
