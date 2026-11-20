# SPDX-License-Identifier: Apache-2.0
"""List the exact existing Cargo registry archives/index and rusqlite Git object DB for the Windows offline home."""
import argparse
import hashlib
import json
from pathlib import Path
import re
import subprocess
import tomllib

ROOT = Path(__file__).resolve().parents[3]
SOURCE = Path("C:/Users/tdotn/.cargo")
INDEX = "index.crates.io-1949cf8c6b5b557f"
REV = "2a71e35d94b2a02f7dd4a0c4cfd59c339370e747"
DB = Path("git/db/rusqlite-8e5e8204e489f2fd")
GIT = Path("C:/Program Files/Git/cmd/git.exe")


def digest(path):
    with path.open("rb") as stream:
        return hashlib.file_digest(stream, "sha256").hexdigest()


def sparse(name):
    name = name.lower()
    if len(name) <= 2:
        return str(len(name)) + "/" + name
    if len(name) == 3:
        return "3/" + name[0] + "/" + name
    return name[:2] + "/" + name[2:4] + "/" + name


def plan():
    lock = ROOT / "Cargo.lock"
    rows = tomllib.loads(lock.read_text("utf-8"))["package"]
    files = {}
    def add(relative, expected=None):
        key = Path(relative).as_posix()
        if key in files:
            return
        file = SOURCE / key
        if file.resolve(strict=True) != file or not file.is_file():
            raise ValueError("Expected original ordinary Cargo input: " + key)
        sha = digest(file)
        if expected is not None and sha != expected:
            raise ValueError("Cargo.lock archive digest mismatch: " + key)
        files[key] = {"bytes": file.stat().st_size, "sha256": sha}
    registry = 0
    repositories = set()
    for row in rows:
        source = row.get("source")
        if source is None:
            continue
        if source.startswith("git+"):
            repositories.add(source)
            continue
        if source != "registry+https://github.com/rust-lang/crates.io-index":
            raise ValueError("Unrecognized locked registry")
        name, version = row["name"], row["version"]
        if not re.fullmatch(r"[a-zA-Z0-9_-]+",name) or not re.fullmatch(r"[0-9A-Za-z.+-]+",version):
            raise ValueError("Invalid package path")
        add(f"registry/cache/{INDEX}/{name}-{version}.crate",row["checksum"])
        add(f"registry/index/{INDEX}/.cache/{sparse(name)}")
        registry += 1
    if repositories != {f"git+https://github.com/rusqlite/rusqlite?rev={REV}#{REV}"}:
        raise ValueError("Git dependencies differ from original pinned commit")
    add(f"registry/index/{INDEX}/config.json")
    db = SOURCE / DB
    actual = subprocess.check_output([GIT,"--git-dir",db,"rev-parse","refs/commit/"+REV+"^{commit}"], text=True).strip()
    if actual != REV or (db/"objects/info/alternates").exists():
        raise ValueError("Git cache does not own the locked commit")
    for name in ["config","HEAD","refs/commit/"+REV]:
        add(DB/name)
    for file in sorted((db/"objects").rglob("*")):
        if file.is_file():
            add(file.relative_to(SOURCE))
    return {"lock_sha256":digest(lock),"packages":registry,"git_commit":REV,"files":files}


if __name__ == "__main__":
    argparse.ArgumentParser(description=__doc__).parse_args()
    print(json.dumps(plan(),sort_keys=True))
