# SPDX-License-Identifier: Apache-2.0
"""Reauthenticate an ordinary-tools export against its recorded Git archive."""
from __future__ import annotations

import hashlib
import os
from pathlib import Path, PurePosixPath
import stat
import subprocess
import tarfile

import common
from gather_context_sources import _linked, _path_parts


class _ArchiveReader:
    def __init__(self, stream):
        self.stream = stream
        self.digest = hashlib.sha256()
        self.bytes = 0

    def read(self, size=-1):
        data = self.stream.read(size)
        self.digest.update(data)
        self.bytes += len(data)
        return data


def _check_member(root: Path, archive: tarfile.TarFile, member: tarfile.TarInfo) -> tuple[str, bool]:
    relative = member.name.rstrip("/") if member.isdir() else member.name
    parts = _path_parts(relative)
    path = root
    for part in parts:
        path /= part
        info = path.lstat()
        common.require(not _linked(info), "authenticated export contains a link")
    if member.isdir():
        common.require(stat.S_ISDIR(info.st_mode), "archived directory type differs")
        return relative, False
    common.require(member.isfile() and stat.S_ISREG(info.st_mode) and info.st_size == member.size,
                   "archived file type or length differs")
    expected = archive.extractfile(member)
    with expected, path.open("rb") as observed:
        while (block := expected.read(1 << 20)):
            common.require(observed.read(len(block)) == block, "archive and export source bytes differ")
        common.require(not observed.read(1), "export source has additional bytes")
    return relative, True


def _verify_stream(root: Path, stream, receipt: dict) -> dict:
    source = _ArchiveReader(stream)
    roster = set()
    files = 0
    with tarfile.open(fileobj=source, mode="r|") as archive:
        for member in archive:
            relative, is_file = _check_member(root, archive, member)
            common.require(relative not in roster, "duplicate archive path")
            roster.add(relative)
            files += is_file
    while source.read(1 << 20):
        pass
    common.require((source.bytes, source.digest.hexdigest()) ==
                   (receipt["archive"]["bytes"], receipt["archive"]["sha256"]),
                   "regenerated archive does not match the original archive pin")
    common.require(files == receipt["files"], "archive file count differs")
    exported = set()
    for directory, directories, filenames in os.walk(root, followlinks=False):
        for name in directories + filenames:
            path = Path(directory) / name
            common.require(not _linked(path.lstat()), "export contains an unapproved linked path")
            exported.add(path.relative_to(root).as_posix())
    common.require(exported == roster, "export has paths outside the authenticated archive")
    return {"archive_sha256": source.digest.hexdigest(), "bytes": source.bytes, "files": files}


def verify_export(root: Path, receipt: dict, *, git: Path) -> dict:
    """Read the recorded archive, or stream it from the already provisioned checkout."""
    recorded = Path(receipt["archive"]["path"])
    if recorded.is_file():
        with recorded.open("rb") as stream:
            return _verify_stream(root, stream, receipt)
    checkout = Path(receipt["checkout"])
    common.require(checkout.is_absolute() and checkout.resolve(strict=True) == checkout
                   and checkout.is_dir(), "recorded source checkout is unavailable")
    common.require(git.is_absolute() and git.is_file(), "absolute Git executable required")
    command = [str(git), "-C", str(checkout), "archive", "--format=tar", receipt["commit"]]
    with subprocess.Popen(command, stdin=subprocess.DEVNULL, stdout=subprocess.PIPE,
                          stderr=subprocess.DEVNULL) as process:
        try:
            result = _verify_stream(root, process.stdout, receipt)
        finally:
            if process.poll() is None and process.stdout is not None:
                process.stdout.close()
        common.require(process.wait() == 0, "pinned Git archive failed")
    return result
