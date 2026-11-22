# SPDX-License-Identifier: Apache-2.0
"""One real call per tool through the wrapper against a live handle, to check result shapes.

For each repository: the three primary tools (airk_answer, airk_search,
airk_parse) with one question, then the five single reads aimed at the first
evidence location returned. Each call goes through ``mcp_wrapper.Wrapper``
with the real CLI backend, so the turn log, raw results, byte counts and
timing are the wrapper's own. The report lists, per call, the spans that
``evidence.extract`` found by kind, so a shape the extractor misses shows up
as a call with bytes and no spans.

``python -I -B probe_tools.py --handle H --out DIR [--accept-queryable] [--question Q]``
"""

from __future__ import annotations

import argparse
import collections
import json
import sys
from pathlib import Path

HERE = Path(__file__).resolve().parent
if str(HERE) not in sys.path:
    sys.path.insert(0, str(HERE))

import common  # noqa: E402
import evidence  # noqa: E402
import mcp_wrapper  # noqa: E402

QUESTION = "Where is the configuration validated, and which tests cover it?"


def summarize(row: dict) -> dict:
    body = common.raw_body(Path(row["result_path"]).read_bytes()) if row.get("result_path") else None
    spans = evidence.extract(body) if body is not None else []
    kinds = collections.Counter(s.kind for s in spans)
    first = next((s for s in spans if s.kind == "text" and s.start), None)
    return {"turn": row["turn"], "tool": row["tool"], "ok": row["ok"], "bytes": row["bytes"],
            "elapsed_ms": row["elapsed_ms"], "spans": dict(kinds),
            "first_text_span": None if first is None else [first.path, first.start, first.end],
            "top_keys": sorted(body)[:30] if isinstance(body, dict) else type(body).__name__,
            "error": row.get("error")}


def probe(handle: common.Handle, catalog: dict, repo: str, out: Path, question: str) -> list[dict]:
    report = []
    wrappers = {}
    location = None
    for path in ("answer", "search", "parse"):
        wrapper = mcp_wrapper.Wrapper(handle=handle, repo=repo, path=path, instruction=question, catalog=catalog,
                                      log_dir=out / repo / path)
        wrappers[path] = wrapper
        field = common.PRIMARY_ARG[wrapper.primary]
        wrapper.call(wrapper.primary, {field: question})
        rows = common.read_jsonl(out / repo / path / "turns.jsonl")
        first = summarize(rows[-1])
        report.append({"repo": repo, "path": path, **first})
        location = location or first["first_text_span"]
    if location:
        # The reads go through the answer path's wrapper (its counted first call allows them, even when that
        # call failed), aimed at the first evidence location any primary call returned.
        wrapper, path = wrappers["answer"], "answer"
        file, start, end = location
        symbol_rows = None
        for tool, args in (("airk_read_file", {"path": file, "start_line": start, "end_line": max(start, end)}),
                           ("airk_context", {"path": file, "line": start}),
                           ("airk_outline", {"path": file}),
                           ("airk_symbols", {"path": file, "limit": 5})):
            wrapper.call(tool, args)
            row = summarize(common.read_jsonl(out / repo / path / "turns.jsonl")[-1])
            report.append({"repo": repo, "path": path, **row})
            if tool == "airk_symbols":
                symbol_rows = common.raw_body(Path(common.read_jsonl(out / repo / path / "turns.jsonl")[-1]
                                                   ["result_path"]).read_bytes())
        name = None
        if isinstance(symbol_rows, dict):
            for item in symbol_rows.get("items") or []:
                if isinstance(item, dict) and isinstance(item.get("name"), str):
                    name = item["name"].split("::")[-1]
                    break
        wrapper.call("airk_references", {"name": name or "main", "limit": 5})
        report.append({"repo": repo, "path": path, "reference_name": name or "main",
                       **summarize(common.read_jsonl(out / repo / path / "turns.jsonl")[-1])})
    return report


def main(argv: list[str] | None = None) -> int:
    parser = argparse.ArgumentParser(description=__doc__.splitlines()[0])
    parser.add_argument("--handle", type=Path, required=True)
    parser.add_argument("--catalog", type=Path)
    parser.add_argument("--out", type=Path, required=True)
    parser.add_argument("--repos", default=",".join(common.REPOS))
    parser.add_argument("--question", default=QUESTION)
    parser.add_argument("--accept-queryable", action="store_true")
    args = parser.parse_args(argv)
    handle = common.load_handle(args.handle, accept_queryable=args.accept_queryable)
    catalog = common.load_catalog(args.catalog or args.handle.resolve().parent.parent / "tools-catalog.json")
    report = {"generated": common.now_iso(), "handle": str(args.handle), "handle_sha256": handle.sha256,
              "readiness": handle.readiness(), "calls": []}
    for repo in args.repos.split(","):
        report["calls"] += probe(handle, catalog, repo, args.out, args.question)
    common.write_json(args.out / "probe.json", report)
    for row in report["calls"]:
        print(json.dumps({k: row.get(k) for k in ("repo", "path", "tool", "ok", "bytes", "elapsed_ms", "spans",
                                                   "error")}))
    return 0


if __name__ == "__main__":
    sys.exit(main())
