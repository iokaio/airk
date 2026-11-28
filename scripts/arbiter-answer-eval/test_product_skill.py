# SPDX-License-Identifier: Apache-2.0
import unittest
from unittest.mock import Mock

import product_skill
import product_skill_http


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


class ProductHttpTest(unittest.TestCase):
    def setUp(self):
        self.http, self.gather = Mock(), Mock()
        self.surface = {"instructions": "from initialize", "tools": [
            {"name": "gather_context", "inputSchema": {"properties": {"task": {}}}},
            {"name": "search_passages", "inputSchema": {"properties": {"limit_per_index": {}, "cursor": {}}}},
        ]}
        self.bridge = product_skill_http.Bridge(self.http, self.gather, self.surface)

    def send(self, method, params):
        return self.bridge.handle_message({"jsonrpc": "2.0", "id": 3, "method": method, "params": params})

    def test_native_handshake_and_catalog(self):
        self.http.exchange.return_value = {"result": {"instructions": self.surface["instructions"], "serverInfo": {"version": "observed"}}}
        self.assertIs(self.send("initialize", {"capabilities": {"roots": {}}}), self.http.exchange.return_value)
        self.assertEqual(self.http.exchange.call_args.args[0]["params"]["capabilities"], {})
        self.http.exchange.return_value = {"result": {"tools": self.surface["tools"]}}
        self.assertIs(self.send("tools/list", {}), self.http.exchange.return_value)
        self.http.exchange.return_value = {"result": {"tools": []}}
        with self.assertRaisesRegex(product_skill_http.common.HarnessError, "catalog changed"):
            self.send("tools/list", {})

    def test_only_gather_is_intercepted(self):
        args = {"limit_per_index": 2, "cursor": "opaque"}
        self.http.exchange.return_value = {"result": {"isError": True, "content": [], "structuredContent": {"code": "expired"}}}
        self.assertIs(self.send("tools/call", {"name": "search_passages", "arguments": args}), self.http.exchange.return_value)
        self.assertEqual(self.http.exchange.call_args.args[0]["params"]["arguments"], args)
        self.send("tools/call", {"name": "gather_context", "arguments": {"task": "read"}})
        self.gather.handle_message.assert_called_once()
        for name in ("search_passages", "gather_context"):
            for field in ("index_ids", "index_set_ids", "workspace", "configured_scope", "project"):
                self.assertIn("error", self.send("tools/call", {"name": name, "arguments": {field: "other"}}))
        self.http.exchange.assert_called_once()

    def test_native_gather_passes_through_and_records_exact_result(self):
        record = Mock()
        bridge = product_skill_http.Bridge(self.http, None, self.surface, record)
        msg = {"jsonrpc": "2.0", "id": 3, "method": "tools/call",
               "params": {"name": "gather_context", "arguments": {"task": "locate rule"}}}
        self.http.exchange.return_value = {"jsonrpc": "2.0", "id": 3, "result": {"content": [], "isError": False}}
        self.assertIs(bridge.handle_message(msg), self.http.exchange.return_value)
        self.http.exchange.assert_called_once_with(msg)
        self.assertEqual(record.call_args.args[:2], (msg["params"]["arguments"], self.http.exchange.return_value))

    def test_http_refusal_retains_native_body(self):
        body = {"error": {"code": "query_admission_busy", "retry_after_ms": 900}}
        self.http.exchange.side_effect = product_skill_http.http_fixture.HttpResponseError(429, body)
        reply = self.send("tools/call", {"name": "search_passages", "arguments": {}})
        self.assertEqual(reply["error"]["data"], {"status": 429, "body": body, "submission": "observed"})

    def test_connection_scope_and_mode(self):
        client = product_skill_http.ScopedClient.__new__(product_skill_http.ScopedClient)
        client.index = "idx_frozen"
        self.assertEqual(client.connection_headers(), {"X-AIRK-Response-Mode": "terse", "X-AIRK-Scope": "idx_frozen"})
