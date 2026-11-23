# SPDX-License-Identifier: Apache-2.0
"""Locations and evidence text in AIRK tool results, and the coverage rule.

Result shapes (read from crates/airk-mcp and the crates it serializes):

* search, read_file, context, references: ``hits[] = {citation: {path,
  start_line, end_line, ...}, text, truncated}``; references nest them under
  ``evidence``. Lines are 1-based and inclusive.
* outline and symbols (default compact): ``items[] = {name, lines: [s, e],
  file?: index}`` with ``files[] = {path, ...}``; ``compact=false`` gives
  ``items[] = {name, citation: {...}}``.
* answer: ``records[].canonical_result`` holds the native body of each inner
  read; ``records[].arguments`` are the read's arguments, not evidence.
* parse: ``evidence[] = {citation, text, fact, step}`` plus ``steps[].result``.
* combined sessions: ``members[].result`` with the per-tool body.
* answer /3 (the loop): ``records[].canonical_result`` as above (a record has
  no scope); ``parts[].sub_needs[]`` cite record ids. A compact span
  ``{path, lines: [s, e] | start_line/end_line, excerpt}`` (changes entry 16,
  L3) is evidence text at that place. The v3 bookkeeping keys are skipped.

Coverage rule 2 (decision of 21 Nov 2026): a text span covers a note only
when it overlaps at least min(5, the note's line count) lines of the note's
range (or of an alternative's range, on that alternative's path). A path with
no line span still covers only through the symbol rule.

A span has a kind: ``text`` (evidence text at that place), ``fact`` (an
outline/symbol row: a name at a place) or ``pointer`` (a location without
text, such as a definition hint). The coverage rule of formats.md counts
evidence text, so by default only ``text`` spans cover a note by line overlap;
``count_fact_rows=True`` also counts ``fact`` rows.
"""

from __future__ import annotations

import dataclasses
import hashlib
import json
import re
from typing import Any, Iterable

# Keys whose values are request arguments, plans or receipts, never evidence.
SKIP_KEYS = frozenset({
    "arguments", "args", "plan", "source_verification", "read", "requires_input", "assessment_history",
    "assessment", "applications", "work", "gaps", "scope", "guidance", "provenance", "execution",
    "publication", "current_publication", "scope_observation", "semantic_readiness", "discovery_readiness",
    "freshness", "indexing", "bind",
    # Answer /3 bookkeeping (CONTRACT.md "Answer loop wire contract version 3"): decisions, the decision log
    # summary, record uses, Union, partial and ticket markers, degradations, model work, the ids the frame
    # omitted, and per-part rounds, follow-ups run and unrun reads. None of them is evidence.
    "decisions", "decision_log", "record_uses", "union", "partial", "ticket", "degradations", "model_work",
    "omitted_evidence", "rounds", "follow_ups_run", "unrun_reads",
})


@dataclasses.dataclass(frozen=True)
class Span:
    path: str
    start: int | None
    end: int | None
    text: str
    kind: str  # text | fact | pointer | path


def norm_path(path: str) -> str:
    path = path.replace("\\", "/")
    while path.startswith("./"):
        path = path[2:]
    return path


def path_matches(result_path: str, note_path: str) -> bool:
    a, b = norm_path(result_path), norm_path(note_path)
    return a == b or a.endswith("/" + b) or b.endswith("/" + a)


def _int(value: Any) -> int | None:
    return value if isinstance(value, int) and not isinstance(value, bool) else None


def _citation_span(citation: dict, owner: dict) -> Span | None:
    path = citation.get("path")
    if not isinstance(path, str):
        return None
    start, end = _int(citation.get("start_line")), _int(citation.get("end_line"))
    if start is None:
        location = citation.get("location")
        if isinstance(location, dict) and isinstance(location.get("Lines"), dict):
            start, end = _int(location["Lines"].get("start")), _int(location["Lines"].get("end"))
    text = owner.get("text")
    if isinstance(text, str) and text and not ("fact" in owner and owner.get("fact") is not None
                                               and _is_fact_record(owner.get("fact"))):
        kind = "text"
    elif isinstance(owner.get("name"), str):
        text = " ".join(str(owner.get(k)) for k in ("name", "signature", "qualified_name") if owner.get(k))
        kind = "fact"
    elif isinstance(text, str) and text:
        kind = "fact"
    else:
        text, kind = "", "pointer"
    if start is None:
        kind = "path" if kind != "pointer" else "pointer"
    return Span(norm_path(path), start, end if end is not None else start, text, kind)


def _is_fact_record(fact: Any) -> bool:
    """Parse evidence carries the originating record; a fact row (name, no text) is not a hit."""
    return isinstance(fact, dict) and "name" in fact and "text" not in fact


def extract(body: Any) -> list[Span]:
    spans: list[Span] = []
    _walk(body, spans)
    return spans


# An Answer /3 record's ``canonical_result`` is the kernel's span projection (Core CONTRACT.md, "One text, one
# copy in the reply"; kernel ``answer/union.rs`` ``project``).
LOOP_PROJECTION_PREFIX = "airk.arbiter.loop-evidence/"


def is_loop_projection(node: Any) -> bool:
    return (isinstance(node, dict) and isinstance(node.get("projection"), str)
            and node["projection"].startswith(LOOP_PROJECTION_PREFIX) and isinstance(node.get("spans"), list))


def _projection_spans(node: dict, spans: list[Span]) -> None:
    """Every location a loop record's projection delivered.

    - A ``spans`` row is one hit: ``pointer``, ``path``, ``start_line``, ``end_line``, ``content_hash``, and
      either its ``text`` or ``same_as`` (the row that carries its text; the reply carries each text once).
      Both are delivered text at the row's own location.
    - Each row's own ``path``, ``start_line`` and ``end_line`` always describe the hit's whole original span,
      also for a row trimmed because another row carries part of its text (``overlaps``). Lines are never
      derived from ``text``.
    - ``also_at`` is not read: every place it lists is also a row of its own, and its ``path:start-end``
      strings carry no hash and cannot be parsed safely (a path may hold a colon).
    - Only rows count. From trimming level 1 on, uncited hits are not rows (``hits_total`` is then larger
      than the row count) and an omitted record appears only in ``omitted_evidence``: neither is delivered.
    - ``rows`` (a record without hits, such as a symbols read) are names with locations: fact rows.

    Until 23 November 2026 these rows were read by the general walk, which knows ``excerpt`` and not
    ``text`` on a row without a ``citation``, so every loop span counted as a pointer and covered nothing."""
    for row in node["spans"]:
        if not isinstance(row, dict) or not isinstance(row.get("path"), str):
            continue
        path, start, end = norm_path(row["path"]), _int(row.get("start_line")), _int(row.get("end_line"))
        end = end if end is not None else start
        text = row.get("text") if isinstance(row.get("text"), str) else ""
        if text:
            spans.append(Span(path, start, end, text, "text" if start is not None else "path"))
        elif isinstance(row.get("same_as"), dict) and start is not None:
            spans.append(Span(path, start, end, "", "text"))
        else:
            spans.append(Span(path, start, end, "", "pointer"))
    for row in node.get("rows") or []:
        if isinstance(row, dict) and isinstance(row.get("path"), str):
            start, end = _int(row.get("start_line")), _int(row.get("end_line"))
            text = row.get("text") if isinstance(row.get("text"), str) else ""
            spans.append(Span(norm_path(row["path"]), start, end if end is not None else start, text,
                              "fact" if start is not None else "path"))


def _walk(node: Any, spans: list[Span]) -> None:
    if isinstance(node, list):
        for item in node:
            _walk(item, spans)
        return
    if not isinstance(node, dict):
        return
    if is_loop_projection(node):
        _projection_spans(node, spans)
        return
    # Compact outline/symbols: items[].lines with files[] path table.
    items, files = node.get("items"), node.get("files")
    compact_paths: list[str] = []
    if isinstance(files, list):
        compact_paths = [norm_path(f["path"]) for f in files if isinstance(f, dict) and isinstance(f.get("path"), str)]
    if isinstance(items, list) and compact_paths:
        for item in items:
            if isinstance(item, dict) and isinstance(item.get("lines"), list) and len(item["lines"]) == 2:
                index = item.get("file", 0)
                if isinstance(index, int) and 0 <= index < len(compact_paths):
                    text = " ".join(str(item.get(k)) for k in ("name", "signature") if item.get(k))
                    spans.append(Span(compact_paths[index], _int(item["lines"][0]), _int(item["lines"][1]), text, "fact"))
    citation = node.get("citation")
    if isinstance(citation, dict):
        span = _citation_span(citation, node)
        if span:
            spans.append(span)
    elif isinstance(node.get("path"), str) and "citation" not in node:
        start, end = _int(node.get("start_line")), _int(node.get("end_line"))
        lines = node.get("lines")
        if start is None and isinstance(lines, list) and len(lines) == 2:
            start, end = _int(lines[0]), _int(lines[1])
        excerpt = node.get("excerpt")
        if start is not None and isinstance(excerpt, str) and excerpt:
            spans.append(Span(norm_path(node["path"]), start, end if end is not None else start, excerpt, "text"))
        elif start is not None:
            spans.append(Span(norm_path(node["path"]), start, end if end is not None else start, "", "pointer"))
        else:
            rest = {k: v for k, v in node.items() if k != "path" and isinstance(v, (str, int))}
            spans.append(Span(norm_path(node["path"]), None, None, json.dumps(rest, ensure_ascii=False), "path"))
    for key, value in node.items():
        if key in SKIP_KEYS or key == "citation":
            continue
        if key == "items" and compact_paths:
            continue
        if key == "files" and compact_paths:
            # files[] rows of a compact listing are the path table; keep them as path-only entries.
            for f in value:
                if isinstance(f, dict) and isinstance(f.get("path"), str):
                    rest = {k: v for k, v in f.items() if k != "path" and isinstance(v, (str, int))}
                    spans.append(Span(norm_path(f["path"]), None, None, json.dumps(rest, ensure_ascii=False), "path"))
            continue
        if key == "fact" and isinstance(citation, dict):
            continue
        _walk(value, spans)


def overlaps(span: Span, lines: list | None) -> bool:
    if lines is None or span.start is None:
        return False
    lo, hi = lines[0], lines[1]
    return span.start <= hi and (span.end if span.end is not None else span.start) >= lo


def _locations(note: dict) -> Iterable[dict]:
    yield {"path": note["path"], "lines": note.get("lines"), "symbol": note.get("symbol")}
    for alternative in note.get("alternatives") or []:
        yield {"path": alternative["path"], "lines": alternative.get("lines"),
               "symbol": alternative.get("symbol") if alternative.get("symbol") is not None else note.get("symbol")}


COVERAGE_RULE = 3
"""Rule 1 (formats.md): one line of overlap. Rule 2 (primary agent, 21 Nov 2026, before the first judged
run): a span covers a location only if it overlaps at least min(5, the location's line count) of its lines.
Rule 3 (coordinator, 22 Nov 2026, before the first loop pass): rule 2's line overlap, and a path without a
line span covers by symbol only when the symbol is named as a whole identifier (names_symbol). Rules 1 and 2
matched the symbol as a substring, so a short symbol such as ``run`` or ``Spec`` matched almost any text of
its file; they keep that behaviour, because the one-turn passes are history and are not rescored."""

_QUALIFIED = re.compile(r"[A-Za-z_][A-Za-z0-9_]*(?:(?:::|\.)[A-Za-z_][A-Za-z0-9_]*)+")
_IDENTIFIER_CHARACTER = "[A-Za-z0-9_]"


def symbol_forms(symbol: str) -> list[str]:
    """The spellings that name a note's symbol: the symbol itself and, for a symbol qualified with ``.`` or
    ``::`` whose segments are all identifiers, its last segment (a tool row names ``add``, the note
    ``Schedule::add``). A heading or other free text has one form: itself."""
    forms = [symbol]
    if _QUALIFIED.fullmatch(symbol):
        forms.append(re.split(r"::|\.", symbol)[-1])
    return forms


def names_symbol(text: str, symbol: str) -> bool:
    """Whether ``text`` names ``symbol`` as a whole identifier: one of its forms with no identifier character
    (letter, digit or underscore) directly before or after it. ``run`` is named by ``"name": "run"`` and by
    ``fn run(``, and not by ``runner``, ``prune`` or ``run_pass``."""
    for form in symbol_forms(symbol):
        before = f"(?<!{_IDENTIFIER_CHARACTER})" if re.match(_IDENTIFIER_CHARACTER, form[0]) else ""
        after = f"(?!{_IDENTIFIER_CHARACTER})" if re.match(_IDENTIFIER_CHARACTER, form[-1]) else ""
        if re.search(before + re.escape(form) + after, text):
            return True
    return False


def overlap_lines(span: Span, lines: list | None) -> int:
    if lines is None or span.start is None:
        return 0
    end = span.end if span.end is not None else span.start
    return max(0, min(end, lines[1]) - max(span.start, lines[0]) + 1)


def required_overlap(lines: list, rule: int = COVERAGE_RULE) -> int:
    return 1 if rule == 1 else min(5, lines[1] - lines[0] + 1)


def covers(spans: list[Span], note: dict, count_fact_rows: bool = False, rule: int = COVERAGE_RULE) -> bool:
    """The coverage rule for one note over one result's spans (rule 3 unless ``rule`` names an earlier one).

    A note without a symbol is never covered by a path alone, under any rule."""
    line_kinds = {"text", "fact"} if count_fact_rows else {"text"}
    for location in _locations(note):
        same = [s for s in spans if path_matches(s.path, location["path"])]
        if not same:
            continue
        if location["lines"] is not None:
            need = required_overlap(location["lines"], rule)
            if any(s.kind in line_kinds and overlap_lines(s, location["lines"]) >= need for s in same):
                return True
        if location["lines"] is None and any(s.kind in line_kinds for s in same):
            return True
        # A path without a line span covers only by symbol: as a whole identifier from rule 3 on, as a
        # substring under the earlier rules (kept for the history they scored).
        symbol = location["symbol"]
        if symbol and isinstance(symbol, str):
            named = (lambda text: names_symbol(text, symbol)) if rule >= 3 else (lambda text: symbol in text)
            if any(s.start is None and named(s.text) for s in same):
                return True
    return False


def covered_notes(spans: list[Span], notes: list[dict], count_fact_rows: bool = False,
                  rule: int = COVERAGE_RULE) -> list[str]:
    return [note["id"] for note in notes if covers(spans, note, count_fact_rows, rule)]


def evidence_digest(body: Any) -> str:
    """Digest of the evidence an agent receives, in order: span locations, text, Answer paragraphs.

    Timings, receipts and publication tokens are excluded, so two replies with
    identical evidence in identical order have the same digest.
    """
    spans = [[s.path, s.start, s.end, hashlib.sha256(s.text.encode("utf-8")).hexdigest(), s.kind]
             for s in extract(body)]
    paragraphs = []
    if isinstance(body, dict) and isinstance(body.get("paragraphs"), list):
        paragraphs = [p.get("text") for p in body["paragraphs"] if isinstance(p, dict)]
    payload = json.dumps({"spans": spans, "paragraphs": paragraphs}, separators=(",", ":"), ensure_ascii=False)
    return hashlib.sha256(payload.encode("utf-8")).hexdigest()


CITATION = re.compile(
    r"(?<![\w/.\-])((?:[\w.@+\-]+/)*[\w@+\-][\w.@+\-]*\.[A-Za-z0-9]+|(?:[\w.@+\-]+/)+[\w.@+\-]+)"
    r":(\d+)(?:\s*[-–]\s*(\d+))?")


def cited_locations(text: str) -> list[dict]:
    seen, found = set(), []
    for match in CITATION.finditer(text or ""):
        path, start = match.group(1), int(match.group(2))
        end = int(match.group(3)) if match.group(3) else start
        key = (path, start, end)
        if key not in seen:
            seen.add(key)
            found.append({"path": path, "lines": [start, end]})
    return found


def backed(location: dict, spans: list[Span]) -> bool:
    """A cited location is backed when some logged result located evidence there."""
    for span in spans:
        if path_matches(span.path, location["path"]) and span.start is not None and overlaps(span, location["lines"]):
            return True
    return False
