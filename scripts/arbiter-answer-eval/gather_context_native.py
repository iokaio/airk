# SPDX-License-Identifier: Apache-2.0
"""Normalize the native reply actually delivered by the Gather Context transport."""

from __future__ import annotations

from functools import lru_cache
import json
from pathlib import Path
import re

import common
import evidence
import loop_reply
from prepare_complete_evidence import PreparationError, SOURCE_LIMIT, excerpt_bytes, reopen_exact


_SHA256 = re.compile(r"[0-9a-f]{64}")
_WORK_COUNTS = ("calls", "rounds", "fanout", "input_tokens", "candidates", "bytes",
                "compile_us", "inference_us", "policy_us", "queue_us")
_OFF = {"focus": None, "context": None}


def _require(condition: bool, reason: str) -> None:
    if not condition:
        raise ValueError(reason)


def verify_observation(expected: dict, observation: dict, stages: dict | None) -> None:
    """Check settled.snapshot and live (never journal fallback) stage observations."""
    _require(observation.get("settled") is True
             and observation.get("semantic_state") == "ready"
             and observation.get("served_mode") == "hybrid"
             and not observation.get("call_errors")
             and observation.get("execution_incomplete") is not True,
             "observed publication is not settled and serving ready Hybrid")
    for field in ("repo", "publication_token", "index_id", "binding"):
        expected_value = expected.get("publication" if field == "publication_token" else field)
        _require(isinstance(expected_value, str) and expected_value
                 and observation.get(field) == expected_value,
                 f"settled observation {field} differs or is unavailable")
    _require(stages == _OFF, "live Focus and Arbiter must both be observed off")


def _count(value: object) -> int | None:
    return value if type(value) is int and value >= 0 else None


def _row(case: dict, delivered_text: str, elapsed_ms: int | None) -> dict:
    _require(isinstance(delivered_text, str) and (elapsed_ms is None or _count(elapsed_ms) is not None),
             "invalid delivered text or elapsed time")
    text_bytes = delivered_text.encode("utf-8")
    return {
        **{key: case[key] for key in ("id", "repo", "source_commit", "publication", "index_id")},
        "delivered_text": delivered_text,
        "delivered_text_sha256": common.sha256_bytes(text_bytes),
        "delivered_passages": [],
        "unverified_passages": [],
        "actions": [],
        "action_trace_complete": False,
        "action_trace_limit": "The emitted native reply does not enumerate every dispatched read.",
        "costs": {
            "native_transport_elapsed_ms": elapsed_ms,
            "serialized_delivery_bytes": len(text_bytes),
            "native_files_scanned": None,
            "native_bytes_scanned": None,
            "native_retained_text_bytes": None,
        },
    }


def capture_failure(case: dict, disposition: str, detail: dict, *, elapsed_ms: int | None,
                    delivered_text: str = "") -> dict:
    """Record a caller-observed terminal refusal, cancellation or timeout."""
    _require(disposition in {"refused", "cancelled", "timeout"}
             and isinstance(detail, dict) and bool(detail),
             "a terminal failure requires its observed detail")
    return {**_row(case, delivered_text, elapsed_ms), "disposition": disposition,
            "terminal_detail": detail}


def _projection_rows(body: dict) -> dict[tuple[str, str], dict]:
    spans = {}
    for record in loop_reply.records(body):
        record_id = record.get("id")
        result = record.get("canonical_result")
        if not evidence.is_loop_projection(result):
            continue
        if result.get("revision") is not None:
            _require(result["revision"] == body["scope"]["publication"]["token"],
                     "retained projection revision differs from the reply publication")
        for span in result["spans"]:
            pointer = span.get("pointer") if isinstance(span, dict) else None
            _require(isinstance(record_id, str) and bool(record_id)
                     and isinstance(pointer, str) and bool(pointer)
                     and (record_id, pointer) not in spans, "invalid or duplicate span pointer")
            spans[record_id, pointer] = span
    return spans


def _exact_witnesses(body: dict, source_root: Path) -> tuple[list[dict], list[dict]]:
    spans = _projection_rows(body)
    verified_text = {}

    @lru_cache(maxsize=2)
    def load_source(path: str, digest: str) -> bytes:
        original = reopen_exact(source_root, path)
        try:
            with original.open("rb") as stream:
                data = stream.read(SOURCE_LIMIT + 1)
        except OSError as error:
            raise PreparationError("source_unreadable") from error
        if len(data) > SOURCE_LIMIT or b"\0" in data:
            raise PreparationError("source_outside_text_limit")
        if common.sha256_bytes(data) != digest:
            raise PreparationError("exported_source_digest_differs")
        return data

    def original(span: dict) -> bytes:
        path, digest = span.get("path"), span.get("content_hash")
        if not isinstance(path, str) or not isinstance(digest, str) or _SHA256.fullmatch(digest) is None:
            raise PreparationError("missing_source_identity")
        data = load_source(path, digest)
        return excerpt_bytes(data, span.get("start_line"), span.get("end_line"))

    unverified = []
    delivered = []
    # Read complete carriers before resolving same_as, which may refer forward.
    for reference, span in spans.items():
        text = span.get("text")
        if not isinstance(text, str) or not text or span.get("truncated") is not False or span.get("overlaps"):
            continue
        try:
            expected = original(span)
            if text.encode("utf-8") == expected:
                verified_text[reference] = expected
        except (PreparationError, UnicodeError):
            continue

    for reference, span in spans.items():
        location = {"record": reference[0], "pointer": reference[1],
                    **{key: span.get(key) for key in ("path", "start_line", "end_line", "content_hash")}}
        raw = verified_text.get(reference)
        if raw is None and span.get("truncated") is False and not span.get("overlaps"):
            shared = span.get("same_as")
            if isinstance(shared, dict):
                carrier = shared.get("record"), shared.get("pointer")
                if all(isinstance(value, str) for value in carrier) and carrier in verified_text:
                    try:
                        expected = original(span)
                        parent = spans[carrier]
                        whole = verified_text[carrier]
                        if span.get("within") is True:
                            if (parent["path"], parent["content_hash"]) == (span["path"], span["content_hash"]):
                                if (parent["start_line"] <= span["start_line"]
                                        <= span["end_line"] <= parent["end_line"] and expected in whole):
                                    raw = expected
                        elif expected == whole:
                            raw = expected
                    except (PreparationError, UnicodeError):
                        pass
        if raw is not None:
            delivered.append({
                **{key: span[key] for key in ("path", "start_line", "end_line")},
                "source_sha256": span["content_hash"],
                "excerpt_sha256": common.sha256_bytes(raw),
                "record": reference[0],
                "pointer": reference[1],
            })
        else:
            if span.get("truncated"):
                reason = "truncated"
            elif span.get("overlaps"):
                reason = "overlap_needs_source_review"
            elif span.get("same_as"):
                reason = "unverified_shared_carrier"
            else:
                reason = "source_bytes_unavailable_or_different"
            unverified.append({**location, "reason": reason})
    return delivered, unverified


def capture_case(case: dict, body: dict, delivered_text: str, *, elapsed_ms: int,
                 source_root: Path) -> dict:
    """Capture a terminal native /3 reply and independently check delivered text witnesses."""
    _require(isinstance(delivered_text, str) and json.loads(delivered_text) == body,
             "parsed native reply differs from the delivered JSON text")
    _require(loop_reply.is_v3(body) and not loop_reply.is_ticket(body),
             "expected a terminal native /3 reply")
    scope = body.get("scope") or {}
    _require(scope.get("index_id") == case["index_id"]
             and (scope.get("publication") or {}).get("token") == case["publication"],
             "native reply index or publication differs")
    model_work = body.get("model_work") or {}
    _require(all(model_work.get(key) in (None, 0) for key in ("focus_calls", "context_calls", "focus_pairs"))
             and model_work.get("focus_profile") is None,
             "native no-model reply reports optional model work")
    partial = body.get("partial")
    reason = partial.get("reason") if isinstance(partial, dict) else None
    _require(partial is None or reason in {"deadline", "budget", "bytes", "stale", "refused"},
             "native reply still queued or has unknown partial status")
    row = _row(case, delivered_text, elapsed_ms)
    if partial is None:
        disposition = "completed"
    else:
        disposition = {"deadline": "timeout", "refused": "refused"}.get(reason, "partial")
    row.update(disposition=disposition, terminal_detail=partial, scope=scope,
               retained_record_metadata=[{key: record[key] for key in ("id", "operation", "arguments", "role")
                                         if key in record} for record in loop_reply.records(body)],
               retained_record_uses=loop_reply.record_uses(body),
               omitted_evidence=body.get("omitted_evidence"),
               native_read_failures=loop_reply.read_failures(body))
    row["delivered_passages"], row["unverified_passages"] = _exact_witnesses(body, source_root)
    work = body.get("work") or {}
    row["costs"].update({f"reported_native_work_{key}": _count(work.get(key)) for key in _WORK_COUNTS})
    row["costs"]["reported_native_elapsed_ms"] = _count(body.get("elapsed_ms"))
    return row
