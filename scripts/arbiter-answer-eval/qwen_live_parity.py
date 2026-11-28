# SPDX-License-Identifier: Apache-2.0
"""Compare a real stdio Qwen tool reply with the frozen replay for a recorded task."""

import argparse
from collections import defaultdict
import json
from pathlib import Path
import subprocess
import sys
from unittest.mock import patch

import agent_cost_batch
import arbiter_judge_service
import live_gather as gather
import qwen_file_trial as trial


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("campaign", type=Path)
    parser.add_argument("state", type=Path)
    parser.add_argument("task")
    parser.add_argument("logs", type=Path)
    args = parser.parse_args()
    c = args.campaign
    address = json.loads(args.state.read_text(encoding="utf-8"))["address"]
    calls = json.loads((c / "replay/first-calls-r1.json").read_text(encoding="utf-8"))["calls"]
    saved = next(r for line in (c / "replay/qwen-file-top40-r1.jsonl").read_text().splitlines()[1:]
                 if (r := json.loads(line))["task"] == args.task)
    row = next(r for r in agent_cost_batch.working_set(c) if r["id"] == args.task)
    src = row["snapshot"]
    maps = c / "topical" / f"{src.name}.map.json"
    repo = gather.Repository(src, json.loads(maps.read_text(encoding="utf-8")))
    arguments = calls[args.task]
    wide = {}
    with patch.object(gather, "TOP_FILES", trial.WIDTH):
        gather.gather(repo, arguments, wide)
    paths, scores = zip(*saved["scores"])
    assert list(paths) == [p for p, _ in wide["ranked"][:trial.WIDTH]]
    locations = defaultdict(list)
    for r in wide["all_locations"]:
        locations[r["at"].rpartition(":")[0]].append(r["at"])
    query = "\n".join([arguments["task"], *arguments.get("needs", []), *arguments.get("anchors", [])])
    current = arbiter_judge_service.score(address, trial.INSTRUCTION,
                                          [(query, trial.document(repo, p, locations[p])) for p in paths])
    order = trial.reorder(paths, current)
    assert order == trial.reorder(paths, scores)
    trace = {}
    brief = gather.gather(repo, arguments, trace, rank_rule=lambda ranked, *_: order + ranked[len(paths):])
    expected = trial.focused(repo, brief, trace)
    here = Path(__file__).resolve().parent
    cmd = [sys.executable, "-B", str(here / "gather_context_mcp.py"), "--repo", row["index"],
           "--tool-file", str(here / "tool_descriptions/gather_context.tool.json"), "--log-dir", str(args.logs),
           "--live-source", str(src), "--live-map", str(maps), "--live-layout", "qwen-focus", "--judge", str(args.state)]
    call = {"jsonrpc": "2.0", "id": 1, "method": "tools/call", "params": {"name": "gather_context", "arguments": arguments}}
    result = subprocess.run(cmd, input=(json.dumps(call) + "\n").encode(), capture_output=True, check=True, timeout=330)
    outcome = json.loads(result.stdout)["result"]
    if outcome["isError"]:
        raise RuntimeError("MCP returned an error")
    actual = json.loads(outcome["content"][0]["text"])
    actual.pop("elapsed_ms"); expected.pop("elapsed_ms")
    assert actual == expected, "Live and replay replies differ"
    print(json.dumps({"task": args.task, "file_order": len(order), "passages": len(actual["passages"]),
                      "bytes": sum(len(p["text"].encode()) for p in actual["passages"])}))


if __name__ == "__main__":
    main()
