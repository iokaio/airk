# SPDX-License-Identifier: Apache-2.0
"""Run one acting agent on one task, with or without Gather Context, and record what it cost.

Evaluation tooling for the cost comparison of 27 November 2026 (plan
``docs/arbiter/retrieval-strategy-combined.md``, "Strategy from 27 November
2026"). The result of that comparison is the percentage lower model cost
with the tool, guarded by a result that is no less correct.

One session is headless Claude Code working in a fresh private copy of the
repository snapshot, with its ordinary file tools (Read, Grep, Glob, Edit,
Write) and nothing else. In the ``tool`` arm it also has the one Gather
Context tool, served by ``gather_context_mcp.py`` exactly as the build under
test advertises it. No first call is prescribed in either arm, so the
measure includes whether the agent chooses the tool and returns to it.

The session folder gets ``session.json`` (the command, the model, the
agent's own reported usage and cost, round trips, times, tool use by name,
Gather Context calls with seconds and bytes), ``stream.jsonl`` (the
transcript), ``answer.md`` (the final message) and ``changes.patch`` (what
the agent changed in its copy). Correctness is judged afterwards against the
task's frozen truth; this script does not judge.

    python -B agent_cost_run.py --task <task.json> --snapshot <source dir> --arm tool|plain \
        --model claude-sonnet-5-5 --output <new session dir> [--handle <handle.json> --repo <name>]
"""

from __future__ import annotations

import argparse
import difflib
import json
import os
import shutil
import subprocess
import sys
import time
from pathlib import Path

HERE = Path(__file__).resolve().parent
if str(HERE) not in sys.path:
    sys.path.insert(0, str(HERE))

import common  # noqa: E402
import tool_names  # noqa: E402

SCHEMA = "airk.private.agent-cost-session/1"
SERVER = "airk"
FILE_TOOLS = ("Read", "Grep", "Glob", "Edit", "Write")
DENIED = ("Bash", "PowerShell", "MultiEdit", "NotebookEdit", "LS", "WebFetch", "WebSearch", "Agent", "Task",
          "TodoWrite", "Skill", "ToolSearch", "SlashCommand", "BashOutput", "KillShell", "ExitPlanMode", "Monitor")
SYSTEM_PROMPT = """You are a software engineer working in a checkout of a repository. Carry out the task you are given.

- If the task asks for a change, make it by editing the files. Do not run builds, tests or any command; you have no shell.
- If the task asks a question or asks for a document, answer it in your final message, or write the document to a file if the task says where it belongs.
- Work the way you normally would. Stop when the task is done.
- End with a short final message: what you changed or found, citing each location as path:line."""
PUBLIC_TOOL = "gather_context"


def summarize(events: list[dict]) -> dict:
    """What a stream-json transcript says the session cost: the agent's own reported numbers."""
    requests, uses, result, init, final = set(), [], None, None, ""
    named: dict[str, str] = {}
    returned: dict[str, int] = {}
    for event in events:
        kind = event.get("type")
        if kind == "system" and event.get("subtype") == "init" and init is None:
            init = event
        elif kind == "assistant":
            message = event.get("message") or {}
            if message.get("id"):
                requests.add(message["id"])
            for block in message.get("content") or []:
                if block.get("type") == "tool_use":
                    uses.append(block.get("name"))
                    named[block.get("id")] = block.get("name")
                elif block.get("type") == "text" and block.get("text", "").strip():
                    final = block["text"]
        elif kind == "user":
            # What each tool handed back: this is what the agent then carries on every later round trip.
            for block in (event.get("message") or {}).get("content") or []:
                if isinstance(block, dict) and block.get("type") == "tool_result":
                    content = block.get("content")
                    text = content if isinstance(content, str) else "".join(
                        part.get("text", "") for part in content or [] if isinstance(part, dict))
                    name = named.get(block.get("tool_use_id"), "unknown")
                    returned[name] = returned.get(name, 0) + len(text.encode("utf-8"))
        elif kind == "result":
            result = event
    usage = (result or {}).get("usage") or {}
    by_tool: dict[str, int] = {}
    for name in uses:
        by_tool[name] = by_tool.get(name, 0) + 1
    return {
        "completed": bool(result) and result.get("is_error") is not True and result.get("subtype") == "success",
        "result_subtype": (result or {}).get("subtype"),
        # One model request is one round trip: the distinct assistant messages of the transcript.
        "round_trips": len(requests),
        "reported_turns": (result or {}).get("num_turns"),
        "cost_usd": (result or {}).get("total_cost_usd"),
        "tokens": {"input": usage.get("input_tokens"), "cache_read": usage.get("cache_read_input_tokens"),
                   "cache_write": usage.get("cache_creation_input_tokens"), "output": usage.get("output_tokens")},
        "elapsed_ms": (result or {}).get("duration_ms"), "model_ms": (result or {}).get("duration_api_ms"),
        "tool_uses": by_tool, "tool_use_total": len(uses),
        # Bytes each tool returned to the agent, and the share that came from its own file tools.
        "tool_result_bytes": returned,
        "own_read_bytes": sum(size for name, size in returned.items() if name in FILE_TOOLS),
        "tools_offered": (init or {}).get("tools"), "mcp_servers": [s.get("name") for s in (init or {}).get("mcp_servers") or []],
        "permission_denials": len((result or {}).get("permission_denials") or []),
        "final_message": final,
    }


def changes(snapshot: Path, work: Path) -> tuple[str, list[dict]]:
    """A unified diff of what the session changed in its copy, and the changed line ranges by file."""
    patch, rows = [], []
    before = {path.relative_to(snapshot).as_posix(): path for path in snapshot.rglob("*") if path.is_file()}
    after = {path.relative_to(work).as_posix(): path for path in work.rglob("*") if path.is_file()}
    for name in sorted(set(before) | set(after)):
        old = before[name].read_bytes() if name in before else b""
        new = after[name].read_bytes() if name in after else b""
        if old == new:
            continue
        left = old.decode("utf-8", errors="replace").splitlines(keepends=True)
        right = new.decode("utf-8", errors="replace").splitlines(keepends=True)
        patch += list(difflib.unified_diff(left, right, f"a/{name}", f"b/{name}"))
        ranges = [[i1 + 1, max(i2, i1 + 1)] for tag, i1, i2, _, _ in
                  difflib.SequenceMatcher(None, left, right, autojunk=False).get_opcodes() if tag != "equal"]
        rows.append({"path": name, "state": "added" if name not in before else "removed" if name not in after
                     else "modified", "snapshot_lines": ranges})
    return "".join(line if line.endswith("\n") else line + "\n" for line in patch), rows


def command(claude: str, model: str, arm: str, mcp_config: Path, max_round_trips: int) -> list[str]:
    allowed = list(FILE_TOOLS) + ([f"mcp__{SERVER}__{PUBLIC_TOOL}"] if arm in ("tool", "oracle", "live") else [])
    return [claude, "-p", "--model", model, "--setting-sources", "", "--strict-mcp-config",
            "--mcp-config", str(mcp_config), "--tools", ",".join(allowed), "--allowedTools", ",".join(allowed),
            "--disallowedTools", ",".join(DENIED), "--permission-mode", "dontAsk", "--disable-slash-commands",
            "--no-session-persistence", "--system-prompt", SYSTEM_PROMPT, "--max-turns", str(max_round_trips),
            "--output-format", "stream-json", "--verbose"]


def advertised_tool(handle: common.Handle, timeout: float = 120.0) -> dict:
    """The Gather Context tool as the build under test lists it: name, description and input schema."""
    listed = subprocess.run([*handle.cli, "--output", "json", "--service-runtime-dir", handle.control, "tools", "list"],
                            capture_output=True, env=common.tool_env(handle), timeout=timeout)
    document = json.loads(listed.stdout)
    tools = document.get("tools", document) if isinstance(document, dict) else document
    found = [tool for tool in tools if tool_names.native(tool.get("name", "")) == "airk_answer"]
    common.require(len(found) == 1, "the build does not list exactly one Gather Context tool")
    return {"name": PUBLIC_TOOL, "description": found[0].get("description", ""), "inputSchema": found[0]["inputSchema"],
            "listed_name": found[0]["name"]}


def main(argv: list[str] | None = None) -> int:
    parser = argparse.ArgumentParser(description=__doc__.splitlines()[0])
    parser.add_argument("--task", required=True, type=Path)
    parser.add_argument("--snapshot", required=True, type=Path)
    parser.add_argument("--arm", required=True, choices=("tool", "plain", "oracle", "live"),
                        help="oracle: the tool answers with the task's truth (gather_context_oracle.py); "
                             "live: it answers from the live files and --live-map (live_gather.py)")
    parser.add_argument("--live-map", type=Path, help="topical map of the snapshot, for the live arm")
    parser.add_argument("--live-layout", default="full", help="live arm: full or focus")
    parser.add_argument("--oracle-layout", default="passages", help="oracle arm: passages or ranges")
    parser.add_argument("--oracle-trust", action="store_true", help="oracle arm: add the line saying what need not be read again")
    parser.add_argument("--model", required=True)
    parser.add_argument("--output", required=True, type=Path)
    parser.add_argument("--handle", type=Path)
    parser.add_argument("--repo")
    parser.add_argument("--instructions-file", type=Path, help="server guidance the build gives an MCP client")
    parser.add_argument("--description-file", type=Path, help="serve this tool description in place of the build's own")
    parser.add_argument("--judge", type=Path, help="state file of a running arbiter_judge_service.py (model-dose experiment)")
    parser.add_argument("--dose", type=int, default=0, help="judgments a call the model spends choosing passages")
    parser.add_argument("--max-round-trips", type=int, default=80)
    parser.add_argument("--timeout", type=float, default=2400.0)
    args = parser.parse_args(argv)
    task = json.loads(args.task.read_text(encoding="utf-8"))
    snapshot = args.snapshot.resolve(strict=True)
    args.output.mkdir(parents=True)
    work = args.output / "work"
    shutil.copytree(snapshot, work)
    servers: dict = {}
    tool = None
    if args.arm in ("tool", "oracle", "live"):
        common.require(args.handle is not None and args.repo, "the tool and oracle arms need --handle and --repo")
        handle = common.load_handle(args.handle.resolve(strict=True))
        tool = advertised_tool(handle)
        tool["description_source"] = "build"
        if args.description_file:
            # A description under test replaces the build's own (tool_descriptions/README.md).
            tool["description"] = args.description_file.read_text(encoding="utf-8").strip()
            tool["description_source"] = args.description_file.name
        tool_file = args.output / "tool.json"
        tool_file.write_text(json.dumps(tool, indent=2, sort_keys=True) + "\n", encoding="utf-8", newline="\n")
        server = [sys.executable, "-B", str(HERE / "gather_context_mcp.py"), "--handle", str(args.handle.resolve()),
                  "--repo", args.repo, "--tool-file", str(tool_file), "--log-dir", str(args.output / "gather")]
        if args.instructions_file:
            server += ["--instructions-file", str(args.instructions_file.resolve(strict=True))]
        if args.dose > 0:
            server += ["--judge", str(args.judge.resolve(strict=True)), "--dose", str(args.dose)]
        if args.arm == "oracle":
            server += ["--oracle-task", str(args.task.resolve()), "--oracle-snapshot", str(snapshot),
                       "--oracle-layout", args.oracle_layout] + (["--oracle-trust"] if args.oracle_trust else [])
        if args.arm == "live":
            server += ["--live-source", str(snapshot), "--live-map", str(args.live_map.resolve(strict=True)),
                       "--live-layout", args.live_layout]
        servers[SERVER] = {"command": server[0], "args": server[1:]}
    mcp_config = args.output / "mcp-config.json"
    mcp_config.write_text(json.dumps({"mcpServers": servers}, indent=2) + "\n", encoding="utf-8", newline="\n")
    argv_out = command(common.find_claude(), args.model, args.arm, mcp_config, args.max_round_trips)
    started, timed_out = time.monotonic(), False
    with (args.output / "stream.jsonl").open("wb") as stream, (args.output / "stderr.log").open("wb") as errors:
        process = subprocess.Popen(argv_out, cwd=work, stdin=subprocess.PIPE, stdout=stream, stderr=errors,
                                   env=common.clean_child_env(), creationflags=getattr(subprocess, "CREATE_NO_WINDOW", 0))
        try:
            process.communicate(task["instruction"].encode("utf-8"), timeout=args.timeout)
        except subprocess.TimeoutExpired:
            timed_out = True
            common.kill_tree(process.pid)
            process.wait()
    wall = round(time.monotonic() - started, 3)
    summary = summarize(common.read_jsonl(args.output / "stream.jsonl"))
    (args.output / "answer.md").write_text(summary.pop("final_message"), encoding="utf-8", newline="\n")
    patch, changed = changes(snapshot, work)
    (args.output / "changes.patch").write_text(patch, encoding="utf-8", newline="\n")
    shutil.rmtree(work)  # the copy is disposable; the patch is what the session did to it
    calls = common.read_jsonl(args.output / "gather" / "calls.jsonl") if (args.output / "gather" / "calls.jsonl").is_file() else []
    session = {"schema": SCHEMA, "at": common.now_iso(), "task": {"path": str(args.task), "sha256": common.sha256_file(args.task),
                                                                 "repository": task.get("repository"), "slot": task.get("slot")},
               "arm": args.arm, "model": args.model, "claude": common.claude_version(argv_out[0]), "command": argv_out,
               "exit_code": process.returncode, "timed_out": timed_out, "wall_seconds": wall, **summary,
               "gather_context": {"calls": len(calls), "seconds": round(sum(row["seconds"] for row in calls), 3),
                                  "reply_bytes": sum(row["reply_bytes"] for row in calls),
                                  "errors": sum(bool(row["is_error"]) for row in calls),
                                  "dose": args.dose,
                                  "judgments": sum((row.get("dose") or {}).get("judgments") or 0 for row in calls),
                                  "dose_applied": sum(bool((row.get("dose") or {}).get("applied")) for row in calls),
                                  "advertised": None if tool is None else {"listed_name": tool["listed_name"],
                                                                           "description_bytes": len(tool["description"]),
                                                                           "description_source": tool["description_source"],
                                                                           "description_sha256": common.sha256_bytes(
                                                                               tool["description"].encode("utf-8")),
                                                                           "guidance": args.instructions_file.name if args.instructions_file else None}},
               "changed_files": changed}
    with (args.output / "session.json").open("x", encoding="utf-8", newline="\n") as file:
        file.write(json.dumps(session, indent=2, sort_keys=True) + "\n")
    print(f"{task.get('repository')}-{task.get('slot')} {args.arm} {args.model} completed={session['completed']} "
          f"round_trips={session['round_trips']} cost=${session['cost_usd']} gather_calls={len(calls)} "
          f"changed={len(changed)} wall={wall}s", flush=True)
    return 0


if __name__ == "__main__":
    sys.exit(main())
