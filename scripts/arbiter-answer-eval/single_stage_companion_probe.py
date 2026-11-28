# SPDX-License-Identifier: Apache-2.0
"""Exercise the six native companion tools on a previously exposed Munarium development file."""
import argparse
from contextlib import closing
import hashlib
import json
from pathlib import Path
import time

import common
import product_skill_http as http


def observer(path):
    if not path.is_file():
        return {"present": False, "bytes": 0, "rows": 0, "sha256": None}
    raw = path.read_bytes()
    return {"present": True, "bytes": len(raw), "rows": len(raw.splitlines()),
            "sha256": hashlib.sha256(raw).hexdigest()}


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("handle", type=Path)
    parser.add_argument("output", type=Path)
    args = parser.parse_args()
    handle = common.load_handle(args.handle.resolve(strict=True))
    root = args.handle.parent / "fixture"
    paths = [root / name for name in ("composed-observer.jsonl", "warm-completions.jsonl")]
    before = {p.name: observer(p) for p in paths}
    source = "clients/python/src/munarium_client/rest.py"
    calls = [
        ("list_indexed_files", {"limit": 2}),
        ("find_definition", {"name": "_retryable", "limit": 2}),
        ("find_identifier_occurrences", {"name": "_retryable", "limit": 2}),
        ("outline_file", {"path": source}),
        ("read_indexed_range", {"path": source, "start_line": 1, "end_line": 7}),
        ("search_passages", {"query": "_retryable", "limit": 2, "include_status": True}),
    ]
    surface = http.read_surface(handle, "munarium")
    assert {tool["name"] for tool in surface["tools"]} == {name for name, _ in calls} | {"gather_context"}
    rows = []
    with closing(http.ScopedClient(handle, "munarium")) as client:
        client.exchange({"jsonrpc": "2.0", "id": 1, "method": "initialize", "params": {
            "protocolVersion": "2025-11-25", "capabilities": {}, "clientInfo": {"name": "companion-probe", "version": "1"}}})
        client.exchange({"jsonrpc": "2.0", "method": "notifications/initialized"})
        for request_id, (name, arguments) in enumerate(calls, 2):
            started = time.monotonic()
            reply = client.exchange({"jsonrpc": "2.0", "id": request_id, "method": "tools/call",
                                     "params": {"name": name, "arguments": arguments}})
            success = "result" in reply and not reply["result"].get("isError", False)
            rows.append({"tool": name, "arguments": arguments, "reply": reply, "success": success,
                         "seconds": round(time.monotonic() - started, 3)})
    after = {p.name: observer(p) for p in paths}
    receipt = {"service": common.read_json(args.handle)["instance"], "index": handle.index_id("munarium"),
               "before": before, "after": after, "calls": rows}
    with args.output.open("x", encoding="utf-8") as out:
        json.dump(receipt, out, indent=2)
    for row in rows:
        print(row["tool"], row["success"], row["seconds"])
    print("observer counters unchanged", before == after)
    return 0 if all(r["success"] for r in rows) and before == after and all(p["present"] for p in before.values()) else 1


if __name__ == "__main__":
    raise SystemExit(main())
