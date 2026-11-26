# SPDX-License-Identifier: Apache-2.0
"""Bounded ordinary source reads for a verified Gather Context evaluation export."""

from __future__ import annotations

from collections import deque
import json
import os
from pathlib import Path
import re
import stat


MAX_INVENTORY_ENTRIES = 50_000
MAX_LIST_PATHS = 256
MAX_FIND_FILES = 192
MAX_FIND_LINES = 96
MAX_SCAN_BYTES = 64 * 1024 * 1024
MAX_FILE_BYTES = 2 * 1024 * 1024
MAX_READ_BYTES = 48 * 1024
MAX_READ_LINES = 400
MAX_PREVIEW = 240
MAX_LITERAL = 512

_PATH = {"type": "string", "description": "Repository-relative path using / separators."}
_PREFIX = {"type": "string", "description": "Optional repository-relative directory using / separators."}
_LINE = {"type": "integer", "minimum": 1}
_DEVICE = re.compile(r"(?:con|prn|aux|nul|com[1-9¹²³]|lpt[1-9¹²³])(?:\.|$)", re.IGNORECASE)
_UNSAFE = re.compile(r'[\\:*?<>|"\x00-\x1f]')

TOOLS = [
    {
        "name": "source_list",
        "description": "List files in the frozen source export or one directory. A cutoff means the returned paths are a subset.",
        "inputSchema": {
            "type": "object", "properties": {"prefix": _PREFIX},
            "required": [], "additionalProperties": False,
        },
    },
    {
        "name": "source_find",
        "description": "Find a literal on individual UTF-8 source lines. Returns scoped counts and line previews.",
        "inputSchema": {
            "type": "object",
            "properties": {
                "literal": {"type": "string", "minLength": 1, "maxLength": MAX_LITERAL},
                "prefix": _PREFIX, "case_sensitive": {"type": "boolean"},
            },
            "required": ["literal"], "additionalProperties": False,
        },
    },
    {
        "name": "source_read",
        "description": "Read an inclusive one-based line range from a file in the frozen export.",
        "inputSchema": {
            "type": "object",
            "properties": {"path": _PATH, "start_line": _LINE, "end_line": _LINE},
            "required": ["path", "start_line", "end_line"], "additionalProperties": False,
        },
    },
]


class Refused(ValueError):
    """The supplied selector or range is outside this tool's authority."""


def _path_parts(value: str, *, allow_root: bool = False) -> list[str]:
    if type(value) is not str or len(value) > 1024 or any(0xD800 <= ord(char) <= 0xDFFF for char in value):
        raise Refused("Expected a bounded relative path")
    if not value and allow_root:
        return []
    parts = value.split("/")
    if any(part in ("", ".", "..") or part.casefold() == ".git"
           or part.endswith((".", " ")) or _UNSAFE.search(part) or _DEVICE.match(part) for part in parts):
        raise Refused("Invalid relative source path")
    return parts


def _linked(info: os.stat_result) -> bool:
    return stat.S_ISLNK(info.st_mode) or bool(
        getattr(info, "st_file_attributes", 0) & stat.FILE_ATTRIBUTE_REPARSE_POINT
    )


def _select(root: Path, parts: list[str], *, directory: bool) -> Path:
    selected = root
    for offset, part in enumerate(parts, 1):
        selected /= part
        info = selected.lstat()
        if _linked(info):
            raise Refused("Linked source paths are outside the frozen export")
        needs_directory = offset < len(parts) or directory
        if not (stat.S_ISDIR(info.st_mode) if needs_directory else stat.S_ISREG(info.st_mode)):
            raise Refused("Source path has the wrong file type")
    resolved = selected.resolve(strict=True)
    if not resolved.is_relative_to(root):
        raise Refused("Source path is outside the frozen export")
    return resolved


def _relative(root: Path, path: Path) -> str:
    return path.relative_to(root).as_posix() if path != root else ""


def _inventory(root: Path, directory: Path) -> tuple[list[Path], dict]:
    pending = deque([directory])
    files = []
    visited = excluded = unavailable = 0
    truncated = False
    while pending and not truncated:
        entries = []
        try:
            with os.scandir(pending.popleft()) as stream:
                for entry in stream:
                    if visited == MAX_INVENTORY_ENTRIES:
                        truncated = True
                        break
                    entries.append(entry)
                    visited += 1
        except OSError:
            unavailable += 1
        for entry in sorted(entries, key=lambda item: item.name):
            try:
                _path_parts(entry.name)
            except Refused:
                excluded += 1
                continue
            try:
                info = entry.stat(follow_symlinks=False)
            except OSError:
                unavailable += 1
                continue
            path = Path(entry.path)
            if _linked(info):
                excluded += 1
            elif stat.S_ISDIR(info.st_mode):
                pending.append(path)
            elif stat.S_ISREG(info.st_mode) and len(_relative(root, path)) <= 1024:
                files.append(path)
            else:
                excluded += 1
    cutoffs = []
    if truncated:
        cutoffs.append("inventory_entry_limit")
    if unavailable:
        cutoffs.append("inventory_unavailable")
    return sorted(files, key=lambda path: _relative(root, path)), {
        "prefix": _relative(root, directory), "visited_entries": visited,
        "enumerated_files": len(files), "excluded_entries": excluded, "unavailable_entries": unavailable,
        "cutoffs": cutoffs,
    }


def _list(root: Path, directory: Path) -> dict:
    files, receipt = _inventory(root, directory)
    if len(files) > MAX_LIST_PATHS:
        receipt["cutoffs"].append("returned_path_limit")
    return {**receipt, "complete": not receipt["cutoffs"],
            "paths": [_relative(root, file) for file in files[:MAX_LIST_PATHS]]}


def _find(root: Path, directory: Path, literal: str, case_sensitive: bool) -> dict:
    if (type(literal) is not str or not 1 <= len(literal) <= MAX_LITERAL
            or any(char in literal for char in ("\r", "\n", "\0"))
            or any(0xD800 <= ord(char) <= 0xDFFF for char in literal)):
        raise Refused("Expected a literal within one source line")
    if type(case_sensitive) is not bool:
        raise Refused("case_sensitive must be a boolean")
    pattern = re.compile(re.escape(literal), 0 if case_sensitive else re.IGNORECASE)
    files, receipt = _inventory(root, directory)
    read_bytes = examined = excluded_text = oversized = unavailable = matched_files = occurrences = matched_lines = 0
    counts = []
    matches = []
    for file in files:
        path = _relative(root, file)
        allowance = min(MAX_FILE_BYTES + 1, MAX_SCAN_BYTES - read_bytes)
        if allowance <= 0:
            receipt["cutoffs"].append("scan_byte_limit")
            break
        try:
            with _select(root, _path_parts(path), directory=False).open("rb") as stream:
                raw = stream.read(allowance)
        except (OSError, Refused):
            unavailable += 1
            continue
        read_bytes += len(raw)
        if len(raw) == allowance:
            if allowance <= MAX_FILE_BYTES:
                receipt["cutoffs"].append("scan_byte_limit")
                break
            oversized += 1
            continue
        try:
            text = raw.decode("utf-8")
        except UnicodeDecodeError:
            excluded_text += 1
            continue
        if "\0" in text:
            excluded_text += 1
            continue
        examined += 1
        file_occurrences = 0
        for line, source in enumerate(text.split("\n"), start=1):
            source = source.removesuffix("\r")
            first = pattern.search(source)
            if first is None:
                continue
            count = sum(1 for _ in pattern.finditer(source))
            file_occurrences += count
            matched_lines += 1
            if len(matches) < MAX_FIND_LINES:
                offset = max(0, first.start() - 40)
                end = min(len(source), offset + MAX_PREVIEW)
                matches.append({"path": path, "start_line": line, "end_line": line,
                                "occurrences": count, "preview": source[offset:end],
                                "preview_start_column": offset + 1, "preview_clipped": offset > 0 or end < len(source)})
        if file_occurrences:
            occurrences += file_occurrences
            matched_files += 1
            if len(counts) < MAX_FIND_FILES:
                counts.append({"path": path, "occurrences": file_occurrences})
    if oversized:
        receipt["cutoffs"].append("file_byte_limit")
    if unavailable:
        receipt["cutoffs"].append("source_unavailable")
    results_cutoffs = []
    if len(counts) < matched_files:
        results_cutoffs.append("returned_count_limit")
    if len(matches) < matched_lines:
        results_cutoffs.append("returned_line_limit")
    return {**receipt, "complete": not receipt["cutoffs"] and not results_cutoffs,
            "scan_complete": not receipt["cutoffs"], "results_cutoffs": results_cutoffs,
            "literal": literal, "case_sensitive": case_sensitive, "examined_files": examined,
            "read_bytes": read_bytes, "excluded_non_utf8_or_binary_files": excluded_text,
            "oversized_files": oversized, "unavailable_files": unavailable, "matched_files": matched_files,
            "matched_lines": matched_lines, "occurrences": occurrences, "counts": counts, "matches": matches}


def _read(root: Path, file: Path, start: int, end: int) -> dict:
    if type(start) is not int or type(end) is not int or not 1 <= start <= end:
        raise Refused("Expected an inclusive one-based line range")
    lines = []
    read_bytes = delivered_bytes = 0
    cutoff = None
    with file.open("rb") as stream:
        for number in range(1, end + 1):
            if len(lines) == MAX_READ_LINES:
                cutoff = "returned_line_limit"
                break
            allowance = MAX_FILE_BYTES - read_bytes
            if allowance <= 0:
                cutoff = "read_byte_limit"
                break
            raw = stream.readline(allowance + 1)
            read_bytes += len(raw)
            if not raw:
                cutoff = "end_of_file"
                break
            if len(raw) > allowance:
                cutoff = "read_byte_limit"
                break
            try:
                text = raw.decode("utf-8")
            except UnicodeDecodeError:
                raise Refused("Source is not UTF-8 text") from None
            if "\0" in text:
                raise Refused("Source is not UTF-8 text")
            if number < start:
                continue
            if delivered_bytes + len(raw) > MAX_READ_BYTES:
                cutoff = "returned_byte_limit"
                break
            delivered_bytes += len(raw)
            lines.append(text)
    path = _relative(root, file)
    passages = [{"path": path, "start_line": start, "end_line": start + len(lines) - 1,
                 "text": "".join(lines)}] if lines else []
    return {"path": path, "requested_start_line": start, "requested_end_line": end,
            "complete": cutoff is None, "cutoff": cutoff, "read_bytes": read_bytes,
            "delivered_source_bytes": delivered_bytes, "passages": passages}


def dispatch(root: Path, tool: str, arguments: dict) -> dict:
    """Dispatch a dynamic tool against one export already authenticated by the controller."""
    try:
        if not isinstance(root, Path) or not root.is_absolute() or _linked(root.lstat()) or not root.is_dir():
            raise Refused("A verified absolute export root is required")
        root = root.resolve(strict=True)
        schema = next((item["inputSchema"] for item in TOOLS if item["name"] == tool), None)
        if (schema is None or type(arguments) is not dict or not set(arguments) <= set(schema["properties"])
                or not set(schema["required"]) <= set(arguments)):
            raise Refused("Unknown source tool or invalid arguments")
        if tool == "source_read":
            path = _select(root, _path_parts(arguments["path"]), directory=False)
            payload = _read(root, path, arguments["start_line"], arguments["end_line"])
        else:
            directory = _select(root, _path_parts(arguments.get("prefix", ""), allow_root=True), directory=True)
            if tool == "source_list":
                payload = _list(root, directory)
            else:
                payload = _find(root, directory, arguments["literal"], arguments.get("case_sensitive", True))
    except (OSError, Refused) as error:
        message = str(error) if isinstance(error, Refused) else "Requested source is unavailable in the frozen export"
        return {"isError": True, "content": [{"type": "text", "text": json.dumps(
            {"complete": False, "error": message}, ensure_ascii=False)}]}
    return {"content": [{"type": "text", "text": json.dumps(payload, ensure_ascii=False, separators=(",", ":"))}]}
