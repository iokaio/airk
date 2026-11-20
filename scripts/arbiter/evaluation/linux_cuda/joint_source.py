# SPDX-License-Identifier: Apache-2.0
"""Verify the complete GNU scientific source view against its original two Git archives."""
import hashlib
import os
from pathlib import Path
import stat
import tarfile

BASE_COMMIT = "f3978a12f053fbdaebcd673e6bf720adbbd23f3a"
BASE_ARCHIVE = "759ea88dce9b3feb8f1a95ee84ca20d1e19c3fb279f04eb36fb90cd207ccda94"
COMMIT = "fdf13652278de16f53eed4cbb8af0ab05fd2abb1"
DELTA_ARCHIVE = "a02463332625bd43ba5c937118f73f6e7615a26ce34ba02e9770bf52fc8407cd"


def append_archive(rows, archive, digest, commit, *, delta):
    count = 0
    with archive.open("rb") as stream:
        if hashlib.file_digest(stream, "sha256").hexdigest() != digest:
            raise ValueError("Git source archive changed")
        stream.seek(0)
        with tarfile.open(fileobj=stream, mode="r:") as tar:
            names = set()
            for member in tar:
                if tar.pax_headers.get("comment") != commit:
                    raise ValueError("unexpected Git source commit")
                if member.isdir():
                    continue
                name = member.name
                if (not member.isfile() or name in names or Path(name).is_absolute()
                        or ".." in Path(name).parts or (delta and not name.startswith("crates/"))):
                    raise ValueError("unexpected Git source entry")
                names.add(name)
                with tar.extractfile(member) as payload:
                    rows[name] = (member.size, hashlib.file_digest(payload, "sha256").hexdigest())
                count += 1
    return count


def inventory(base_archive, delta_archive):
    rows = {}
    original = append_archive(rows, base_archive, BASE_ARCHIVE, BASE_COMMIT, delta=False)
    changed = append_archive(rows, delta_archive, DELTA_ARCHIVE, COMMIT, delta=True)
    if (original, changed, len(rows)) != (10143, 35, 10151):
        raise ValueError("joint scientific source roster differs")
    return rows


def authenticate(root):
    from configure_ort import CAMPAIGN, REPO
    from verified_inputs import identity

    uid = identity()
    approved = REPO / "artifacts/v190/final-plan/gnu-joint-source"
    if root != approved or root.resolve(strict=True) != approved or root.stat().st_uid != uid:
        raise ValueError("designated same-user joint GNU source required")
    rows = inventory(approved.with_name("gnu-r7-source.tar"),
                     CAMPAIGN / "linux-joint-source-delta-r1/native-source-delta.tar")
    checked = set()
    for parent, dirs, names in os.walk(root):
        for name in dirs:
            path = Path(parent) / name
            if path.is_symlink() or path.stat().st_uid != uid:
                raise ValueError("unexpected joint source directory")
        for name in names:
            path = Path(parent) / name
            key = path.relative_to(root).as_posix()
            info = path.lstat()
            if (key not in rows or not stat.S_ISREG(info.st_mode) or info.st_uid != uid
                    or info.st_size != rows[key][0]):
                raise ValueError("unexpected joint source entry: " + key)
            with path.open("rb") as stream:
                if hashlib.file_digest(stream, "sha256").hexdigest() != rows[key][1]:
                    raise ValueError("changed joint source: " + key)
            checked.add(key)
    if checked != rows.keys():
        raise ValueError("incomplete joint source")
    return {"base": BASE_COMMIT, "commit": COMMIT, "base_archive": BASE_ARCHIVE,
            "native_delta": DELTA_ARCHIVE, "files": len(rows)}
