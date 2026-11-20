# SPDX-License-Identifier: Apache-2.0
"""Freeze the maintained Linux shipping inputs in the existing native checkout with the normal source identity."""
import argparse
import hashlib
import json
import os
from pathlib import Path
import runpy
import shutil
import stat
import subprocess
import sys

from configure_ort import BASE, REPO, audit, sha
from unpack import directory, guarded
from verified_inputs import identity


def owner(root):
    sys.path.insert(0, str(root / "scripts"))
    try:
        return runpy.run_path(str(root / "scripts/build-service-runtime.py"))
    finally:
        sys.path.pop(0)


def names(root, paths):
    raw = subprocess.check_output(["/usr/bin/git", "--no-optional-locks", "-C", str(root),
                                   "ls-files", "--cached", "--others", "--exclude-standard", "-z", "--", *paths], timeout=60)
    return {Path(os.fsdecode(k)) for k in raw.split(bytes([0])) if k}


def stage(args):
    uid = identity()
    os.umask(0o077)
    if sha(args.base) != args.base_sha256:
        raise ValueError("native checkout receipt changed")
    base = json.loads(args.base.read_bytes())
    root = Path(base["source"])
    if root != BASE/"linux-cuda-service-source-r1/source":
        raise ValueError("unexpected native shipping checkout")
    guarded(root, uid, stat.S_ISDIR)
    head = subprocess.check_output(["/usr/bin/git","-C",str(root),"rev-parse","HEAD"],timeout=10).decode().strip()
    if head != base["commit"]:
        raise ValueError("native source base changed")
    build = owner(REPO)
    before = build["source_identity"]()
    paths = {"Cargo.toml","Cargo.lock","rust-toolchain.toml",".cargo",".config","crates","scripts","fixtures",
             "third_party","LICENSE","NOTICE","THIRD_PARTY_NOTICES.md",*build["FRONTEND_INPUTS"]}
    roster = names(REPO,paths) | names(root,paths)
    # Include every fixture reached through the Rust owner's literal-data inventory.
    literal = build["compiled_literal_inputs"]([os.fsencode(path) for path in roster])
    roster.update(Path(os.fsdecode(path)) for path in literal)
    if len(roster) > 20000:
        raise ValueError("unexpected native source inventory size")
    changed = {}
    for name in sorted(roster):
        if name.is_absolute() or ".." in name.parts or name.parts[0] in ("artifacts",".airk","temp",".git"):
            raise ValueError("source roster escapes checkout")
        src, dst = REPO/name, root/name
        if src.is_symlink() or dst.is_symlink():
            raise ValueError("unexpected symlink in native sources: " + str(name))
        if not src.exists():
            if dst.exists():
                raise ValueError("retained native file missing from maintained sources: " + str(name))
            continue
        meta = src.stat()
        if meta.st_uid != uid or not stat.S_ISREG(meta.st_mode) or meta.st_size > 128 << 20:
            raise ValueError("invalid source owner, type or size: " + str(name))
        digest = sha(src)
        if dst.exists():
            guarded(dst,uid,stat.S_ISREG)
            if sha(dst) == digest:
                continue
        directory(dst.parent,root,uid)
        guarded(dst.parent,uid,stat.S_ISDIR)
        with src.open("rb") as original, dst.open("wb") as out:
            shutil.copyfileobj(original,out)
        if sha(dst) != digest or sha(src) != digest:
            raise ValueError("source changed during projection: " + str(name))
        changed[name.as_posix()] = digest
    after = owner(REPO)["source_identity"]()
    native = owner(root)["source_identity"]()
    if not before == after == native:
        raise ValueError(f"native shipping source mismatch: before={before}, after={after}, native={native}")
    audit(root,uid)
    return {"source":str(root),"base":head,"base_receipt":args.base_sha256,"identity":native,
            "examined":len(roster),"overlaid":changed,"uid":uid}


if __name__ == "__main__":
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("--base",type=Path,required=True)
    parser.add_argument("--base-sha256",required=True)
    print(json.dumps(stage(parser.parse_args()),sort_keys=True))
