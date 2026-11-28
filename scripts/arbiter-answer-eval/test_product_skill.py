# SPDX-License-Identifier: Apache-2.0
import unittest
from unittest.mock import Mock

import product_skill


class ProductSurfaceTest(unittest.TestCase):
    routes = {"find_definition": "airk_symbols", "find_identifier_occurrences": "airk_references",
              "outline_file": "airk_outline", "search_passages": "airk_search",
              "read_indexed_range": "airk_read_file", "list_indexed_files": "airk_list_files"}

    def setUp(self):
        tools = [{"name": name} for name in ["gather_context", *self.routes, "extra"]]
        self.gather, self.native = Mock(), Mock(return_value={"content": [], "isError": True})
        self.server = product_skill.ProductServer(self.gather, tools, self.native)

    def call(self, name, args):
        return self.server.handle_message({"jsonrpc": "2.0", "id": 1, "method": "tools/call",
                                           "params": {"name": name, "arguments": args}})

    def test_catalog_and_dispatch(self):
        self.assertEqual(self.server.names, {"gather_context", *self.routes})
        args = {"cursor": "next", "revision": "token", "limit": 4, "max_bytes": 200}
        for public, native in self.routes.items():
            with self.subTest(public=public):
                self.assertEqual(self.call(public, args)["result"], self.native.return_value)
                self.native.assert_called_with(native, args)
        self.call("gather_context", {"task": "trace"})
        self.gather.handle_message.assert_called_once()

    def test_scope_is_supplied_by_connection(self):
        for name in self.server.names:
            for field in ("index", "project"):
                self.assertTrue(self.call(name, {field: "other"})["result"]["isError"])
        self.native.assert_not_called()
        self.gather.handle_message.assert_not_called()

    def test_current_guidance_and_missing_catalog(self):
        self.assertIn("find_definition", product_skill.guidance())
        with self.assertRaisesRegex(ValueError, "gather_context"):
            product_skill.select_tools([])
