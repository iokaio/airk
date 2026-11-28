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
import product_skill  # noqa: E402

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


def command(claude: str, model: str, arm: str, mcp_config: Path, max_round_trips: int, skill: bool = False,
            product: bool = False) -> list[str]:
    allowed = list(FILE_TOOLS) + ([f"mcp__{SERVER}__{PUBLIC_TOOL}"] if arm in ("tool", "oracle", "live") else [])
    skill_name = "airk-eval:gather-evidence"
    plugin = HERE / "airk-eval"
    if product:
        allowed = list(FILE_TOOLS) + [f"mcp__{SERVER}__{name}" for name in tool_names.mcp_tools()]
        skill_name, plugin = product_skill.QUALIFIED_NAME, mcp_config.parent / "airkctl"
    permissions = allowed + ([f"Skill({skill_name})"] if skill else [])
    offered = allowed + (["Skill"] if skill else [])
    denied = [name for name in DENIED if not (skill and name == "Skill")]
    if skill:
        denied.extend(["Skill(doctor)", "Skill(plugin-authoring)"])
    discovery = ["--plugin-dir", str(plugin)] if skill else ["--disable-slash-commands"]
    return [claude, "-p", "--model", model, "--setting-sources", "", "--strict-mcp-config",
            "--mcp-config", str(mcp_config), "--tools", ",".join(offered), "--allowedTools", ",".join(permissions),
            "--disallowedTools", ",".join(denied), "--permission-mode", "dontAsk", *discovery,
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
    parser.add_argument("--live-layout", choices=("full", "focus", "qwen-focus"), default="full")
    parser.add_argument("--live-decline", action="store_true", help="live arm: apply the decline rule first")
    parser.add_argument("--oracle-layout", default="passages", help="oracle arm: passages or ranges")
    parser.add_argument("--oracle-trust", action="store_true", help="oracle arm: add the line saying what need not be read again")
    parser.add_argument("--oracle-complete", action="store_true", help="oracle arm: add the completeness line when every need is covered")
    parser.add_argument("--model", required=True)
    parser.add_argument("--output", required=True, type=Path)
    parser.add_argument("--handle", type=Path)
    parser.add_argument("--repo")
    parser.add_argument("--instructions-file", type=Path, help="server guidance the build gives an MCP client")
    parser.add_argument("--description-file", type=Path, help="serve this tool description in place of the build's own")
    parser.add_argument("--tool-file", type=Path, help="a saved tool listing, in place of asking a running fixture")
    parser.add_argument("--judge", type=Path, help="state file of a running arbiter_judge_service.py (model-dose experiment)")
    parser.add_argument("--dose", type=int, default=0, help="judgments a call the model spends choosing passages")
    parser.add_argument("--max-round-trips", type=int, default=80)
    parser.add_argument("--timeout", type=float, default=2400.0)
    parser.add_argument("--checked-windows", action="store_true", help="preflight and audit Windows session outputs")
    parser.add_argument("--skill-mode", choices=("explicit", "natural"))
    parser.add_argument("--product-tools", action="store_true", help="offer the native seven-tool catalog and use the shipping skill source")
    parser.add_argument("--product-http", action="store_true", help="use the fixture MCP endpoint for both discovery and native calls")
    args = parser.parse_args(argv)
    if args.product_http and (not args.product_tools or args.arm not in ("live", "oracle")):
        parser.error("--product-http needs --product-tools and a live or oracle Gather arm")
    if args.product_tools and (args.arm not in ("tool", "oracle", "live") or not args.handle or args.skill_mode == "explicit"):
        parser.error("--product-tools requires a scoped fixture and a tool arm, with natural discovery or no skill")
    if (args.dose > 0 or (args.arm == "live" and args.live_layout == "qwen-focus")) and args.judge is None:
        parser.error("model scoring needs --judge")
    task = json.loads(args.task.read_text(encoding="utf-8"))
    snapshot = args.snapshot.resolve(strict=True)
    if args.product_tools:
        handle = common.load_handle(args.handle.resolve(strict=True))
        index = handle.index_id(args.repo)
        records = common.read_json(handle.path)["indexes"]
        roots = {Path(row["root"]).resolve(strict=True) for row in records.values() if row["index_id"] == index}
        common.require(roots == {snapshot}, "native fixture index and working snapshot differ")
        common.require(args.tool_file is None and args.description_file is None and args.instructions_file is None, "product tools use the fixture catalog")
        if args.product_http:
            import product_skill_http
            surface = product_skill_http.read_surface(handle, args.repo)
            product_catalog, product_instructions = surface["tools"], surface["instructions"]
        else:
            product_catalog = product_skill.catalog(handle)
            product_instructions = product_skill.guidance()
    work = args.output / "work"
    guard = HERE / "checked_cost_fixture.ps1"
    if args.checked_windows:
        subprocess.run(["pwsh", "-NoProfile", "-File", str(guard), "-Mode", "Prepare",
                        "-Session", str(args.output), "-Snapshot", str(snapshot)], check=True)
    else:
        args.output.mkdir(parents=True)
        shutil.copytree(snapshot, work)
    if args.product_tools and args.skill_mode:
        copy = args.output / "airkctl" / "skills" / "airk-tools" / "SKILL.md"
        if args.checked_windows:
            subprocess.run(["pwsh", "-NoProfile", "-File", str(guard), "-Mode", "StageSkill",
                            "-Session", str(args.output), "-Snapshot", str(product_skill.SKILL)], check=True)
        else:
            copy.parent.mkdir(parents=True)
            shutil.copyfile(product_skill.SKILL, copy)
        common.require(common.sha256_file(copy) == common.sha256_file(product_skill.SKILL), "skill bytes differ")
    servers: dict = {}
    tool = None
    if args.arm in ("tool", "oracle", "live"):
        common.require(bool(args.repo) and (args.handle is not None or (args.tool_file is not None and args.arm != "tool")),
                       "the tool arm needs --handle and --repo; the oracle and live arms need --repo and --handle or --tool-file")
        if args.product_tools:
            tool = dict(next(tool for tool in product_catalog if tool["name"] == PUBLIC_TOOL))
            tool.update(listed_name=PUBLIC_TOOL, description_source="build")
        elif args.tool_file is not None:
            # A saved listing: the oracle and live arms then need no running fixture.
            tool = json.loads(args.tool_file.read_text(encoding="utf-8"))
            tool["description_source"] = args.tool_file.name
        else:
            tool = advertised_tool(common.load_handle(args.handle.resolve(strict=True)))
            tool["description_source"] = "build"
        if args.description_file:
            # A description under test replaces the build's own (tool_descriptions/README.md).
            tool["description"] = args.description_file.read_text(encoding="utf-8").strip()
            tool["description_source"] = args.description_file.name
        tool_file = args.output / "tool.json"
        tool_file.write_text(json.dumps(tool, indent=2, sort_keys=True) + "\n", encoding="utf-8", newline="\n")
        server = [sys.executable, "-B", str(HERE / "gather_context_mcp.py"),
                  "--repo", args.repo, "--tool-file", str(tool_file), "--log-dir", str(args.output / "gather")]
        if args.handle is not None:
            server += ["--handle", str(args.handle.resolve())]
        if args.product_tools:
            args.instructions_file = args.output / "unified-guidance.txt"
            args.instructions_file.write_text(product_instructions, encoding="utf-8")
        if args.instructions_file:
            server += ["--instructions-file", str(args.instructions_file.resolve(strict=True))]
        if args.dose > 0 or (args.arm == "live" and args.live_layout.startswith("qwen-")):
            server += ["--judge", str(args.judge.resolve(strict=True)), "--dose", str(args.dose)]
        if args.arm == "oracle":
            server += ["--oracle-task", str(args.task.resolve()), "--oracle-snapshot", str(snapshot),
                       "--oracle-layout", args.oracle_layout] + (["--oracle-trust"] if args.oracle_trust else [])
            server += ["--oracle-complete"] if args.oracle_complete else []
        if args.arm == "live":
            server += ["--live-source", str(snapshot), "--live-map", str(args.live_map.resolve(strict=True)),
                       "--live-layout", args.live_layout] + (["--live-decline"] if args.live_decline else [])
        if args.product_http:
            surface_file = args.output / "native-surface.json"
            surface_file.write_text(json.dumps(surface, sort_keys=True), encoding="utf-8")
            server += ["--native-surface", str(surface_file)]
        elif args.product_tools:
            server.append("--product-tools")
        servers[SERVER] = {"command": server[0], "args": server[1:]}
    mcp_config = args.output / "mcp-config.json"
    mcp_config.write_text(json.dumps({"mcpServers": servers}, indent=2) + "\n", encoding="utf-8", newline="\n")
    argv_out = command(common.find_claude(), args.model, args.arm, mcp_config, args.max_round_trips, bool(args.skill_mode), args.product_tools)
    env = common.clean_child_env()
    settings = {}
    if args.skill_mode:
        settings["enabledPlugins"] = {"cc-plugin-plugin-authoring@builtin": False}
        settings["skillOverrides"] = {"doctor": "off", "plugin-authoring": "off"}
        env["DISABLE_DOCTOR_COMMAND"] = "1"
    if args.checked_windows:
        hooks = {}
        for event, mode in (("PreToolUse", "BeforeTool"), ("PostToolUse", "AfterTool")):
            cmd = subprocess.list2cmdline(["pwsh", "-NoProfile", "-File", guard.as_posix(), "-Mode", mode, "-Session", args.output.resolve().as_posix()])
            hooks[event] = [{"matcher": "Edit|Write", "hooks": [{"type": "command", "command": cmd}]}]
        settings["hooks"] = hooks
        env.update({name: str(args.output / "tmp") for name in ("TEMP", "TMP", "TMPDIR", "CLAUDE_CODE_TMPDIR")})
    if settings:
        argv_out.extend(["--settings", json.dumps(settings)])
    prompt = task["instruction"]
    if args.skill_mode:
        env["CLAUDE_CODE_DISABLE_BUNDLED_SKILLS"] = "1"
        if args.skill_mode == "explicit":
            prompt = "Invoke the airk-eval:gather-evidence skill for this task.\n\n" + prompt
    started, timed_out = time.monotonic(), False
    with (args.output / "stream.jsonl").open("wb") as stream, (args.output / "stderr.log").open("wb") as errors:
        process = subprocess.Popen(argv_out, cwd=work, stdin=subprocess.PIPE, stdout=stream, stderr=errors,
                                   env=env, creationflags=getattr(subprocess, "CREATE_NO_WINDOW", 0))
        try:
            process.communicate(prompt.encode("utf-8"), timeout=args.timeout)
        except subprocess.TimeoutExpired:
            timed_out = True
            common.kill_tree(process.pid)
            process.wait()
    wall = round(time.monotonic() - started, 3)
    if args.checked_windows:
        subprocess.run(["pwsh", "-NoProfile", "-File", str(guard), "-Mode", "Audit", "-Session", str(args.output)], check=True)
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
    if args.skill_mode:
        session["skill"] = {"mode": args.skill_mode, "sha256": common.sha256_file(
            copy if args.product_tools else HERE / "airk-eval" / "skills" / "gather-evidence" / "SKILL.md")}
    if args.product_http:
        session["native_mcp"] = {"surface_sha256": common.sha256_file(surface_file), "index": index,
                                 "fixture": str(handle.path), "handle_sha256": handle.sha256}
    if args.product_tools:
        expected = {f"mcp__{SERVER}__{name}" for name in tool_names.mcp_tools()}
        offered = {name for name in session["tools_offered"] or [] if name.startswith("mcp__")}
        events = common.read_jsonl(args.output / "stream.jsonl")
        init = next((e for e in events if e.get("subtype") == "init"), {})
        session["product_surface_valid"] = (expected == offered and set(init.get("skills") or []) ==
                                            ({product_skill.QUALIFIED_NAME} if args.skill_mode else set()))
    with (args.output / "session.json").open("x", encoding="utf-8", newline="\n") as file:
        file.write(json.dumps(session, indent=2, sort_keys=True) + "\n")
    print(f"{task.get('repository')}-{task.get('slot')} {args.arm} {args.model} completed={session['completed']} "
          f"round_trips={session['round_trips']} cost=${session['cost_usd']} gather_calls={len(calls)} "
          f"changed={len(changed)} wall={wall}s", flush=True)
    return 1 if args.product_tools and not (session["product_surface_valid"] and session["completed"] and session["exit_code"] == 0) else 0


if __name__ == "__main__":
    sys.exit(main())
