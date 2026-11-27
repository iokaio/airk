# SPDX-License-Identifier: Apache-2.0
"""The harness reads the Rust-emitted public tool name table and serves pre-1.9 and 1.9 images alike."""
from __future__ import annotations

import json
import tempfile
import unittest
from pathlib import Path

import common
import tool_names


class ToolNames(unittest.TestCase):
    def test_every_spelling_maps_to_one_native_name(self) -> None:
        # Literal expectations from the user's decision of 26 November 2026, not read from the table.
        for native, legacy, current in [
            ("airk_answer", "answer", "gather_context"),
            ("airk_search", "search", "search_passages"),
            ("airk_symbols", "symbols", "find_definition"),
            ("airk_references", "references", "find_identifier_occurrences"),
            ("airk_read_file", "read_file", "read_indexed_range"),
            ("airk_context", "context", "read_around"),
            ("airk_outline", "outline", "outline_file"),
            ("airk_list_files", "list_files", "list_indexed_files"),
            ("airk_status", "status", "index_status"),
            ("airk_file_info", "file_info", "file_info"),
            ("airk_parse", "parse", "parse"),
        ]:
            for spelling in (native, legacy, current):
                self.assertEqual(tool_names.native(spelling), native, spelling)
            self.assertEqual(tool_names.public(native), current)
            self.assertEqual(tool_names.public(native, names="legacy"), legacy)
            self.assertEqual(tool_names.operation(current), legacy)
        with self.assertRaises(ValueError):
            tool_names.public("airk_search", names="old")

    def test_mcp_offers_the_seven_core_retrieval_tools(self) -> None:
        # Literal list from the user's decisions of 27 November 2026, not read from the table.
        self.assertEqual(tool_names.mcp_tools(), (
            "gather_context", "search_passages", "find_definition", "find_identifier_occurrences",
            "read_indexed_range", "outline_file", "list_indexed_files"))

    def test_a_catalog_from_either_image_is_keyed_by_native_names(self) -> None:
        wanted = {name for names in common.PATH_TOOLS.values() for name in names}
        for names in tool_names.NAMES:
            listing = [{"name": tool_names.public(name, names=names), "description": "d",
                        "inputSchema": {"type": "object"}} for name in sorted(wanted)]
            with tempfile.TemporaryDirectory() as directory:
                path = Path(directory) / "tools-catalog.json"
                path.write_text(json.dumps({"tools": listing}), encoding="utf-8")
                catalog = common.load_catalog(path)
            self.assertTrue(wanted <= set(catalog), names)
            self.assertEqual(catalog["airk_search"]["catalog_name"],
                             "search_passages" if names == "current" else "search")


if __name__ == "__main__":
    unittest.main()
