# SPDX-License-Identifier: Apache-2.0
"""A stand-in for the AIRK CLI's ``tools call`` that serves canned synthetic results.

Used by the unit tests and the smoke run before the evaluation service exists.
Invoked exactly like the real CLI::

    mock_cli.py --canned <file> --output json --service-runtime-dir <dir> \
        tools call <tool> --index <id> --args <json>

The canned file holds synthetic data only::

    {"files": {"path": "text"},             # served by read_file/context/references
     "spans": [{"path", "lines": [s, e]}],  # hits for search/answer/parse, in order
     "symbols": [{"name", "kind", "path", "lines": [s, e]}],
     "fail": ["tool"]}                       # tools that exit non-zero

Bodies follow the native shapes (hits[].citation.{path,start_line,end_line},
answer records[].canonical_result, parse evidence[], compact outline items).
"""

from __future__ import annotations

import argparse
import json
import sys
from pathlib import Path


def citation(path: str, start: int, end: int) -> dict:
    return {"path": path, "content_hash": "0" * 64, "start_line": start, "end_line": end,
            "location": {"Lines": {"start": start, "end": end}}}


def lines_of(files: dict, path: str, start: int, end: int) -> str:
    rows = files.get(path, "").split("\n")
    return "\n".join(rows[max(0, start - 1):end])


def hit(files: dict, path: str, start: int, end: int) -> dict:
    return {"citation": citation(path, start, end), "text": lines_of(files, path, start, end), "truncated": False}


def search_body(canned: dict) -> dict:
    return {"collection": "mock", "truncated": False,
            "hits": [hit(canned["files"], s["path"], *s["lines"]) for s in canned.get("spans", [])]}


def body_for(tool: str, args: dict, canned: dict) -> dict:
    files = canned.get("files", {})
    if tool == "airk_search":
        return search_body(canned)
    if tool == "airk_answer":
        records = [{"id": f"r{i}", "operation": "search", "arguments": {"query": args.get("question")},
                    "role": "unknown", "canonical_result": {"collection": "mock", "truncated": False,
                                                            "hits": [hit(files, s["path"], *s["lines"])]}}
                   for i, s in enumerate(canned.get("spans", []))]
        return {"schema": "airk.arbiter.context/2", "question": args.get("question"), "records": records,
                "paragraphs": [{"text": f"See {s['path']}.", "evidence": [f"r{i}"], "interpretation": False}
                               for i, s in enumerate(canned.get("spans", []))],
                "gaps": [], "stop": "complete"}
    if tool == "airk_parse":
        evidence = [dict(hit(files, s["path"], *s["lines"]), step="s1", publication="mock")
                    for s in canned.get("spans", [])]
        return {"schema": "airk.workbench.result/1", "status": "complete", "executed": True,
                "steps": [{"id": "s1", "tool": "search", "arguments": {"query": args.get("text")}, "status": "complete",
                           "result": search_body(canned)}], "evidence": evidence}
    if tool == "airk_read_file":
        path, start, end = args["path"], int(args["start_line"]), int(args["end_line"])
        return {"read": {"status": "content", "requested_path": path}, "collection": "mock", "truncated": False,
                "hits": [hit(files, path, start, end)]}
    if tool == "airk_context":
        path, line = args["path"], int(args["line"])
        before, after = int(args.get("before", 16)), int(args.get("after", 16))
        total = len(files.get(path, "").split("\n"))
        start, end = max(1, line - before), min(total, line + after)
        return {"read": {"status": "content"}, "collection": "mock", "truncated": False,
                "hits": [hit(files, path, start, end)]}
    if tool in ("airk_outline", "airk_symbols"):
        wanted = [s for s in canned.get("symbols", [])
                  if (tool == "airk_outline" and s["path"] == args.get("path"))
                  or (tool == "airk_symbols" and (not args.get("name") or args.get("name") in s["name"]))]
        paths = sorted({s["path"] for s in wanted})
        return {"representation": "minimal", "files": [{"path": p} for p in paths],
                "items": [{"name": s["name"], "kind": s.get("kind", "function"), "lines": s["lines"],
                           "file": paths.index(s["path"])} for s in wanted]}
    if tool == "airk_references":
        name = args.get("name", "")
        hits = []
        for path, text in sorted(files.items()):
            for number, row in enumerate(text.split("\n"), start=1):
                if name and name in row:
                    hits.append({"citation": citation(path, number, number), "text": row, "truncated": False})
        return {"resolution": "lexical", "evidence": {"collection": "mock", "truncated": False, "hits": hits}}
    raise SystemExit(f"mock CLI has no tool {tool}")


def main(argv: list[str] | None = None) -> int:
    parser = argparse.ArgumentParser()
    parser.add_argument("--canned", type=Path, required=True)
    parser.add_argument("--output")
    parser.add_argument("--service-runtime-dir")
    parser.add_argument("verb", nargs=2)
    parser.add_argument("tool")
    parser.add_argument("--index", required=True)
    parser.add_argument("--args", required=True)
    ns = parser.parse_args(argv)
    if ns.verb != ["tools", "call"]:
        sys.stderr.write("mock CLI supports only 'tools call'\n")
        return 2
    canned = json.loads(ns.canned.read_bytes())
    if ns.tool in canned.get("fail", []):
        sys.stdout.write(json.dumps({"content": [{"type": "text", "text": "mock failure"}], "isError": True}))
        return 1
    body = body_for(ns.tool, json.loads(ns.args), canned)
    sys.stdout.write(json.dumps({"content": [{"type": "text", "text": json.dumps(body)}]}))
    return 0


if __name__ == "__main__":
    sys.exit(main())
