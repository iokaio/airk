# SPDX-License-Identifier: Apache-2.0
"""Run with the provisioned PyArrow interpreter and -I -B."""

import hashlib
import io
from pathlib import Path
import runpy
import unittest
from unittest.mock import Mock, patch

import pyarrow as pa
import pyarrow.parquet as pq


SCRIPTS = Path(__file__).resolve().parents[1]
SCHEMA_ONLY = runpy.run_path(str(SCRIPTS / "arbiter/analysis/public_source_admission/audit_repository_label_schema.py"))["schema_only"]


class RepositoryLabelSchemaTests(unittest.TestCase):
    def setUp(self):
        stream = io.BytesIO()
        pq.write_table(pa.table({"marker": ["dummy"]}), stream)
        self.raw = stream.getvalue()
        self.sha = hashlib.sha256(self.raw).hexdigest()
        self.owner = Mock()
        self.owner.regular.return_value.open.return_value = io.BytesIO(self.raw)
        self.state = (0, 1, len(self.raw), 2, 3)
        self.owner.archive_state.return_value = self.state

    def test_returns_only_footer_without_reading_cases(self):
        with patch.object(pq.ParquetFile, "read", side_effect=AssertionError("case access")):
            result = SCHEMA_ONLY(self.owner, Path("fixture"), len(self.raw), self.sha)
        self.assertEqual(result, {"bytes": len(self.raw), "sha256": self.sha, "rows": 1,
                                  "fields": [("marker", "string")]})

    def test_reject_wrong_archive_digest(self):
        with self.assertRaisesRegex(ValueError, "archive identity differs"):
            SCHEMA_ONLY(self.owner, Path("fixture"), len(self.raw), "0" * 64)

    def test_reject_changed_archive(self):
        self.owner.archive_state.side_effect = [self.state, (0, 1, len(self.raw), 9, 9)]
        with self.assertRaisesRegex(ValueError, "archive changed"):
            SCHEMA_ONLY(self.owner, Path("fixture"), len(self.raw), self.sha)


if __name__ == "__main__":
    unittest.main()
