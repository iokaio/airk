# SPDX-License-Identifier: Apache-2.0
"""Measure read-after-edit behavior in an owned, already registered scratch index.

Run once per phase on a live native fixture. The caller sets the configured scratch
index, edits or resets the one-file workspace, and retires it afterwards. This
probe only reads via scoped HTTP MCP and writes one new private JSON receipt.
"""
import argparse
from contextlib import closing
from dataclasses import replace
import json
from pathlib import Path
import re
import time

import common
from product_skill_http import ScopedClient


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("--handle", type=Path, required=True)
    parser.add_argument("--index", required=True)
    parser.add_argument("--path", required=True)
    parser.add_argument("--marker", required=True)
    parser.add_argument("--previous-revision")
    parser.add_argument("--output", type=Path, required=True)
    parser.add_argument("--deadline", type=int, default=90)
    args = parser.parse_args()
    if args.output.exists():
        parser.error("receipt already exists")
    handle = common.load_handle(args.handle)
    if args.index in handle.indexes.values():
        parser.error("a disposable index distinct from the frozen indexes is required")
    handle = replace(handle, indexes={"scratch": args.index})
    rows = []
    started = time.monotonic()
    with closing(ScopedClient(handle, "scratch")) as client:
        client.exchange({"jsonrpc": "2.0", "id": 1, "method": "initialize", "params": {
            "protocolVersion": "2025-11-25", "capabilities": {}, "clientInfo": {"name": "airk-eval", "version": "1"}}})
        client.exchange({"jsonrpc": "2.0", "method": "notifications/initialized"})

        def read(revision=None):
            call = {"path": args.path, "start_line": 1, "end_line": 1, "include_status": True}
            if revision:
                call["revision"] = revision
            reply = client.exchange({"jsonrpc": "2.0", "id": len(rows) + 2, "method": "tools/call",
                                     "params": {"name": "read_indexed_range", "arguments": call}})
            body = common.envelope_body(reply.get("result"))
            text = json.dumps(body, ensure_ascii=False)
            tokens = sorted(set(re.findall(r"airk-r1:[a-f0-9]{32}:[0-9]+:[a-f0-9]{64}", text)))
            row = {"elapsed_s": time.monotonic() - started, "requested": revision, "publications": tokens,
                   "marker_present": args.marker in text, "is_error": bool(reply.get("error") or
                       reply.get("result", {}).get("isError")), "reply": reply}
            rows.append(row)
            return row

        deadline = started + args.deadline
        while True:
            current = read()
            if (not current["is_error"] and current["marker_present"] and len(current["publications"]) == 1
                    and current["publications"][0] != args.previous_revision):
                break
            if time.monotonic() >= deadline:
                break
            time.sleep(min(1, max(0, deadline - time.monotonic())))
        old = read(args.previous_revision) if args.previous_revision else None
    with args.output.open("x", encoding="utf-8") as stream:
        json.dump({"index": args.index, "path": args.path, "observations": rows}, stream, indent=2)
    print(json.dumps({k: v for k, v in current.items() if k != "reply"}))
    if old:
        print("previous revision", json.dumps({k: v for k, v in old.items() if k != "reply"}))
    return int(current["is_error"] or not current["marker_present"] or len(current["publications"]) != 1
               or current["publications"][0] == args.previous_revision)


if __name__ == "__main__":
    raise SystemExit(main())
