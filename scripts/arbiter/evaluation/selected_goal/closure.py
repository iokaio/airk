# SPDX-License-Identifier: Apache-2.0
"""Read authenticated source bytes; never import preserved implementation code."""
import hashlib
import json
import os
from pathlib import Path
import re
import stat

MAX_JSON = 64 << 20


def require(condition, message):
    if not condition:
        raise ValueError(message)


def unique_pairs(pairs):
    result = {}
    for key, value in pairs:
        require(key not in result, "duplicate JSON key")
        result[key] = value
    return result


def plain(path):
    path = Path(os.path.abspath(path))
    for item in (path, *path.parents):
        info = item.lstat()
        require(not stat.S_ISLNK(info.st_mode) and not getattr(info, "st_file_attributes", 0) & 0x400,
                "linked or reparse input")
    info = path.stat()
    require(stat.S_ISREG(info.st_mode) and info.st_nlink == 1, "non-regular or hard-linked input")
    return path, info


def checked_bytes(reference, limit=MAX_JSON):
    require(isinstance(reference, dict) and set(reference) == {"path", "sha256", "bytes"}, "exact file reference required")
    require(isinstance(reference["path"], str) and Path(reference["path"]).is_absolute(), "absolute input path required")
    require(isinstance(reference["sha256"], str) and re.fullmatch(r"[0-9a-f]{64}", reference["sha256"]), "invalid SHA256")
    require(type(reference["bytes"]) is int and 0 <= reference["bytes"] <= limit, "input size bound")
    path, before = plain(reference["path"])
    require(before.st_size == reference["bytes"], "input length changed")
    with path.open("rb") as stream:
        opened = os.fstat(stream.fileno())
        require((opened.st_dev, opened.st_ino) == (before.st_dev, before.st_ino), "input identity changed")
        raw = stream.read(limit + 1)
        after = os.fstat(stream.fileno())
    _, final = plain(path)
    require(len(raw) == reference["bytes"] and hashlib.sha256(raw).hexdigest() == reference["sha256"], "input bytes changed")
    require((before.st_dev, before.st_ino, before.st_size, before.st_mtime_ns) ==
            (after.st_dev, after.st_ino, after.st_size, after.st_mtime_ns) ==
            (final.st_dev, final.st_ino, final.st_size, final.st_mtime_ns), "input changed while reading")
    return raw


def checked_json(reference):
    return json.loads(checked_bytes(reference), object_pairs_hook=unique_pairs)


def source_body(row, repository):
    """Validate one exact map row; tolerate only its explicit SPDX prefix."""
    relative = Path(row["destination"])
    require(not relative.is_absolute() and ".." not in relative.parts, "foreign maintained destination")
    path = Path(repository).resolve() / relative
    require(relative.parts[0] in {"scripts", "tools", "crates", "packages"}, "unmaintained source destination")
    size = row.get("maintained_bytes")
    require(type(size) is int, "maintained byte count required")
    raw = checked_bytes({"path": str(path), "sha256": row["maintained_sha256"], "bytes": size})
    prefix = row.get("spdx_prefix", "")
    changes = row.get("changes", [])
    if changes and isinstance(changes[0], dict):
        require(len(changes) == 1 and changes[0].get("kind") == "spdx-prefix", "unsupported source transformation")
        require(not prefix, "ambiguous prefix declarations")
        prefix = changes[0]["prefix"]
    elif changes:
        require(changes == ["exact raw bytes; preservation path only"], "unsupported source transformation")
    require(prefix in ("", "# SPDX-License-Identifier: Apache-2.0\n", "# SPDX-License-Identifier: Apache-2.0\r\n",
                       "// SPDX-License-Identifier: Apache-2.0\n"), "unreviewed prefix")
    require(raw.startswith(prefix.encode()), "declared prefix missing")
    original = raw[len(prefix.encode()):]
    require(hashlib.sha256(original).hexdigest() == row["original_sha256"], "original body differs")
    original_size = row.get("original_bytes", row.get("bytes"))
    require(type(original_size) is int and len(original) == original_size, "original length differs")
    return {"path": str(path), "sha256": row["maintained_sha256"], "bytes": size}


def resolve_sources(selections, maps, repository):
    """Exact original path AND digest selection; no filename search/fallback."""
    rows = [row for document in maps for row in document["files"]]
    resolved = {}
    for role, selected in selections.items():
        require(set(selected) == {"source", "original_sha256"}, "exact source selection required")
        matches = [r for r in rows if r.get("source") == selected["source"] and
                   r.get("original_sha256") == selected["original_sha256"]]
        require(len(matches) == 1, "missing or ambiguous exact source role: " + role)
        resolved[role] = source_body(matches[0], repository)
    return resolved
