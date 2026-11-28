# SPDX-License-Identifier: Apache-2.0
"""Bridge one frozen experimental Gather arm into a scoped native HTTP MCP session."""

from contextlib import closing
from pathlib import Path
import sys

import common
import mcp_wrapper
import tool_names

sys.path.insert(0, str(Path(__file__).resolve().parents[1]))
import http_fixture  # noqa: E402


class ScopedClient(http_fixture.McpClient):
    def __init__(self, handle, repo):
        record = common.read_json(handle.path)
        endpoint = http_fixture.read_endpoint(handle.control)
        common.require(record.get("state") == "ready" and record.get("instance") == endpoint["service_instance"]
                       and record["image"]["service"]["sha256"] == endpoint["executable_sha256"],
                       "HTTP endpoint differs from the frozen native fixture")
        self.index = handle.index_id(repo)
        super().__init__(endpoint, handle.control)

    def connection_headers(self):
        return {"X-AIRK-Scope": self.index, "X-AIRK-Response-Mode": "terse"}

    def exchange(self, message):
        self.send(message, scoped=False)
        return self.receive(message["id"]) if "id" in message else None


def read_surface(handle, repo):
    """Capture exactly the descriptions, schemas, annotations, order and guidance of the endpoint."""
    with closing(ScopedClient(handle, repo)) as client:
        init = client.exchange({"jsonrpc": "2.0", "id": 1, "method": "initialize", "params": {
            "protocolVersion": "2025-11-25", "capabilities": {}, "clientInfo": {"name": "airk-eval", "version": "1"}}})
        client.exchange({"jsonrpc": "2.0", "method": "notifications/initialized"})
        listing = client.exchange({"jsonrpc": "2.0", "id": 2, "method": "tools/list", "params": {}})["result"]
        tools = listing["tools"]
        common.require(not listing.get("nextCursor") and sorted(t["name"] for t in tools) == sorted(tool_names.mcp_tools()),
                       "native MCP endpoint must expose exactly the seven current tools")
        return {"instructions": init["result"]["instructions"], "tools": tools}


class Bridge:
    def __init__(self, client, gather, surface):
        self.client, self.gather, self.surface = client, gather, surface
        self.schemas = {t["name"]: t["inputSchema"] for t in surface["tools"]}

    def handle_message(self, message):
        if not isinstance(message, dict) or message.get("jsonrpc") != "2.0":
            return mcp_wrapper._error(None, -32600, "Invalid request")
        method, msg_id = message.get("method"), message.get("id")
        params = message.get("params", {})
        if not isinstance(params, dict):
            return mcp_wrapper._error(msg_id, -32602, "Expected object parameters")
        if method == "tools/call":
            name, args = params.get("name"), params.get("arguments", {})
            if not isinstance(name, str) or name not in self.schemas:
                return mcp_wrapper._error(msg_id, -32602, "Unknown tool")
            if not isinstance(args, dict):
                return mcp_wrapper._error(msg_id, -32602, "Expected object arguments")
            if set(args).difference(self.schemas[name].get("properties", {})):
                return mcp_wrapper._error(msg_id, -32602, "Unknown tool argument")
            if name == "gather_context":
                return self.gather.handle_message(message)
        elif method == "initialize":
            # Scope comes from the configured connection. The bridge has no client-root callback.
            message = {**message, "params": {**params, "capabilities": {}}}
        elif method not in ("notifications/initialized", "ping", "tools/list"):
            return None if msg_id is None else mcp_wrapper._error(msg_id, -32601, "Unknown method")
        try:
            reply = self.client.exchange(message)
        except http_fixture.HttpResponseError as error:
            if isinstance(error.body, dict) and error.body.get("id") == msg_id and "error" in error.body:
                return error.body
            reply = mcp_wrapper._error(msg_id, -32603, str(error))
            reply["error"]["data"] = {"status": error.status, "body": error.body, "submission": error.submission}
            return reply
        if method == "initialize":
            common.require(reply["result"].get("instructions") == self.surface["instructions"],
                           "native initialization guidance changed after preflight")
        if method == "tools/list":
            common.require(reply["result"]["tools"] == self.surface["tools"] and not reply["result"].get("nextCursor"),
                           "native MCP catalog changed after preflight")
        return reply
