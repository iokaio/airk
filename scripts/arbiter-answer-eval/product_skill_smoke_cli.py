# SPDX-License-Identifier: Apache-2.0
"""Synthetic CLI double for the seven-tool client wiring smoke. Never use for cost or retrieval scoring."""

import json
from pathlib import Path
import sys

import common
import tool_names


def main(argv):
    catalog = common.load_catalog(Path(argv[0]))
    if "list" in argv:
        rows = []
        for name in tool_names.mcp_tools():
            row = catalog[tool_names.native(name)]
            rows.append({"name": name, "description": row["description"], "inputSchema": row["inputSchema"]})
        print(json.dumps({"tools": rows}))
        return
    name = argv[argv.index("call") + 1]
    args = json.loads(argv[argv.index("--args") + 1])
    # Only this synthetic definition is supplied. Other requests get an explicit miss.
    found = name == "airk_symbols" and "LumenParcel" in json.dumps(args)
    content = ("catalog/parcel.py:3 class LumenParcel" if found else "No matching evidence in this synthetic response.")
    print(json.dumps({"content": [{"type": "text", "text": content}]}))


if __name__ == "__main__":
    main(sys.argv[1:])
