# SPDX-License-Identifier: Apache-2.0
"""Authenticate the immutable joint CUDA source snapshot and original scientific native dependencies."""
import hashlib
import os
from pathlib import Path
import runpy
import tarfile

from configure_ort import CAMPAIGN, REPO, sha
from shipping_build import CORE, document

COMMIT = "f3978a12f053fbdaebcd673e6bf720adbbd23f3a"
ARCHIVE = "759ea88dce9b3feb8f1a95ee84ca20d1e19c3fb279f04eb36fb90cd207ccda94"
ASSEMBLY = "7ebbf2c3d2fdfec82d465ee1e35a6caad7c2b4a31e7c2291853484ae6b79b1eb"
ROOT = REPO / "artifacts/v190/final-plan/gnu-r7-src"


def authenticate():
    """Reopen the signed tree's complete archive and verify every file consumed by Cargo."""
    archive = ROOT.with_name("gnu-r7-source.tar")
    if sha(archive) != ARCHIVE or ROOT.resolve(strict=True) != ROOT:
        raise ValueError("joint source archive or staged root changed")
    seen = set()
    with tarfile.open(archive,"r:") as tar:
        for member in tar:
            if tar.pax_headers.get("comment") != COMMIT:
                raise ValueError("joint archive Git commit changed")
            path = ROOT / member.name
            if member.isdir():
                continue
            if not member.isfile() or path.is_symlink() or path.stat().st_uid != os.getuid():
                raise ValueError("unexpected joint source kind or owner")
            payload = tar.extractfile(member)
            if path.stat().st_size != member.size or sha(path) != hashlib.file_digest(payload,"sha256").hexdigest():
                raise ValueError("changed joint source: " + member.name)
            seen.add(member.name)
    actual = {p.relative_to(ROOT).as_posix() for p in ROOT.rglob("*") if p.is_file() or p.is_symlink()}
    if actual != seen:
        raise ValueError("unexpected source files in immutable GNU joint snapshot")
    return {"commit":COMMIT,"archive_sha256":ARCHIVE,"files":len(seen)}


def native():
    assembly = document(CAMPAIGN/"linux-cuda-native-scientific-assembly-r1.json",ASSEMBLY)
    if (not assembly["observation"]["fits_empty"] or assembly["observation"]["approved"]
            or sha(Path(assembly["manifest"])) != assembly["manifest_sha256"]):
        raise ValueError("original scientific ELF descriptor changed")
    verifier = runpy.run_path(str(ROOT/"scripts/cuda-linux-package.py"))
    handoff = verifier["verify_handoff"](CAMPAIGN/"linux-cuda-ort-final-core-r1.json",CORE,ROOT)
    return {**handoff["environment"],"AIRK_APPROVED_LINUX_CUDA_MANIFEST_SHA256":assembly["manifest_sha256"],
            "AIRK_STATIC_LINUX_CUDA_BUILD_SHA256":CORE}
