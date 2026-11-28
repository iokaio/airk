# SPDX-License-Identifier: Apache-2.0
"""Admission and source provenance for the research finalist's task-form Gather transport."""

import hashlib
import unicodedata
from functools import cache

import common
import passage_reply_budget


class UnsupportedRequest(ValueError):
    """The finalist has no implementation for a requested native feature."""


@cache
def native_limits():
    return common.read_json(common.ANSWER_CASES)


def reply_cap(arguments):
    """Admit native task-form controls that this experiment can execute; retain caller text verbatim."""
    allowed = {"task", "needs", "anchors", "deliverable", "budget", "response_mode"}
    other = sorted(set(arguments) - allowed)
    if other:
        raise UnsupportedRequest("unsupported finalist Gather option(s): " + ", ".join(other))
    if arguments.get("response_mode") not in (None, "terse"):
        raise UnsupportedRequest("finalist Gather serves the terse experimental brief")
    task = arguments.get("task")
    if not isinstance(task, str) or not task.strip() or len(task.encode()) > 16384:
        raise ValueError("invalid Gather task")
    if arguments.get("deliverable") not in (None, "analysis", "document", "test", "code_change"):
        raise ValueError("invalid Gather deliverable")
    for field, count, size in (("needs", 8, 400), ("anchors", 16, 200)):
        values = arguments.get(field, [])
        if (not isinstance(values, list) or len(values) > count or
                any(not isinstance(v, str) or not v.strip() or len(v.encode()) > size or
                    any(unicodedata.category(c) == "Cc" for c in v) for v in values) or len(set(values)) != len(values)):
            raise ValueError(f"invalid Gather {field}")
    fixture = native_limits()
    defaults = fixture["default_budget"]
    budget = arguments.get("budget", defaults)
    if not isinstance(budget, dict) or budget.keys() != defaults.keys() or any(type(v) is not int for v in budget.values()):
        raise ValueError("Gather requires the complete native budget")
    changed = sorted(k for k in defaults if k != "max_bytes" and budget[k] != defaults[k])
    if changed:
        raise UnsupportedRequest("unsupported finalist budget override(s): " + ", ".join(changed))
    bounds = {r["id"]: r["budget"]["max_bytes"] for r in fixture["budget_cases"] if r["accepted"]}
    if not bounds["max_bytes_minimum"] <= budget["max_bytes"] <= bounds["max_bytes_maximum"]:
        raise ValueError("invalid Gather retention bytes")
    if budget["max_bytes"] < len(task.encode()) + fixture["minimum_reply_skeleton_bytes"]:
        raise ValueError("budget cannot hold the task and minimum reply")
    return passage_reply_budget.effective_wire_limit(arguments, fixture["reply_frame_bytes"])


def render(arguments, repository, index, metrics, passages, candidate_count, request_id, framing, *, attribution="need_ranges"):
    """Fit the full live reply, including verified original source identities, before returning any passage."""
    if attribution not in ("need_ranges", "bundled_file"):
        raise ValueError("Unrecognized evidence attribution")
    cap = reply_cap(arguments)
    publication = metrics.get("publication")
    fingerprint = metrics.get("scope_fingerprint")
    if not index or not publication or not fingerprint:
        raise ValueError("native scoped path inventory lacks a publication or grant")
    needs = arguments.get("needs") or [arguments["task"]]
    citations = {}
    for p in passages:
        path, _, extent = p.at.rpartition(":")
        lo, hi = map(int, extent.split("-"))
        lines = repository.texts[path]
        if not 1 <= lo <= hi <= len(lines) or p.text != "\n".join(lines[lo - 1:hi]) + "\n":
            raise ValueError("passage differs from the selected workspace range")
        if p.at in citations:
            raise ValueError("duplicate delivered source range")
        supports = getattr(p, "supports", None)
        if supports is None:
            supports = [{"need": p.owner, "scored_at": p.scored_at}] if p.owner is not None else []
        if attribution == "bundled_file" and supports:
            raise ValueError("A bundled file score cannot be attributed to a caller need")
        file_scored_at = getattr(p, "file_scored_at", None)
        if file_scored_at is not None:
            card_path, _, card_extent = file_scored_at.rpartition(":")
            first, last = map(int, card_extent.split("-"))
            if attribution != "bundled_file" or card_path != path or not 1 <= first <= last <= len(lines):
                raise ValueError("File card range differs from source identity")
        owners = set()
        verified = []
        for link in supports:
            owner, at = link["need"], link["scored_at"]
            if type(owner) is not int or not 1 <= owner <= len(needs) or owner in owners or not isinstance(at, str):
                raise ValueError("need link lacks a unique scored range or caller need")
            scored_path, _, scored_extent = at.rpartition(":")
            start, end = map(int, scored_extent.split("-"))
            if scored_path != path or not lo <= start <= end <= hi:
                raise ValueError("scored range differs from its delivered source range")
            owners.add(owner)
            verified.append({"need": owner, "scored_at": at})
        citations[p.at] = (repository.digests[path].hex(), hashlib.sha256(p.text.encode()).hexdigest(), verified, file_scored_at)

    def decorate(body):
        body.pop("source_revision", None)
        body["eligibility"] = {"index_id": index, "publication": publication, "scope_fingerprint": fingerprint}
        body["source"] = "Saved workspace bytes read for this call; displayed lines use LF."
        if attribution == "bundled_file":
            body["evidence_basis"] = "task_level_file_ranking"
            for need in body["needs"]:
                need["status"] = "not_individually_scored"
        for row in body["passages"]:
            row["workspace_sha256"], row["text_sha256"], row["support"], card = citations[row["at"]]
            if card is not None:
                row["file_scored_at"] = card
            if len(row["support"]) == 1:
                row["scored_at"] = row["support"][0]["scored_at"]
        if arguments.get("needs") or arguments.get("deliverable") in ("test", "code_change"):
            body["standing_guidance"] = "Project rules, test commands and build guidance are not independently verified."
        if arguments.get("deliverable") == "code_change":
            body["error_paths"] = "User-facing error paths are not independently verified."
        if not arguments.get("needs"):
            body["needs"][0]["origin"] = "task"

    return passage_reply_budget.render(arguments["task"], needs, None, passages, candidate_count, cap,
                                       request_id, framing, decorate=decorate,
                                       owners=lambda p: [link["need"] for link in citations[p.at][2]])
