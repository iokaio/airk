# SPDX-License-Identifier: Apache-2.0
"""Route a task-form Gather call to the selected, verified single-Arbiter policy."""
import json
from pathlib import Path
import time

import common
import finalist_gather_contract as contract
import live_gather
import mcp_wrapper
import passage_reply_budget
import workspace_scope


def check_policy(path):
    fingerprint = common.sha256_file(path)
    common.require(fingerprint == "51fa5dc90bcf4eb5e17678582d277a9841b04065020d8bcf4f127df2ba3657a9",
                   "Unrecognized single-Arbiter policy freeze")
    freeze = common.read_json(path)
    for name, digest in freeze["source_sha256"].items():
        common.require(common.sha256_file(Path(__file__).with_name(name)) == digest,
                       f"Selected single-Arbiter policy source changed: {name}")
    return fingerprint


class Gather:
    def __init__(self, handle, repo, source, log_dir, score, propose, max_calls):
        self.handle, self.repo, self.log_dir = handle, repo, log_dir
        self.score, self.propose = score, propose
        self.workspace = live_gather.WorkspaceRepository(source)
        self.cache = workspace_scope.InventoryCache()
        self.calls, self.max_calls = 0, max_calls

    def handle_message(self, message, framing="line"):
        arguments = message["params"]["arguments"]
        ident = message["id"]
        call = self.calls + 1
        start = time.monotonic()
        metrics, batches = {}, []
        started_scoring = False
        phase = "request"

        def score(instruction, pairs):
            nonlocal started_scoring
            pairs = list(pairs)
            started_scoring = True
            measured = time.monotonic()
            batch = {"instruction": instruction, "pairs": pairs, "scores": None}
            batches.append(batch)
            try:
                scores = self.score(instruction, pairs)
                if len(scores) != len(pairs):
                    raise ValueError("Scorer returned the wrong number of judgments")
                batch["scores"] = scores
                return scores
            finally:
                batch["seconds"] = round(time.monotonic() - measured, 4)

        budget = arguments.get("budget")
        cap = contract.native_limits()["reply_frame_bytes"]
        if isinstance(budget, dict) and type(budget.get("max_bytes")) is int:
            cap = min(cap, max(0, budget["max_bytes"]))
        if call > self.max_calls:
            wire = passage_reply_budget.encode({"error": {"code": "call_limit_reached"}}, ident, framing, error=True)
            frameable = len(wire) <= cap
            common.append_jsonl(self.log_dir / "calls.jsonl", {
                "call": call, "at": common.now_iso(), "arguments": arguments, "seconds": 0,
                "reply_bytes": len(json.loads(wire.split(b"\r\n\r\n", 1)[-1])["result"]["content"][0]["text"].encode()) if frameable else 0,
                "wire_bytes": len(wire) if frameable else 0, "is_error": True, "judgments": 0, "started_scoring": False})
            if not frameable:
                raise mcp_wrapper.UnframeableReply("No admissible refusal fits the caller budget")
            return json.loads(wire.split(b"\r\n\r\n", 1)[-1])
        self.calls = call
        audit = {}
        try:
            cap = contract.reply_cap(arguments)
            if len(passage_reply_budget.encode({"error": {"code": "reply_budget_too_small"}}, ident, framing, error=True)) > cap:
                raise ValueError("Request ID exceeds the available response frame")
            phase = "workspace"
            eligible = workspace_scope.eligible_paths(self.handle, self.repo, metrics, self.cache)
            repository = self.workspace.refresh(eligible)
            metrics.update(repository.capture)
            phase = "policy"
            passages, total, audit = self.propose(repository, arguments, score)
            phase = "render"
            render = contract.render
            if audit.get("spare_context"):
                from finalist_spare_context import render
            wire, kept = render(arguments, repository, self.handle.index_id(self.repo), metrics,
                                         passages, total, ident, framing, attribution=audit["attribution"])
            if wire is None or len(wire) > cap:
                raise ValueError("Minimum framed response exceeds caller budget")
            audit["wire_passages"] = [p.at for p in kept]
        except Exception as exc:
            # No fallback model, implicit repeat, or unverified source may answer on failure.
            code = ("unsupported_argument" if isinstance(exc, contract.UnsupportedRequest) else
                    {"request": "invalid_arguments", "workspace": "workspace_evidence_unavailable",
                     "policy": "arbiter_unavailable", "render": "reply_unavailable"}[phase])
            detail = {"error": {"code": code, "retryable": False}}
            if isinstance(exc, workspace_scope.InventoryUnavailable):
                audit["native_inventory"] = {"page": exc.page, "publication": exc.publication, "reply": exc.reply}
                native = (exc.reply.get("result", {}).get("structuredContent") or {}).get("error")
                if isinstance(native, dict):
                    detail["error"]["native_error"] = native
            wire = passage_reply_budget.encode(detail, ident, framing, error=True)
            audit["failure"] = {"phase": phase, "type": type(exc).__name__, "message": str(exc)[:1024]}
        if len(wire) > cap:
            full_text = json.loads(wire.split(b"\r\n\r\n", 1)[-1])["result"]["content"][0]["text"]
            audit["unframeable_error"] = full_text
            original = json.loads(full_text)["error"]
            small = {"code": original["code"], "retryable": False}
            native = original.get("native_error", {})
            if native:
                # Preserve the native no-replay boundary even when its explanatory string is too long.
                small["native_error"] = {k: native[k] for k in ("code", "retryable", "fresh_brokerage_required",
                                         "query_replay_safe", "query_submitted") if k in native}
            wire = passage_reply_budget.encode({"error": small}, ident, framing, error=True)
            if len(wire) > cap:
                minimal = {"error": {"code": original["code"], "retryable": False}}
                wire = passage_reply_budget.encode(minimal, ident, framing, error=True)
            if len(wire) > cap:
                wire = b""
        reply = json.loads(wire.split(b"\r\n\r\n", 1)[-1]) if wire else None
        short = f"call-{call:03}"
        (self.log_dir / (short + ".reply.json")).write_bytes(json.dumps(reply, ensure_ascii=False).encode("utf-8"))
        (self.log_dir / (short + ".wire")).write_bytes(wire)
        common.write_json(self.log_dir / (short + ".scores.json"), {"batches": batches, "policy": audit})
        common.append_jsonl(self.log_dir / "calls.jsonl", {
            "call": call, "at": common.now_iso(), "arguments": arguments, "seconds": round(time.monotonic() - start, 3),
            "reply_bytes": len(reply["result"]["content"][0]["text"].encode()) if reply else 0, "wire_bytes": len(wire),
            "is_error": reply is None or reply["result"]["isError"], "workspace_capture": metrics, "started_scoring": started_scoring,
            "judgments": sum(len(batch["scores"] or []) for batch in batches)})
        if reply is None:
            raise mcp_wrapper.UnframeableReply("No admissible error frame fits the caller budget")
        return reply