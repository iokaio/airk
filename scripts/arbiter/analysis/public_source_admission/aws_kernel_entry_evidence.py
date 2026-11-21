# SPDX-License-Identifier: Apache-2.0
"""Capture a per-entry witness for a stopped Windows store with its original census digest."""
import hashlib
import json
import os
from pathlib import Path
import stat
import time


def capture(root: Path, output: Path, deadline: float):
    """Use the original path order and retain each digest preimage for exact difference reports."""
    digest = hashlib.sha256()
    count = total = 0
    pending = [root]
    with output.open("xb") as rows:
        while pending:
            path = pending.pop()
            if time.monotonic() >= deadline or count >= 500_000:
                raise ValueError("store witness exceeded time or entry limit")
            result = path.lstat()
            directory = stat.S_ISDIR(result.st_mode)
            if result.st_file_attributes & stat.FILE_ATTRIBUTE_REPARSE_POINT or not (directory or stat.S_ISREG(result.st_mode)):
                raise ValueError("unknown retained entry type")
            row = [path.relative_to(root).as_posix(), directory, result.st_dev, result.st_ino,
                   result.st_size, result.st_mtime_ns, result.st_file_attributes]
            if directory:
                pending.extend(sorted(path.iterdir(), reverse=True))
            else:
                total += result.st_size
                if total > 12 << 30:
                    raise ValueError("retained store exceeds original reservation")
                with path.open("rb") as stream:
                    handle = os.fstat(stream.fileno())
                    if (handle.st_dev, handle.st_ino, handle.st_size) != (result.st_dev, result.st_ino, result.st_size):
                        raise ValueError("open file does not match selected entry")
                    row.append(hashlib.file_digest(stream, "sha256").hexdigest())
            raw = json.dumps(row, ensure_ascii=True, separators=(",", ":")).encode() + b"\n"
            rows.write(raw)
            digest.update(raw)
            count += 1
    return {"bytes": total, "entries": count, "sha256": digest.hexdigest()}


def compare(first: Path, second: Path):
    """Report paths and changed fields; cap private examples while retaining the complete witnesses."""
    names = ("path", "directory", "volume", "file_id", "bytes", "mtime_ns", "attributes", "sha256")
    examples = []
    changed = 0
    with first.open("rb") as left, second.open("rb") as right:
        while True:
            one, two = left.readline(), right.readline()
            if not one and not two:
                break
            if one == two:
                continue
            changed += 1
            if len(examples) < 64:
                old = json.loads(one) if one else []
                new = json.loads(two) if two else []
                examples.append({"before_path": old[0] if old else None, "after_path": new[0] if new else None,
                                 "fields": [key for i, key in enumerate(names) if old[i:i+1] != new[i:i+1]]})
    return {"different_rows": changed, "examples": examples}
