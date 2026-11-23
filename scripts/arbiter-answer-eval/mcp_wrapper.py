# SPDX-License-Identifier: Apache-2.0
"""MCP stdio server that gives an evaluation agent exactly one path's tools.

JSON-RPC 2.0 over stdio, newline-delimited (the MCP stdio transport); a client
that frames with ``Content-Length`` headers is answered the same way. Methods:
initialize, notifications/initialized, tools/list, tools/call, ping.

Each tools/call becomes one CLI process against the evaluation service (see
common.call_tool). The wrapper injects the index id, enforces the path's tool
list, the first-call rule (the path's primary tool with the instruction
verbatim) and the 12-turn cap, and logs every call to ``turns.jsonl`` with the
raw result beside it (formats.md).
"""

from __future__ import annotations

import argparse
import io
import json
import sys
import threading
import time
from pathlib import Path
from typing import Any, BinaryIO

HERE = Path(__file__).resolve().parent
if str(HERE) not in sys.path:
    sys.path.insert(0, str(HERE))

import common  # noqa: E402
import replay  # noqa: E402

PROTOCOL_VERSIONS = ("2025-06-18", "2025-03-26", "2024-11-05")
SERVER_INFO = {"name": "airk-answer-eval-wrapper", "version": "1"}


class Wrapper:
    def __init__(self, *, handle: common.Handle | None, repo: str, path: str, instruction: str,
                 catalog: dict[str, dict], log_dir: Path, answer_defaults: dict | None = None,
                 parse_defaults: dict | None = None, call_timeout_s: float = 300.0,
                 backend=None) -> None:
        common.require(path in common.PATH_TOOLS, f"unknown path {path!r}")
        self.handle, self.repo, self.path = handle, repo, path
        self.instruction = instruction
        self.tools = common.PATH_TOOLS[path]
        self.primary = self.tools[0]
        missing = [t for t in self.tools if t not in catalog]
        common.require(not missing, f"tools catalog lacks {missing}")
        self.catalog = catalog
        self.log_dir = Path(log_dir)
        self.results_dir = self.log_dir / "results"
        self.results_dir.mkdir(parents=True, exist_ok=True)
        self.turn_log = self.log_dir / "turns.jsonl"
        budget = dict(common.ANSWER_BUDGET_DEFAULT)
        self.answer_defaults = dict(answer_defaults or {})
        answer_defaults = dict(answer_defaults or {})
        budget.update(answer_defaults.pop("budget", {}) or {})
        self.answer_forced = {"schema": common.ANSWER_SCHEMA, "budget": budget, **answer_defaults}
        self.parse_forced = {"schema": common.PARSE_SCHEMA}
        self.parse_defaults = {"mode": "execute"} if parse_defaults is None else dict(parse_defaults)
        self.call_timeout_s = call_timeout_s
        self.backend = backend or self._cli_backend
        self.counted = 0
        self.calls = 0
        self.lock = threading.Lock()

    # ------------------------------------------------------------ catalog

    def list_tools(self) -> list[dict]:
        return [{"name": name, "description": self.catalog[name]["description"],
                 "inputSchema": common.agent_schema(name, self.catalog[name]["inputSchema"])}
                for name in self.tools]

    def meta(self) -> dict:
        return {"schema": "airk.private.answer-eval-wrapper/1", "repo": self.repo, "path": self.path,
                "tools": self.tools, "primary": self.primary, "turn_cap": common.TURN_CAP,
                "instruction_sha256": common.sha256_bytes(self.instruction.encode("utf-8")),
                "answer_forced": self.answer_forced, "parse_forced": self.parse_forced,
                "parse_defaults": self.parse_defaults,
                "handle": None if self.handle is None else {"path": str(self.handle.path), "sha256": self.handle.sha256,
                                                            "index_id": self.handle.index_id(self.repo)}}

    # ------------------------------------------------------------ calls

    def _cli_backend(self, tool: str, args: dict) -> common.CallOutcome:
        common.require(self.handle is not None, "no handle")
        return common.call_tool(self.handle, tool, self.repo, args, self.call_timeout_s,
                                strip_env=common.AGENT_STRIPPED_ENV)

    def _send_arguments(self, tool: str, arguments: dict) -> dict:
        sent = {k: v for k, v in arguments.items() if k not in common.HIDDEN_ARGS}
        if tool == "airk_answer":
            sent.update(self.answer_forced)
        elif tool == "airk_parse":
            for key, value in self.parse_defaults.items():
                sent.setdefault(key, value)
            sent.update(self.parse_forced)
        return sent

    def _reject(self, tool: str, arguments: Any, reason: str) -> dict:
        common.append_jsonl(self.turn_log, {
            "turn": None, "call": self.calls, "tool": tool, "arguments": arguments, "started": common.now_iso(),
            "elapsed_ms": 0, "bytes": 0, "ok": False, "result_path": None, "result_sha256": None,
            "rejected": True, "reason": reason})
        return {"content": [{"type": "text", "text": f"Refused: {reason}"}], "isError": True}

    def call(self, tool: str, arguments: Any) -> dict:
        with self.lock:
            self.calls += 1
            if not isinstance(arguments, dict):
                return self._reject(tool, arguments, "arguments must be an object")
            if tool not in self.tools:
                return self._reject(tool, arguments, f"{tool} is not available; available tools: {', '.join(self.tools)}")
            if self.counted >= common.TURN_CAP:
                return self._reject(tool, arguments, f"the {common.TURN_CAP}-call limit is reached; write your brief now")
            if self.counted == 0:
                field = common.PRIMARY_ARG[self.primary]
                given = arguments.get(field)
                if tool != self.primary or not isinstance(given, str) or given.strip() != self.instruction.strip():
                    return self._reject(tool, arguments, (
                        f"the first call must be {self.primary} with the task text, exactly as given, "
                        f"as its {field!r} argument"))
            self.counted += 1
            turn = self.counted
            first_call = None
            if turn == 1:
                # Plan 3.5: the first call is the path's primary tool with the instruction as written, issued by
                # the harness with the product's defaults for everything else (response mode, verification) and
                # the cases file's default budget: exactly the arguments the replays send
                # (replay.canonical_first_call). Arguments the agent added are recorded and not sent. On
                # 23 November 2026 an agent's own first call chose verification off, so sessions and replays
                # measured different renderings. The agent chooses its own arguments from its second call on.
                _tool, sent = replay.canonical_first_call(self.path, self.instruction, self.answer_defaults)
                first_call = {"rule": "canonical", "ignored_arguments": sorted(
                    k for k in arguments if k != common.PRIMARY_ARG[self.primary] and k not in common.HIDDEN_ARGS)}
            else:
                sent = self._send_arguments(tool, arguments)
            started = common.now_iso()
            stem = f"turn-{turn:02d}"
            outcome, retries = self._with_typed_retries(tool, sent, stem)
            result_path = self.results_dir / f"{stem}.stdout.json"
            common.write_bytes(result_path, outcome.stdout)
            stderr_path = None
            if outcome.stderr:
                stderr_path = self.results_dir / f"{stem}.stderr.txt"
                common.write_bytes(stderr_path, outcome.stderr)
            reply = self._reply(outcome)
            text_bytes = sum(len(b.get("text", "").encode("utf-8")) for b in reply["content"])
            elapsed = outcome.elapsed_ms + sum(r["elapsed_ms"] + r["waited_ms"] for r in retries)
            common.append_jsonl(self.turn_log, {
                "turn": turn, "call": self.calls, "tool": tool, "arguments": arguments, "sent_arguments": sent,
                "first_call": first_call,
                "started": started, "elapsed_ms": elapsed, "bytes": text_bytes, "ok": outcome.ok,
                "typed_retries": retries, "final_attempt_ms": outcome.elapsed_ms,
                "result_path": str(result_path), "result_sha256": common.sha256_bytes(outcome.stdout),
                "stderr_path": None if stderr_path is None else str(stderr_path),
                "exit_code": outcome.exit_code, "error": outcome.error, "rejected": False,
                "response_mode": common.effective_response_mode(self.handle, sent, common.AGENT_STRIPPED_ENV),
                "env_stripped": list(common.AGENT_STRIPPED_ENV)})
            return reply

    MAX_TYPED_RETRIES = 8
    MAX_RETRY_WAIT_MS = 2000

    def _with_typed_retries(self, tool: str, sent: dict, stem: str):
        """One turn: the call, repeated while the service returns a typed retry (baseline-bookbuilder finding:
        selection_busy, publication_busy and scoped_observation_unavailable under 4 concurrent sessions).
        Each retried reply is kept beside the turn's result; the agent sees only the final reply."""
        retries = []
        outcome = self.backend(tool, sent)
        while len(retries) < self.MAX_TYPED_RETRIES:
            retry, delay_ms, code = common.typed_retry(outcome.envelope)
            if not retry:
                break
            path = self.results_dir / f"{stem}.retry-{len(retries) + 1}.stdout.json"
            common.write_bytes(path, outcome.stdout)
            wait = min(max(delay_ms, 25) * (2 ** len(retries)), self.MAX_RETRY_WAIT_MS)
            time.sleep(wait / 1000)
            retries.append({"code": code, "elapsed_ms": outcome.elapsed_ms, "waited_ms": wait,
                            "result_path": str(path), "result_sha256": common.sha256_bytes(outcome.stdout)})
            outcome = self.backend(tool, sent)
        if tool == "airk_answer":
            outcome = self._attach_tickets(tool, sent, stem, outcome, retries)
        return outcome, retries

    MAX_TICKET_ATTACHES = 16
    MAX_TICKET_WAIT_MS = 30000

    def _attach_tickets(self, tool: str, sent: dict, stem: str, outcome, retries: list):
        """Answer /3 queueing (plan 12.4.1): a reply queued at admission is a ticket. Like a typed retry, the
        wrapper waits ``retry_after_ms`` and attaches with ``continuation`` = the ticket id, so the agent sees
        the Answer itself, not the queue; each ticket reply is kept beside the turn and logged as a retry
        with code ``ticket``. Harness convention, 22 Nov 2026."""
        import loop_reply  # local: the wrapper's other paths do not need it
        attaches = 0
        while attaches < self.MAX_TICKET_ATTACHES:
            body = common.envelope_body(outcome.envelope) if outcome.ok else None
            queued, after_ms, ticket_id = loop_reply.ticket_wait(body)
            if not queued or not ticket_id:
                break
            attaches += 1
            path = self.results_dir / f"{stem}.ticket-{attaches}.stdout.json"
            common.write_bytes(path, outcome.stdout)
            wait = min(max(after_ms, 250), self.MAX_TICKET_WAIT_MS)
            time.sleep(wait / 1000)
            retries.append({"code": "ticket", "ticket": ticket_id, "elapsed_ms": outcome.elapsed_ms,
                            "waited_ms": wait, "result_path": str(path),
                            "result_sha256": common.sha256_bytes(outcome.stdout)})
            outcome = self.backend(tool, {**sent, "continuation": ticket_id})
        return outcome

    @staticmethod
    def _reply(outcome: common.CallOutcome) -> dict:
        if outcome.envelope is not None and isinstance(outcome.envelope.get("content"), list):
            reply = {"content": common.envelope_content(outcome.envelope)}
            if not outcome.ok:
                reply["isError"] = True
            return reply
        if outcome.envelope is not None:
            return {"content": common.envelope_content(outcome.envelope), **({} if outcome.ok else {"isError": True})}
        tail = outcome.stderr[-2048:].decode("utf-8", "replace")
        return {"content": [{"type": "text", "text": f"Tool call failed ({outcome.error}). {tail}".strip()}],
                "isError": True}

    # ------------------------------------------------------------ JSON-RPC

    def handle_message(self, message: Any) -> dict | None:
        if not isinstance(message, dict) or message.get("jsonrpc") != "2.0":
            return _error(None, -32600, "invalid request")
        method, msg_id, params = message.get("method"), message.get("id"), message.get("params") or {}
        is_request = "id" in message
        if method is None:
            return None  # a response from the client; nothing to do
        if not is_request:
            return None  # notifications/initialized, notifications/cancelled, ...
        if method == "initialize":
            requested = params.get("protocolVersion")
            version = requested if requested in PROTOCOL_VERSIONS else PROTOCOL_VERSIONS[0]
            return _result(msg_id, {"protocolVersion": version, "capabilities": {"tools": {"listChanged": False}},
                                    "serverInfo": SERVER_INFO})
        if method == "ping":
            return _result(msg_id, {})
        if method == "tools/list":
            return _result(msg_id, {"tools": self.list_tools()})
        if method == "tools/call":
            name = params.get("name")
            if not isinstance(name, str):
                return _error(msg_id, -32602, "tools/call needs a tool name")
            return _result(msg_id, self.call(name, params.get("arguments", {})))
        return _error(msg_id, -32601, f"method not found: {method}")


def _result(msg_id: Any, result: dict) -> dict:
    return {"jsonrpc": "2.0", "id": msg_id, "result": result}


def _error(msg_id: Any, code: int, message: str) -> dict:
    return {"jsonrpc": "2.0", "id": msg_id, "error": {"code": code, "message": message}}


# ---------------------------------------------------------------- transport


def read_message(stream: BinaryIO) -> tuple[Any, str] | None:
    """Read one message; returns (message, framing) or None at end of input."""
    while True:
        line = stream.readline()
        if not line:
            return None
        stripped = line.strip()
        if not stripped:
            continue
        if stripped.lower().startswith(b"content-length:"):
            length = int(stripped.split(b":", 1)[1])
            while True:
                header = stream.readline()
                if not header or not header.strip():
                    break
            payload = stream.read(length)
            return _decode(payload), "header"
        return _decode(stripped), "line"


def _decode(payload: bytes) -> Any:
    try:
        return json.loads(payload)
    except ValueError:
        return {"_parse_error": True}


def write_message(stream: BinaryIO, message: dict, framing: str) -> None:
    data = json.dumps(message, ensure_ascii=False, separators=(",", ":")).encode("utf-8")
    if framing == "header":
        stream.write(b"Content-Length: %d\r\n\r\n" % len(data) + data)
    else:
        stream.write(data + b"\n")
    stream.flush()


def serve(wrapper: Wrapper, stdin: BinaryIO, stdout: BinaryIO) -> None:
    while True:
        read = read_message(stdin)
        if read is None:
            return
        message, framing = read
        if isinstance(message, dict) and message.get("_parse_error"):
            write_message(stdout, _error(None, -32700, "parse error"), framing)
            continue
        if isinstance(message, list):  # JSON-RPC batch
            replies = [r for r in (wrapper.handle_message(m) for m in message) if r is not None]
            if replies:
                data = json.dumps(replies, ensure_ascii=False).encode("utf-8")
                stdout.write(data + b"\n")
                stdout.flush()
            continue
        reply = wrapper.handle_message(message)
        if reply is not None:
            write_message(stdout, reply, framing)


def build_parser() -> argparse.ArgumentParser:
    parser = argparse.ArgumentParser(description=__doc__.splitlines()[0])
    parser.add_argument("--handle", type=Path, required=True)
    parser.add_argument("--repo", choices=common.KNOWN_REPOS, required=True)
    parser.add_argument("--path", choices=sorted(common.PATH_TOOLS), required=True)
    parser.add_argument("--instruction-file", type=Path, required=True)
    parser.add_argument("--log-dir", type=Path, required=True)
    parser.add_argument("--catalog", type=Path, help="default: <handle>/../../tools-catalog.json")
    parser.add_argument("--answer-defaults", help="JSON object forced into every airk_answer call")
    parser.add_argument("--parse-defaults", help="JSON object of airk_parse defaults (default {\"mode\":\"execute\"})")
    parser.add_argument("--call-timeout", type=float, default=300.0, help="seconds per CLI process")
    parser.add_argument("--accept-queryable", action="store_true",
                        help="pilot only: accept a handle marked queryable whose vectors are partial")
    return parser


def main(argv: list[str] | None = None) -> int:
    args = build_parser().parse_args(argv)
    handle = common.load_handle(args.handle, accept_queryable=args.accept_queryable)
    catalog = common.load_catalog(args.catalog or args.handle.resolve().parent.parent / "tools-catalog.json")
    instruction = args.instruction_file.read_text(encoding="utf-8")
    wrapper = Wrapper(handle=handle, repo=args.repo, path=args.path, instruction=instruction, catalog=catalog,
                      log_dir=args.log_dir, answer_defaults=json.loads(args.answer_defaults) if args.answer_defaults else None,
                      parse_defaults=json.loads(args.parse_defaults) if args.parse_defaults else None,
                      call_timeout_s=args.call_timeout)
    meta = wrapper.meta()
    meta.update(started=common.now_iso(), handle_readiness=handle.readiness(), catalog_sha256=common.sha256_file(
        args.catalog or args.handle.resolve().parent.parent / "tools-catalog.json"))
    common.write_json(args.log_dir / "wrapper.json", meta)
    stdin = sys.stdin.buffer if hasattr(sys.stdin, "buffer") else io.BytesIO()
    serve(wrapper, stdin, sys.stdout.buffer)
    return 0


if __name__ == "__main__":
    sys.exit(main())
