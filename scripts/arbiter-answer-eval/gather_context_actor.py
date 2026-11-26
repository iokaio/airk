# SPDX-License-Identifier: Apache-2.0
"""Fresh, isolated Codex actors for the ordinary and archived Gather Context diagnostic."""

from __future__ import annotations

from collections.abc import Callable
from dataclasses import dataclass, field
import json
import math
from pathlib import Path
import time

import codex_isolation_probe as codex_driver
import common
import gather_context_sources as sources
import run_session


MODEL = "gpt-6.2-sol"
EFFORT = "medium"
SCHEMA = "airk.private.gather-context-actor/1"
ARCHIVE_TOOL = "gather_context"
ARCHIVE_TOOLS = [
    {
        "name": ARCHIVE_TOOL,
        "description": "Read the source evidence prepared for the supplied task in the frozen repository.",
        "inputSchema": {
            "type": "object", "properties": {"task": {"type": "string"}},
            "required": ["task"], "additionalProperties": False,
        },
    },
]
WORKSPACE_CHILDREN = ("work", "codex-logs", "codex-state", "codex-tmp")


def _instructions(arm: str) -> str:
    original = ("Your first tool call must be {primary} with the task text, exactly as it was given to you, "
                "as its \"{field}\" argument. After that, choose your own follow-up calls.")
    if arm == "ordinary":
        replacement = "Choose your own source reads using the tools you have been given."
    else:
        replacement = original.format(primary=ARCHIVE_TOOL, field="task")
    common.require(original in run_session.SYSTEM_PROMPT, "Shared actor brief prompt changed")
    return run_session.SYSTEM_PROMPT.replace(original, replacement)


def _content_items(reply: dict) -> list[dict]:
    common.require(isinstance(reply, dict) and isinstance(reply.get("content"), list)
                   and all(isinstance(block, dict) and set(block) == {"type", "text"}
                           and block["type"] == "text" and isinstance(block["text"], str)
                           for block in reply["content"]), "Invalid actor tool reply")
    return [{"type": "inputText", "text": block["text"]} for block in reply["content"]]


def _reply_bytes(items: list[dict]) -> int:
    return sum(len(item["text"].encode("utf-8")) for item in items)


def _digest(value) -> str:
    return common.sha256_bytes(common.canonical_json(value))


def _refusal(reason: str) -> dict:
    return {"isError": True, "content": [{"type": "text", "text": reason}]}


@dataclass
class ActorDispatch:
    """Validate and retain each dynamic dispatch and the exact returned envelope."""

    arm: str
    instruction: str
    reply_byte_limit: int
    deadline: float
    clock: Callable[[], float] = time.monotonic
    source_root: Path | None = None
    archived_reply: dict | None = None
    archived_reply_sha256: str | None = None
    rows: list[dict] = field(default_factory=list, init=False)
    replies: list[bytes] = field(default_factory=list, init=False)
    evidence_bytes: int = field(default=0, init=False)
    _archived_raw: bytes = field(default=b"", init=False, repr=False)

    def __post_init__(self):
        common.require(self.arm in ("ordinary", "archived") and isinstance(self.instruction, str)
                       and bool(self.instruction), "Invalid Gather Context actor arm or task")
        common.require(type(self.reply_byte_limit) is int and self.reply_byte_limit > 0,
                       "Expected a matched positive tool evidence byte limit")
        if self.arm == "ordinary":
            common.require(isinstance(self.source_root, Path) and self.source_root.is_absolute()
                           and self.archived_reply is None and self.archived_reply_sha256 is None,
                           "Ordinary actor requires exactly one authenticated export")
        else:
            common.require(self.source_root is None and isinstance(self.archived_reply, dict)
                           and set(self.archived_reply) == {"content"}
                           and isinstance(self.archived_reply["content"], list)
                           and len(self.archived_reply["content"]) == 1,
                           "Archived actor requires exactly one successful frozen reply")
            items = _content_items(self.archived_reply)
            self._archived_raw = common.canonical_json(self.archived_reply)
            common.require(common.sha256_bytes(self._archived_raw) == self.archived_reply_sha256,
                           "Archived reply digest differs from its frozen pin")
            common.require(0 < _reply_bytes(items) <= self.reply_byte_limit,
                           "Archived reply would exceed the shared evidence byte limit")

    @property
    def tools(self) -> list[dict]:
        return sources.TOOLS if self.arm == "ordinary" else ARCHIVE_TOOLS

    def __call__(self, tool: str, arguments: dict) -> dict:
        ordinal = len(self.rows) + 1
        began = self.clock()
        if ordinal > common.TURN_CAP:
            reply = _refusal("Tool call limit reached")
        elif began >= self.deadline:
            reply = _refusal("Actor deadline reached")
        elif self.arm == "ordinary":
            reply = sources.dispatch(self.source_root, tool, arguments)
        elif ordinal != 1 or tool != ARCHIVE_TOOL or arguments != {"task": self.instruction}:
            reply = _refusal("The first call must request the supplied task exactly")
        else:
            reply = json.loads(self._archived_raw)

        items = _content_items(reply)
        finished = self.clock()
        if not reply.get("isError") and finished >= self.deadline:
            reply = _refusal("Actor deadline reached during the source read")
        elif not reply.get("isError") and self.evidence_bytes + _reply_bytes(items) > self.reply_byte_limit:
            reply = _refusal("Tool evidence byte limit reached; try a smaller source read")
        items = _content_items(reply)
        size = _reply_bytes(items)
        raw = common.canonical_json(reply)
        success = not reply.get("isError", False)
        if success:
            self.evidence_bytes += size
        self.rows.append({
            "ordinal": ordinal, "tool": tool, "arguments": arguments, "success": success,
            "elapsed_ms": int((finished - began) * 1000), "reply_text_bytes": size,
            "evidence_text_bytes": size if success else 0, "reply_sha256": common.sha256_bytes(raw),
            "dispatch_sha256": _digest(items), "reply_file": f"reply-{ordinal:03d}.json",
        })
        self.replies.append(raw)
        return reply


def _audit_actor(stream: bytes, observed: dict, dispatch: ActorDispatch) -> list[str]:
    """Reconcile retained driver events, dispatch records and the actual tool replies."""
    problems = []
    if observed.get("protocol_error"):
        problems.append("Actor driver reported a protocol error")
    if observed.get("configured_mcp") != {} or observed.get("instructions_count") != 0:
        problems.append("Actor isolation was not observed")
    if (observed.get("model_observed"), observed.get("effort_observed"), observed.get("provider_observed"),
            observed.get("auth_route")) != (MODEL, EFFORT, "openai", "chatgpt"):
        problems.append("Actor model, effort or authentication route was not observed")
    thread = observed.get("thread_id")
    turn = observed.get("turn_id")
    records = [json.loads(line) for line in stream.split(b"\n") if line.strip()]
    calls = []
    finals = []
    terminals = []
    allowed = {tool["name"] for tool in dispatch.tools}
    for event in records:
        method = event.get("method")
        params = event.get("params") or {}
        if method == "item/tool/call" and "id" in event:
            if (params.get("threadId"), params.get("turnId"), params.get("namespace")) != (
                    thread, turn, codex_driver.DYNAMIC_NAMESPACE) or params.get("tool") not in allowed:
                problems.append("Tool request crossed actor, turn or offer")
            calls.append((params.get("callId"), event["id"], params.get("tool"), params.get("arguments")))
        elif method and "id" in event:
            problems.append("Unexpected actor server request")
        if method == "item/completed" and (params.get("threadId"), params.get("turnId")) == (thread, turn):
            item = params.get("item") or {}
            if item.get("type") == "agentMessage" and item.get("phase") != "commentary":
                finals.append(item.get("text"))
        if method == "turn/completed":
            terminal = params.get("turn") or {}
            terminals.append((params.get("threadId"), terminal.get("id"), terminal.get("status")))

    attempts = observed.get("attempts", [])
    identities = [(attempt["call_id"], attempt["rpc_id"], attempt["tool"], attempt["arguments"])
                  for attempt in attempts]
    if (calls != identities or len(calls) != len(dispatch.rows)
            or len({call[0] for call in calls}) != len(calls)
            or len({call[1] for call in calls}) != len(calls)):
        problems.append("Actor events and tool attempts differ")
    for attempt, row in zip(attempts, dispatch.rows):
        if (attempt["tool"], attempt["arguments"], attempt["success"], attempt["output_sha256"]) != (
                row["tool"], row["arguments"], row["success"], row["dispatch_sha256"]):
            problems.append("Actor tool dispatch or reply digest differs")
            break
    if len(calls) > common.TURN_CAP:
        problems.append("Actor exceeded the tool call cap")
    if observed.get("turn_status") == "completed":
        if not thread or not turn or terminals != [(thread, turn, "completed")]:
            problems.append("Completed actor turn is missing from the transcript")
        if not finals or not finals[-1] or finals[-1] != observed.get("final"):
            problems.append("Retained actor brief differs from the transcript")
        if dispatch.arm == "archived" and (not dispatch.rows or not dispatch.rows[0]["success"]):
            problems.append("Archived reply did not reach the actor in its first call")
    return problems


def _workspace(directory: Path) -> None:
    common.require(directory.is_absolute() and directory.resolve(strict=True) == directory
                   and not sources._linked(directory.lstat()) and directory.is_dir(),
                   "Expected a fresh absolute and canonical actor directory")
    common.require({item.name for item in directory.iterdir()} == set(WORKSPACE_CHILDREN),
                   "Actor directory is not a fresh guarded layout")
    for name in WORKSPACE_CHILDREN:
        child = directory / name
        common.require(not sources._linked(child.lstat()) and child.is_dir() and not any(child.iterdir()),
                       "Actor child directory is linked or already used")


def run_actor(*, config: run_session.CodexRunConfig, instruction: str, directory: Path, timeout_s: float,
              arm: str, reply_byte_limit: int, audit_workspace, write_receipt, source_root: Path | None = None,
              archived_reply: dict | None = None, archived_reply_sha256: str | None = None,
              runner=codex_driver.run_app_server, clock=time.monotonic) -> dict:
    """Run one fresh diagnostic actor and persist only after the supplied custody audit.

    The controller must authenticate the input export or archive, create the four
    empty child directories with the checked Windows guard, verify every child
    and possible child/cache output parent before launch, and audit outputs on
    both success and failure. The writer owns checked writes of the returned
    receipt paths, including any temporary output it uses.
    """
    common.require(type(timeout_s) in (int, float) and math.isfinite(timeout_s) and timeout_s > 0,
                   "Expected a positive finite actor deadline")
    _workspace(directory)
    common.require(callable(audit_workspace) and callable(write_receipt),
                   "Actor requires audited workspace and receipt writers")
    provenance = run_session.codex_provenance(config)
    command = run_session.codex_command(config, directory)
    base_instructions = _instructions(arm)
    audit_workspace("before", directory, config.isolated_home)
    started = common.now_iso()
    began = clock()
    dispatch = ActorDispatch(arm, instruction, reply_byte_limit, began + timeout_s, clock,
                             source_root, archived_reply, archived_reply_sha256)
    planned = codex_driver.dynamic_specs(dispatch.tools)
    startup_failure = False
    try:
        code, stream, stderr, observed = runner(
            command, run_session.codex_child_env(config, directory), directory / "work",
            model=MODEL, effort=EFFORT, instruction=instruction, tool_catalog=dispatch.tools,
            clean_home=True, dispatch=dispatch, base_instructions=base_instructions,
            timeout=max(0, dispatch.deadline - clock()), require_chatgpt=True,
        )
    except OSError:
        startup_failure = True
        code, stream, stderr, observed = 2, b"", b"", {}
    finally:
        audit_workspace("after", directory, config.isolated_home)

    elapsed_ms = int((clock() - began) * 1000)
    try:
        problems = _audit_actor(stream, observed, dispatch)
    except (KeyError, TypeError, ValueError, AttributeError):
        problems = ["Actor data could not be reconciled with its transcript"]
    if startup_failure:
        problems.append("Actor process failed to start")
    if code is None:
        status = "timeout"
    elif code != 0 or observed.get("turn_status") != "completed":
        status = "failed"
    else:
        status = "discarded" if problems else "complete"

    brief = (observed.get("final") or "").encode("utf-8")
    turns = b"".join(common.canonical_json(row) + b"\n" for row in dispatch.rows)
    artifacts = {
        "instruction.txt": instruction.encode("utf-8"), "base-instructions.txt": base_instructions.encode("utf-8"),
        "tool-offer.json": common.canonical_json(planned), "stream.jsonl": stream, "stderr.txt": stderr,
        "brief.md": brief, "turns.jsonl": turns,
    }
    artifacts.update((row["reply_file"], raw) for row, raw in zip(dispatch.rows, dispatch.replies))
    record = {
        "schema": SCHEMA, "arm": arm, "status": status, "exit_code": code, "problems": problems,
        "started": started, "elapsed_ms": elapsed_ms, "actor": provenance, "observed": observed,
        "model": MODEL, "effort": EFFORT, "max_tool_calls": common.TURN_CAP, "timeout_s": timeout_s,
        "reply_byte_limit": reply_byte_limit, "evidence_text_bytes": dispatch.evidence_bytes,
        "returned_control_text_bytes": sum(row["reply_text_bytes"] for row in dispatch.rows if not row["success"]),
        "tool_calls": len(dispatch.rows), "source_root": str(source_root) if source_root is not None else None,
        "archived_reply_sha256": archived_reply_sha256,
        "offer_basis": "driver thread/start input; model-visible catalog not observed on hosted traffic",
        "files": {name: common.sha256_bytes(raw) for name, raw in artifacts.items()},
    }
    for name, raw in artifacts.items():
        write_receipt(directory / name, raw)
    write_receipt(directory / "session.json", common.canonical_json(record) + b"\n")
    return record
