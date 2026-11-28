# SPDX-License-Identifier: Apache-2.0
"""The production skill and seven-tool surface for topical-index experiments."""

import json
from pathlib import Path
import re
import subprocess

import common
import mcp_wrapper
import tool_names

SKILL = Path(__file__).resolve().parents[2] / "crates" / "airkctl" / "skills" / "airk-tools" / "SKILL.md"
QUALIFIED_NAME = "airkctl:airk-tools"


def guidance():
    """Read the current unified MCP initialization text from its Rust owner."""
    source = SKILL.parents[3] / "airk-mcp" / "src" / "unified.rs"
    line = re.search(r'^pub const GUIDANCE: &str = (".*");$', source.read_text(encoding="utf-8"), re.MULTILINE)
    if line is None:
        raise ValueError(f"Cannot extract the unified guidance from {source}")
    return json.loads(line.group(1))


def catalog(handle):
    command = [*handle.cli, "--output", "json", "--service-runtime-dir", handle.control, "tools", "list", "--json"]
    output = subprocess.run(command, env=common.tool_env(handle), capture_output=True, check=True, timeout=120)
    return select_tools(json.loads(output.stdout)["tools"])


def select_tools(listing):
    by_name = {tool["name"]: tool for tool in listing}
    # Exact public spellings keep a legacy release from masquerading as the candidate.
    required = tool_names.mcp_tools()
    missing = set(required).difference(by_name)
    if missing:
        raise ValueError(f"Fixture lacks current public tool names: {sorted(missing)}")
    return [by_name[name] for name in required]


class ProductServer:
    """Delegate Gather to the selected experimental arm and other tools to the same scoped fixture."""

    def __init__(self, gather, tools, call_native):
        self.gather = gather
        self.tools = select_tools(tools)
        self.names = {t["name"] for t in self.tools}
        self.call_native = call_native

    def handle_message(self, message):
        if not isinstance(message, dict) or message.get("jsonrpc") != "2.0" or "id" not in message:
            return self.gather.handle_message(message)
        if message.get("method") == "tools/list":
            return mcp_wrapper._result(message["id"], {"tools": self.tools})
        if message.get("method") == "tools/call":
            params = message.get("params") or {}
            if not isinstance(params, dict) or not isinstance(params.get("name"), str):
                return mcp_wrapper._error(message["id"], -32602, "Expected a tool name and arguments")
            name, args = params.get("name"), params.get("arguments", {})
            if name in self.names and (not isinstance(args, dict) or common.HIDDEN_ARGS.intersection(args)):
                return mcp_wrapper._result(message["id"], {"content": [{"type": "text", "text": "The connection supplies the index scope."}], "isError": True})
            if name in self.names and name != "gather_context":
                return mcp_wrapper._result(message["id"], self.call_native(tool_names.native(name), args))
        return self.gather.handle_message(message)
