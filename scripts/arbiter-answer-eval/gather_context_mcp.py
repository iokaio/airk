# SPDX-License-Identifier: Apache-2.0
"""MCP stdio server that offers an acting agent the one Gather Context tool.

Evaluation tooling for the cost comparison of 27 November 2026 (plan
``docs/arbiter/retrieval-strategy-combined.md``, "Strategy from 27 November
2026"). It serves the tool exactly as the build under test advertises it:
the public name, description and input schema come from that build's own
tool listing, saved by the runner as ``--tool-file``. It prescribes nothing:
no first call, no argument is forced, no call limit beyond ``--max-calls``.

Each ``tools/call`` becomes one CLI call against the evaluation fixture, by
native name, in the service's default (terse) response mode, with the index
of ``--repo``. Every call is logged to ``<log-dir>/calls.jsonl`` with its
arguments, seconds and reply bytes, and the raw reply is kept beside it.

    python -B gather_context_mcp.py --handle <handle.json> --repo munarium \
        --tool-file <tool.json> --log-dir <new dir> [--instructions-file <text>]

With ``--judge <state.json> --dose N`` (model-dose experiment) a reranker at
a running ``arbiter_judge_service.py`` chooses the brief's passages with N
judgments a call (``gather_context_dose.py``). The server then makes the
same call a second time in full mode to read the candidates; the service
answers it from the finished job. What was spent is logged with the call.
"""

from __future__ import annotations

import argparse
import json
import sys
import time
from pathlib import Path
from typing import Any

HERE = Path(__file__).resolve().parent
if str(HERE) not in sys.path:
    sys.path.insert(0, str(HERE))

import arbiter_judge_service  # noqa: E402
import common  # noqa: E402
import gather_context_dose  # noqa: E402
import gather_context_oracle  # noqa: E402
import mcp_wrapper  # noqa: E402
import tool_names  # noqa: E402

SERVER_INFO = {"name": "airk", "version": "1"}


class Server:
    """One advertised tool, forwarded unchanged. ``backend`` is injectable for tests."""

    def __init__(self, *, tool: dict, repo: str, log_dir: Path, backend, instructions: str | None = None,
                 max_calls: int = 64, score=None, dose: int = 0) -> None:
        common.require(isinstance(tool.get("name"), str) and isinstance(tool.get("inputSchema"), dict),
                       "the tool file needs a name and an inputSchema")
        self.tool, self.repo, self.backend = tool, repo, backend
        self.instructions, self.max_calls = instructions, max_calls
        self.score, self.dose = score, dose
        self.log_dir = Path(log_dir)
        self.log_dir.mkdir(parents=True, exist_ok=True)
        self.calls = 0

    def list_tools(self) -> list[dict]:
        return [{key: self.tool[key] for key in ("name", "description", "inputSchema") if key in self.tool}]

    def call(self, name: str, arguments: Any) -> dict:
        if name != self.tool["name"] or not isinstance(arguments, dict):
            return {"content": [{"type": "text", "text": f"unknown tool or arguments: {name}"}], "isError": True}
        if self.calls >= self.max_calls:
            return {"content": [{"type": "text", "text": "call limit reached"}], "isError": True}
        self.calls += 1
        started = time.monotonic()
        text, is_error, raw = self.backend(tool_names.native(name), self.repo, arguments)
        (self.log_dir / f"call-{self.calls:03}.reply.json").write_bytes(raw)
        spent = None
        if self.score is not None and self.dose > 0 and not is_error:
            text, spent = self.choose(name, arguments, text)
        seconds = round(time.monotonic() - started, 3)
        common.append_jsonl(self.log_dir / "calls.jsonl", {
            "call": self.calls, "at": common.now_iso(), "arguments": arguments, "seconds": seconds,
            "reply_bytes": len(text.encode("utf-8")), "is_error": is_error, **({"dose": spent} if spent else {})})
        return {"content": [{"type": "text", "text": text}], "isError": is_error}

    def choose(self, name: str, arguments: dict, text: str) -> tuple[str, dict]:
        """Have the model choose the brief's passages. Any fault leaves the service's own brief in place."""
        try:
            brief = json.loads(text)
            if not isinstance(brief, dict) or brief.get("schema") != gather_context_dose.BRIEF_SCHEMA:
                return text, {"applied": False, "why": "not a brief"}
            full_text, failed, raw = self.backend(tool_names.native(name), self.repo, {**arguments, "response_mode": "full"})
            (self.log_dir / f"call-{self.calls:03}.full.json").write_bytes(raw)
            if failed:
                return text, {"applied": False, "why": "the full reply failed"}
            full = json.loads(full_text)
            chosen, spent = gather_context_dose.choose(brief, full, self.score, self.dose)
            spent["full_reused"] = (full.get("delivery") or {}).get("reused")
            if not spent.get("applied"):
                return text, spent
            return json.dumps(chosen, ensure_ascii=False, separators=(",", ":")), spent
        except Exception as error:  # noqa: BLE001 - the agent must still get the service's reply
            return text, {"applied": False, "why": f"{type(error).__name__}: {error}"[:300]}

    def handle_message(self, message: Any) -> dict | None:
        if not isinstance(message, dict) or message.get("jsonrpc") != "2.0":
            return mcp_wrapper._error(None, -32600, "invalid request")
        method, msg_id, params = message.get("method"), message.get("id"), message.get("params") or {}
        if method is None or "id" not in message:
            return None
        if method == "initialize":
            requested = params.get("protocolVersion")
            version = requested if requested in mcp_wrapper.PROTOCOL_VERSIONS else mcp_wrapper.PROTOCOL_VERSIONS[0]
            result = {"protocolVersion": version, "capabilities": {"tools": {"listChanged": False}},
                      "serverInfo": SERVER_INFO}
            if self.instructions:
                result["instructions"] = self.instructions
            return mcp_wrapper._result(msg_id, result)
        if method == "ping":
            return mcp_wrapper._result(msg_id, {})
        if method == "tools/list":
            return mcp_wrapper._result(msg_id, {"tools": self.list_tools()})
        if method == "tools/call":
            return mcp_wrapper._result(msg_id, self.call(params.get("name"), params.get("arguments", {})))
        return mcp_wrapper._error(msg_id, -32601, f"method not found: {method}")


def fixture_backend(handle: common.Handle, timeout: float):
    """Call the fixture through its CLI, in the service's own default response mode."""
    def call(native: str, repo: str, arguments: dict) -> tuple[str, bool, bytes]:
        outcome = common.call_tool(handle, native, repo, arguments, timeout, strip_env=common.AGENT_STRIPPED_ENV)
        text = common.envelope_text(outcome.envelope) or (outcome.error or "the tool call failed")
        return text, not outcome.ok, outcome.stdout or b""
    return call


def oracle_backend(task: dict, snapshot: Path, layout: str = "passages", trust: bool = False, complete: bool = False):
    """Answer every call with the oracle brief of ``task`` (``gather_context_oracle.py``); no service is called."""
    def call(native: str, repo: str, arguments: dict) -> tuple[str, bool, bytes]:
        text = json.dumps(gather_context_oracle.brief(task, snapshot, arguments, layout, trust, complete), ensure_ascii=False,
                          separators=(",", ":"))
        return text, False, text.encode("utf-8")
    return call


def live_backend(source: Path, map_file: Path, layout: str = "full", decline: bool = False, file_score=None):
    """Answer every call from the live files and the topical map (``live_gather.py``); no service is called.

    With ``decline`` the approved decline rule (``gather_decline.py``) is applied first, and a declined
    call is answered with the refusal, as a tool error.
    """
    import gather_decline
    import live_gather
    import live_gather_v3
    index = json.loads(map_file.read_text(encoding="utf-8"))
    repository = live_gather.Repository(source, index)
    # The map says which version built it; the third version's map is answered by the third version's gather step.
    gather = live_gather_v3.gather if index.get("schema") == live_gather_v3.SCHEMA else live_gather.gather

    def call(native: str, repo: str, arguments: dict) -> tuple[str, bool, bytes]:
        if decline:
            seen = gather_decline.features(repository, arguments)
            declined, _ = gather_decline.decide(seen)
            if declined:
                text = json.dumps({"error": {"code": "gather_context_declined",
                                             "message": gather_decline.refusal(seen)}},
                                  ensure_ascii=False, separators=(",", ":"))
                return text, True, text.encode("utf-8")
        if layout == "qwen-focus":
            import qwen_file_trial
            common.require(file_score is not None and gather is live_gather.gather, "Qwen file ranking requires the original topical map and a scorer")
            brief = qwen_file_trial.live_reply(repository, arguments, file_score)
        else:
            brief = gather(repository, arguments)
        if layout == "focus":
            brief = live_gather.focus(repository, brief)
        text = json.dumps(brief, ensure_ascii=False, separators=(",", ":"))
        return text, False, text.encode("utf-8")
    return call


def main(argv: list[str] | None = None) -> int:
    parser = argparse.ArgumentParser(description=__doc__.splitlines()[0])
    parser.add_argument("--handle", type=Path, help="fixture lifetime; not needed when the oracle or live files answer")
    parser.add_argument("--repo", required=True)
    parser.add_argument("--tool-file", type=Path, required=True)
    parser.add_argument("--log-dir", type=Path, required=True)
    parser.add_argument("--instructions-file", type=Path)
    parser.add_argument("--max-calls", type=int, default=64)
    parser.add_argument("--call-timeout", type=float, default=330.0)
    parser.add_argument("--oracle-task", type=Path, help="serve the oracle brief of this task instead of calling the service")
    parser.add_argument("--oracle-snapshot", type=Path)
    parser.add_argument("--oracle-layout", choices=gather_context_oracle.LAYOUTS, default="passages")
    parser.add_argument("--oracle-trust", action="store_true")
    parser.add_argument("--oracle-complete", action="store_true")
    parser.add_argument("--live-source", type=Path, help="answer from these live files and --live-map instead of the service")
    parser.add_argument("--live-map", type=Path)
    parser.add_argument("--live-layout", choices=("full", "focus", "qwen-focus"), default="full")
    parser.add_argument("--live-decline", action="store_true")
    parser.add_argument("--judge", type=Path, help="state file of a running arbiter_judge_service.py")
    parser.add_argument("--dose", type=int, default=0, help="judgments a call the model spends choosing passages")
    parser.add_argument("--product-tools", action="store_true")
    args = parser.parse_args(argv)
    score = None
    if args.dose > 0 or args.live_layout.startswith("qwen-"):
        common.require(args.judge is not None, "model scoring needs --judge")
        address = json.loads(args.judge.read_text(encoding="utf-8"))["address"]
        score = lambda instruction, pairs: arbiter_judge_service.score(address, instruction, pairs)  # noqa: E731
    common.require(args.handle is not None or args.oracle_task is not None or args.live_source is not None,
                   "--handle is needed unless --oracle-task or --live-source answers the calls")
    handle = common.load_handle(args.handle.resolve(strict=True)) if args.handle is not None else None
    tool = json.loads(args.tool_file.read_text(encoding="utf-8"))
    instructions = args.instructions_file.read_text(encoding="utf-8").strip() if args.instructions_file else None
    server = Server(tool=tool, repo=args.repo, log_dir=args.log_dir, instructions=instructions,
                    backend=(oracle_backend(json.loads(args.oracle_task.read_text(encoding="utf-8")), args.oracle_snapshot,
                                            args.oracle_layout, args.oracle_trust, args.oracle_complete)
                             if args.oracle_task else live_backend(args.live_source, args.live_map, args.live_layout, args.live_decline, score) if args.live_source
                             else fixture_backend(handle, args.call_timeout)), max_calls=args.max_calls,
                    score=score, dose=args.dose)
    if args.product_tools:
        import product_skill
        common.require(handle is not None, "the product tool surface needs a native fixture")
        def native_call(name, arguments):
            outcome = common.call_tool(handle, name, args.repo, arguments, args.call_timeout,
                                       strip_env=common.AGENT_STRIPPED_ENV)
            if outcome.envelope is None:
                return mcp_wrapper.Wrapper._reply(outcome)
            return {**outcome.envelope, **({} if outcome.ok else {"isError": True})}
        server = product_skill.ProductServer(server, product_skill.catalog(handle), native_call)
    mcp_wrapper.serve(server, sys.stdin.buffer, sys.stdout.buffer)
    return 0


if __name__ == "__main__":
    sys.exit(main())
