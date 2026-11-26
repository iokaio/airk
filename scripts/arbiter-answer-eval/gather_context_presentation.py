# SPDX-License-Identifier: Apache-2.0
"""Presentation-only replay of authenticated first Answer text."""

from __future__ import annotations

from collections import Counter
from dataclasses import dataclass
import json

import common
import evidence
from complete_evidence import CompleteEvidence


@dataclass(frozen=True)
class PresentationPair:
    original: bytes
    grouped: bytes
    original_output_sha256: str
    grouped_output_sha256: str


def _output_digest(text: str) -> str:
    items = [{"type": "inputText", "text": text}]
    return common.sha256_bytes(common.canonical_json(items))


def _part_ranges(body: dict) -> list[list[evidence.Span]]:
    ranges_by_record = {
        record["id"]: [
            span for span in evidence.extract(record["canonical_result"])
            if span.start is not None and span.end is not None
        ]
        for record in body.get("records", [])
    }
    uses_by_part = {}
    for use in body.get("record_uses", []):
        uses_by_part.setdefault(use["part"], set()).add(use["record"])
    result = []
    for part in body["parts"]:
        record_ids = uses_by_part.get(part["id"], set()).copy()
        for need in part.get("sub_needs", []):
            record_ids.update(need["evidence"])
        result.append([
            span for record_id in record_ids
            for span in ranges_by_record.get(record_id, [])
        ])
    return result


def render(native_reply: dict, archived_text: bytes, *, treatment: CompleteEvidence,
           recorded_output_sha256: str) -> PresentationPair:
    """Reorder supplemental passages without changing ordinary Answer content.

    The caller authenticates the retained session and first dispatch with
    run_session.verify_complete_evidence before providing its observed digest.
    Both returned byte strings are actor text, with one Codex inputText item.
    """
    common.require(not native_reply.get("isError"), "expected a successful native Answer")
    native_content = {"content": common.envelope_content(native_reply)}
    expected = treatment.augment(native_content)
    original_text = archived_text.decode("utf-8")
    delivered = {"content": [{"type": "text", "text": original_text}]}
    treatment.verify_dispatch(delivered, recorded_output_sha256)
    common.require(delivered == expected, "archived text differs from ordinary Answer and frozen passages")

    original = json.loads(original_text)
    common.require(isinstance(original.get("parts"), list), "ordinary Answer is missing its parts")
    part_ranges = _part_ranges(original)

    def rank(passage: dict) -> int:
        citation = passage["citation"]
        for index, ranges in enumerate(part_ranges):
            if any(
                span.path == citation["path"]
                and span.start <= citation["end_line"]
                and span.end >= citation["start_line"]
                for span in ranges
            ):
                return index
        return len(part_ranges)

    passages = original["source_passages"]
    ordinary = {key: value for key, value in original.items() if key != "source_passages"}
    grouped = {}
    for key, value in ordinary.items():
        grouped[key] = value
        if key == "parts":
            # Stable sort keeps original order within a part and for unmatched passages.
            grouped["source_passages"] = sorted(passages, key=rank)
    grouped_text = json.dumps(grouped, ensure_ascii=False, separators=(",", ":"))
    grouped_bytes = grouped_text.encode("utf-8")
    reordered = json.loads(grouped_text)
    common.require(Counter(map(common.canonical_json, reordered.pop("source_passages")))
                   == Counter(map(common.canonical_json, passages))
                   and reordered == ordinary and len(grouped_bytes) == len(archived_text),
                   "grouped presentation changed content or byte count")
    return PresentationPair(archived_text, grouped_bytes, recorded_output_sha256,
                            _output_digest(grouped_text))
