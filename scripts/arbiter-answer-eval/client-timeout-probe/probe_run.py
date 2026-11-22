# SPDX-License-Identifier: Apache-2.0
"""Run headless Claude Code against probe_server.py for one long tool call.

The Claude Code binary and the child environment are the evaluation
harness's (``common.find_claude`` and ``common.clean_child_env``), so the
result describes the client the harness uses. One run writes, under
``--out``: ``mcp-config.json``, ``command.json``, ``stream.jsonl``,
``stderr.txt``, ``server.jsonl`` (every MCP message, from the server's side)
and ``summary.json`` (what the client did and what the agent saw).
"""

from __future__ import annotations

import argparse
import json
import subprocess
import sys
import time
from datetime import datetime, timezone
from pathlib import Path

HERE = Path(__file__).resolve().parent
sys.path.insert(0, str(HERE.parent))

import common  # noqa: E402

PROMPT = ("Call the tool mcp__probe__hold exactly once with seconds={seconds}. Do not call any other tool and do not "
          "retry. When it finishes, reply with one line: the exact tool result text, or the exact error or status "
          "message you saw instead.")


def summarize(out: Path, seconds: float, elapsed_s: float, exit_code: int | None, timed_out: bool) -> dict:
    rows = [json.loads(line) for line in (out / "server.jsonl").read_text(encoding="utf-8").splitlines() if line]
    calls = [r for r in rows if r["dir"] == "in" and r["msg"].get("method") == "tools/call"]
    replies = [r for r in rows if r["dir"] == "out" and "result" in r["msg"]
               and any(r["msg"].get("id") == c["msg"].get("id") for c in calls)]
    progress = [r for r in rows if r["dir"] == "out" and r["msg"].get("method") == "notifications/progress"]
    cancels = [r for r in rows if r["dir"] == "in" and r["msg"].get("method") == "notifications/cancelled"]
    closed = [r for r in rows if r["dir"] == "note" and r["msg"].get("stdin_closed")]
    init = next((r["msg"] for r in rows if r["dir"] == "in" and r["msg"].get("method") == "initialize"), None)
    stream = common.parse_stream(out / "stream.jsonl")
    tool_results = []
    for line in (out / "stream.jsonl").read_text(encoding="utf-8", errors="replace").splitlines():
        try:
            event = json.loads(line)
        except json.JSONDecodeError:
            continue
        content = ((event.get("message") or {}).get("content")) if event.get("type") == "user" else None
        for block in content or []:
            if isinstance(block, dict) and block.get("type") == "tool_result":
                tool_results.append({"is_error": block.get("is_error"), "content": block.get("content")})
    result = stream.get("result") or {}
    return {
        "schema": "airk.private.client-timeout-probe/1", "hold_seconds": seconds,
        "transport": json.loads((out / "mcp-config.json").read_text(encoding="utf-8"))["mcpServers"]["probe"]["type"],
        "session_elapsed_s": round(elapsed_s, 1), "exit_code": exit_code, "session_timed_out": timed_out,
        "initialize_params": (init or {}).get("params"),
        "tool_calls": [{"t_s": c["t_s"], "id": c["msg"].get("id"),
                        "progress_token": ((c["msg"].get("params") or {}).get("_meta") or {}).get("progressToken"),
                        "task_param": (c["msg"].get("params") or {}).get("task")} for c in calls],
        "progress_sent": len(progress), "server_replies": [{"t_s": r["t_s"], "id": r["msg"]["id"]} for r in replies],
        "cancellations": [{"t_s": r["t_s"], "params": r["msg"].get("params")} for r in cancels],
        "stdin_closed_t_s": closed[0]["t_s"] if closed else None,
        "agent_tool_results": tool_results,
        "final_text": result.get("result"), "num_turns": result.get("num_turns"),
        "total_cost_usd": result.get("total_cost_usd"), "is_error": result.get("is_error"),
    }


def main() -> int:
    parser = argparse.ArgumentParser(description=__doc__.splitlines()[0])
    parser.add_argument("--seconds", type=float, required=True)
    parser.add_argument("--out", type=Path, required=True, help="new, empty receipt folder")
    parser.add_argument("--model", default=common.AGENT_MODEL)
    parser.add_argument("--effort", default="low", choices=common.EFFORTS)
    parser.add_argument("--progress-interval", type=float, default=10.0)
    parser.add_argument("--claude", help="Claude Code executable (default: common.find_claude())")
    parser.add_argument("--session-timeout", type=float, help="kill the session after this many seconds")
    parser.add_argument("--env", action="append", default=[], help="extra NAME=VALUE for the Claude Code child")
    parser.add_argument("--http", type=int, metavar="PORT", help="use streamable HTTP on this local port, not stdio")
    args = parser.parse_args()
    out: Path = args.out
    out.mkdir(parents=True, exist_ok=False)
    claude = args.claude or common.find_claude()
    server = [sys.executable, "-I", str(HERE / "probe_server.py"), "--log", str(out / "server.jsonl"),
              "--progress-interval", str(args.progress_interval)]
    (out / "server.jsonl").write_text("", encoding="utf-8")
    http_server = None
    if args.http:
        server += ["--http", str(args.http)]
        http_server = subprocess.Popen(server, stdout=subprocess.DEVNULL, stderr=(out / "server-stderr.txt").open("wb"),
                                       creationflags=getattr(subprocess, "CREATE_NO_WINDOW", 0))
        time.sleep(1.0)  # the listener binds at once; a failed bind shows in server-stderr.txt
        config = {"mcpServers": {"probe": {"type": "http", "url": f"http://127.0.0.1:{args.http}/mcp"}}}
    else:
        config = {"mcpServers": {"probe": {"type": "stdio", "command": server[0], "args": server[1:], "env": {}}}}
    (out / "mcp-config.json").write_text(json.dumps(config, indent=1), encoding="utf-8")
    tool = "mcp__probe__hold"
    command = [claude, "-p", "--model", args.model, "--effort", args.effort, "--setting-sources", "",
               "--strict-mcp-config", "--mcp-config", str(out / "mcp-config.json"), "--tools", tool,
               "--allowedTools", tool, "--permission-mode", "dontAsk", "--disable-slash-commands",
               "--no-session-persistence", "--output-format", "stream-json", "--verbose"]
    extra = dict(item.split("=", 1) for item in args.env)
    env = common.clean_child_env(extra)
    prompt = PROMPT.format(seconds=f"{args.seconds:g}")
    (out / "command.json").write_text(json.dumps({
        "command": command, "prompt": prompt, "claude_version": common.claude_version(claude), "extra_env": extra,
        "inherited_timeout_env": {k: v for k, v in env.items() if "TIMEOUT" in k.upper() or "MCP" in k.upper()},
        "started_utc": datetime.now(timezone.utc).isoformat()}, indent=1), encoding="utf-8")
    began = time.monotonic()
    with (out / "stream.jsonl").open("wb") as stream, (out / "stderr.txt").open("wb") as err:
        proc = subprocess.Popen(command, cwd=out, stdin=subprocess.PIPE, stdout=stream, stderr=err, env=env,
                                creationflags=getattr(subprocess, "CREATE_NO_WINDOW", 0))
        proc.stdin.write(prompt.encode("utf-8"))
        proc.stdin.close()
        try:
            exit_code, timed_out = proc.wait(timeout=args.session_timeout), False
        except subprocess.TimeoutExpired:
            common.kill_tree(proc.pid)
            exit_code, timed_out = proc.wait(timeout=60), True
    if http_server is not None:
        common.kill_tree(http_server.pid)
        http_server.wait(timeout=60)
    summary = summarize(out, args.seconds, time.monotonic() - began, exit_code, timed_out)
    (out / "summary.json").write_text(json.dumps(summary, indent=1), encoding="utf-8")
    print(json.dumps(summary, indent=1))
    return 0


if __name__ == "__main__":
    raise SystemExit(main())
