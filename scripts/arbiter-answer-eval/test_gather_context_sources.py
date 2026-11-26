# SPDX-License-Identifier: Apache-2.0
"""Direct source tool fixtures and refusal controls, with no filesystem writes."""

import io
import json
from pathlib import Path
import stat
from types import SimpleNamespace
import unittest
from unittest import mock

import gather_context_sources as sources


ROOT = Path(__file__).resolve().parent


class OrdinarySourceToolsTests(unittest.TestCase):
    def call(self, tool, **arguments):
        reply = sources.dispatch(ROOT, tool, arguments)
        self.assertEqual(len(reply["content"]), 1)
        return json.loads(reply["content"][0]["text"]), reply.get("isError", False)

    def test_tool_inventory_is_scoped_and_reports_output_cutoff(self):
        with mock.patch.object(sources, "MAX_LIST_PATHS", 1):
            body, error = self.call("source_list", prefix="client-timeout-probe")
        self.assertFalse(error)
        self.assertEqual(body["prefix"], "client-timeout-probe")
        self.assertGreater(body["enumerated_files"], 1)
        self.assertEqual(len(body["paths"]), 1)
        self.assertTrue(body["paths"][0].startswith("client-timeout-probe/"))
        self.assertFalse(body["complete"])
        self.assertEqual(body["cutoffs"], ["returned_path_limit"])

    def test_literal_has_exact_occurrences_case_choice_and_real_lines(self):
        raw = b"action.route action.route\r\nAck ACTION.ROUTE\nactXon.route\n"
        file = ROOT / "test_replay.py"
        with (
            mock.patch.object(sources, "_inventory", side_effect=lambda root, directory: (
                [file], {"prefix": "", "cutoffs": []})),
            mock.patch.object(Path, "open", side_effect=lambda _: io.BytesIO(raw)),
        ):
            exact, exact_error = self.call("source_find", literal="action.route")
            folded, folded_error = self.call("source_find", literal="action.route", case_sensitive=False)
        self.assertFalse(exact_error or folded_error)
        self.assertTrue(exact["complete"])
        self.assertEqual(exact["occurrences"], 2)
        self.assertEqual(exact["matched_lines"], 1)
        self.assertEqual(exact["counts"], [{"path": "test_replay.py", "occurrences": 2}])
        self.assertEqual(folded["occurrences"], 3)
        self.assertEqual([match["start_line"] for match in folded["matches"]], [1, 2])

    def test_find_reports_partial_work_separately_from_partial_output(self):
        files = [ROOT / "replay.py", ROOT / "test_replay.py"]
        file_content = b"Test\nTest\n"
        with (
            mock.patch.object(sources, "_inventory", side_effect=lambda root, directory: (
                files, {"prefix": "", "cutoffs": []})),
            mock.patch.object(Path, "open", side_effect=lambda _: io.BytesIO(file_content)),
            mock.patch.object(sources, "MAX_FIND_LINES", 1),
        ):
            clipped, error = self.call("source_find", literal="Test")
            with mock.patch.object(sources, "MAX_SCAN_BYTES", len(file_content) + 1):
                partial, partial_error = self.call("source_find", literal="Test")
        self.assertFalse(error or partial_error)
        self.assertTrue(clipped["scan_complete"])
        self.assertFalse(clipped["complete"])
        self.assertEqual(clipped["results_cutoffs"], ["returned_line_limit"])
        self.assertEqual(clipped["occurrences"], 4)
        self.assertFalse(partial["scan_complete"])
        self.assertFalse(partial["complete"])
        self.assertEqual(partial["cutoffs"], ["scan_byte_limit"])
        self.assertEqual(partial["examined_files"], 1)

    def test_read_preserves_original_range_line_endings_and_partial_eof(self):
        raw = "alpha\r\nbéta\r\nend".encode("utf-8")
        with mock.patch.object(Path, "open", side_effect=lambda _: io.BytesIO(raw)):
            exact, error = self.call("source_read", path="test_replay.py", start_line=2, end_line=2)
            eof, eof_error = self.call("source_read", path="test_replay.py", start_line=2, end_line=5)
        self.assertFalse(error or eof_error)
        self.assertTrue(exact["complete"])
        self.assertEqual(exact["delivered_source_bytes"], len("béta\r\n".encode("utf-8")))
        self.assertEqual(exact["passages"], [
            {"path": "test_replay.py", "start_line": 2, "end_line": 2, "text": "béta\r\n"},
        ])
        self.assertFalse(eof["complete"])
        self.assertEqual(eof["cutoff"], "end_of_file")
        self.assertEqual(eof["passages"][0]["end_line"], 3)
        self.assertEqual(eof["passages"][0]["text"], "béta\r\nend")

    def test_read_does_not_clip_a_multibyte_line_into_a_passage(self):
        with (
            mock.patch.object(Path, "open", side_effect=lambda _: io.BytesIO("ββ\n".encode("utf-8"))),
            mock.patch.object(sources, "MAX_READ_BYTES", 3),
        ):
            body, error = self.call("source_read", path="test_replay.py", start_line=1, end_line=1)
        self.assertFalse(error)
        self.assertFalse(body["complete"])
        self.assertEqual(body["cutoff"], "returned_byte_limit")
        self.assertEqual(body["passages"], [])

    def test_refuses_invalid_escapes_devices_streams_and_tool_arguments(self):
        for path in ("../README.md", "/README.md", "C:/README.md", "README.md:stream",
                     "bad\\file", "bad./file", ".git/config", "NUL.txt", "COM¹"):
            with self.subTest(path=path):
                body, error = self.call("source_read", path=path, start_line=1, end_line=2)
                self.assertTrue(error)
                self.assertFalse(body["complete"])
        self.assertTrue(self.call("source_read", path="test_replay.py", start_line=True, end_line=2)[1])
        self.assertTrue(self.call("source_list", prefix="client-timeout-probe", index="other")[1])
        self.assertTrue(self.call("source_find", literal="first\nsecond")[1])
        self.assertTrue(self.call("source_find", literal="first", case_sensitive="true")[1])

    def test_refuses_a_reparse_point_before_opening_the_source(self):
        root_stat = ROOT.lstat()
        reparse = SimpleNamespace(st_mode=stat.S_IFREG, st_file_attributes=stat.FILE_ATTRIBUTE_REPARSE_POINT)

        def attributes(path):
            return root_stat if path == ROOT else reparse

        with mock.patch.object(Path, "lstat", attributes), mock.patch.object(Path, "open") as opening:
            body, error = self.call("source_read", path="junction/file", start_line=1, end_line=1)
        self.assertTrue(error)
        self.assertIn("Linked source paths", body["error"])
        opening.assert_not_called()

    def test_source_range_is_returned_from_an_actual_file(self):
        body, error = self.call("source_read", path="test_replay.py", start_line=1, end_line=1)
        self.assertFalse(error)
        self.assertTrue(body["complete"])
        self.assertTrue(body["passages"][0]["text"].startswith("# SPDX-License-Identifier:"))


if __name__ == "__main__":
    unittest.main()
