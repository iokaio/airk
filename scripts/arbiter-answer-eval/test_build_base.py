# SPDX-License-Identifier: Apache-2.0
"""Synthetic tests for build_base.py; no service is started."""

import tempfile
import unittest
from pathlib import Path

import build_base


class BuildBaseTests(unittest.TestCase):
    def test_semantic_readiness_is_read_as_lg_bases_reads_it(self):
        self.assertTrue(build_base.semantic_ready({"state": "ready", "semantic_ready_utc": "t"}))
        self.assertFalse(build_base.semantic_ready({"state": "ready", "semantic": {"munarium": "failed"}}))
        self.assertFalse(build_base.semantic_ready({"state": "ready", "semantic_ready_utc": "t", "error": "e"}))
        self.assertFalse(build_base.semantic_ready(None))

    def test_the_failed_vectors_job_is_read_from_the_progress_outputs(self):
        # The shape of a real progress observation (s6 base, 23 November 2026), shortened.
        text = ('{"status":"progress","observation":{"progress":{"phase":"publishing"},"observed_unix_ms":1795414084843,'
                '"job":{"id":"f393ceee","project":"p","kind":"vectors","state":"failed","fencing_generation":1,'
                '"attempts":2,"outcome":{"state":"failed","code":"project_operation_failed"}}}}')
        with tempfile.TemporaryDirectory() as folder:
            commands = Path(folder) / "commands"
            commands.mkdir()
            (commands / "0001-semantic-progress.stdout").write_text('{"observation":{"job":{"kind":"index"}}}')
            self.assertIsNone(build_base.failed_vectors_job(Path(folder)))
            (commands / "0342-semantic-progress.stdout").write_text(text)
            found = build_base.failed_vectors_job(Path(folder))
            self.assertEqual((found["job"], found["attempts"], found["code"], found["observed_unix_ms"]),
                             ("f393ceee", 2, "project_operation_failed", 1795414084843))


if __name__ == "__main__":
    unittest.main()
