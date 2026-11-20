# SPDX-License-Identifier: Apache-2.0
"""Extract only hash-pinned redistribution packages beneath the verified WSL user's build directory."""
import argparse
from collections import Counter
import hashlib
import json
import os
from pathlib import Path, PurePosixPath
import pwd
import shutil
import stat
import tarfile
import time


def guarded(path, uid, kind):
    info = path.lstat()
    if info.st_uid != uid or not kind(info.st_mode):
        raise ValueError(f"unexpected owner or kind: {path}")
    if not stat.S_ISLNK(info.st_mode):
        if info.st_mode & 0o022 or any(key.startswith("system.posix_acl_") for key in os.listxattr(path)):
            raise ValueError(f"unexpected write or inheritance policy: {path}")


def directory(path, boundary, uid):
    if path == boundary:
        guarded(path, uid, stat.S_ISDIR)
        return
    path.relative_to(boundary)
    directory(path.parent, boundary, uid)
    if not path.exists():
        path.mkdir(0o700)
    guarded(path, uid, stat.S_ISDIR)


def member_destination(root, component, archive_root, name):
    parts = PurePosixPath(name).parts
    if not parts or parts[0] != archive_root or any(p in (".", "..") for p in parts):
        raise ValueError("archive path outside package")
    if len(parts) == 1:
        return None
    relative = parts[1:]
    tree = "cudnn" if component == "cudnn" else "cuda"
    # Vendor notices and release-level metadata remain separate per package.
    if len(relative) == 1 and ("LICENSE" in relative[0].upper() or relative[0].lower().endswith((".txt", ".json"))):
        return root / "notices" / component / relative[0]
    return root.joinpath(tree, *relative)


def unpack_one(src, root, component, filename, uid, counts, links):
    stem = filename.removesuffix(".tar.xz")
    seen = set()
    expanded = 0
    with tarfile.open(fileobj=src, mode="r|xz") as archive:
        for item in archive:
            path = member_destination(root, component, stem, item.name)
            if path is None:
                if not item.isdir():
                    raise ValueError("root must be a directory")
                continue
            if path in seen or len(seen) >= 100_000 or item.size < 0:
                raise ValueError("duplicate member or invalid package extent")
            seen.add(path)
            expanded += item.size
            if expanded > 10 << 30:
                raise ValueError("package expansion limit exceeded")
            directory(path.parent, root, uid)
            if item.isdir():
                directory(path, root, uid)
            elif item.isfile():
                if path.exists():
                    guarded(path, uid, stat.S_ISREG)
                    with path.open("rb") as existing, archive.extractfile(item) as other:
                        if hashlib.file_digest(existing, "sha256").digest() != hashlib.file_digest(other, "sha256").digest():
                            raise ValueError(f"distinct source packages collide at {path}")
                else:
                    mode = 0o700 if item.mode & 0o111 else 0o600
                    fd = os.open(path, os.O_CREAT | os.O_EXCL | os.O_WRONLY | os.O_NOFOLLOW, mode)
                    with os.fdopen(fd, "wb") as out, archive.extractfile(item) as other:
                        shutil.copyfileobj(other, out, 1 << 20)
                    guarded(path, uid, stat.S_ISREG)
                    if path.stat().st_size != item.size:
                        raise ValueError("incomplete extracted member")
                counts["files"] += 1
            elif item.issym():
                target = PurePosixPath(item.linkname)
                if target.is_absolute() or not (path.parent / target).resolve().is_relative_to(root):
                    raise ValueError("symlink target escapes build root")
                if os.path.lexists(path):
                    guarded(path, uid, stat.S_ISLNK)
                    if os.readlink(path) != item.linkname:
                        raise ValueError("distinct package symlink collision")
                else:
                    path.symlink_to(item.linkname)
                guarded(path, uid, stat.S_ISLNK)
                links.add(path)
                counts["symlinks"] += 1
            else:
                raise ValueError(f"unsupported archive member type: {item.name}")
    return expanded


def run(args):
    uid = os.getuid()
    if uid != args.uid or uid != os.geteuid() or uid == 0 or pwd.getpwuid(uid).pw_name != args.user:
        raise ValueError("registered human account required")
    os.umask(0o077)
    home = Path(pwd.getpwuid(uid).pw_dir)
    parent = args.output.parent
    if (not args.output.is_absolute() or not parent.is_relative_to(home) or
            parent.resolve(strict=True) != parent or os.path.lexists(args.output)):
        raise ValueError("fresh directory below the verified human home required")
    for ancestor in [parent, *parent.parents]:
        guarded(ancestor, uid, stat.S_ISDIR)
        if ancestor == home:
            break
    receipt = args.acquisition.read_bytes()
    if hashlib.sha256(receipt).hexdigest() != args.sha256:
        raise ValueError("acquisition receipt differs")
    sealed = json.loads(receipt)
    rows = sealed["archives"]
    if not sealed["staged"] or len(rows) != 12 or len({r["name"] for r in rows}) != len(rows):
        raise ValueError("unexpected redistribution acquisition")
    args.output.mkdir(0o700)
    guarded(args.output, uid, stat.S_ISDIR)
    links = set()
    counts = Counter()
    packages = []
    started = time.monotonic()
    for row in rows:
        name = row["name"]
        if PurePosixPath(name).name != name or not name.endswith("-archive.tar.xz"):
            raise ValueError("invalid sealed archive name")
        path = args.acquisition.parent / name
        with path.open("rb") as stream:
            if os.fstat(stream.fileno()).st_size != row["bytes"] or hashlib.file_digest(stream, "sha256").hexdigest() != row["sha256"]:
                raise ValueError("input archive changed")
            stream.seek(0)
            expanded = unpack_one(stream, args.output, name.split("-linux-x86_64-")[0], name, uid, counts, links)
        packages.append({"name": name, "expanded_bytes": expanded, "sha256": row["sha256"]})
    for link in links:
        target = link.resolve(strict=True)
        if not target.is_relative_to(args.output):
            raise ValueError("unresolved or escaping final symlink")
        guarded(target, uid, lambda mode: stat.S_ISREG(mode) or stat.S_ISDIR(mode))
    return {"schema": "airk.private.linux-cuda-build-inputs/1", "root": str(args.output),
            "counts": counts, "packages": packages, "acquisition_sha256": args.sha256,
            "elapsed_seconds": time.monotonic()-started}


if __name__ == "__main__":
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("--acquisition", type=Path, required=True)
    parser.add_argument("--sha256", required=True)
    parser.add_argument("--output", type=Path, required=True)
    parser.add_argument("--uid", type=int, required=True)
    parser.add_argument("--user", required=True)
    print(json.dumps(run(parser.parse_args()), sort_keys=True))
