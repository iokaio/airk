# SPDX-License-Identifier: Apache-2.0
"""Reproduce the inverse-replay proof of the L4 CPU Focus row movement.

Read-only against the repository: it reads Git objects and the live
runtime.rs, works on copies in a fresh temporary directory, and prints one
line per check. Exit 0 only when every hash matches the recorded value.

Chain proved (LF-normalized SHA-256 of runtime.rs):
  live c9008eef  --reverse l4-runtime-hooks.patch-->        9ede486c
  881787f0 4d420821 --reverse routing (f3978a12..179bb1da)--> 9ede486c
  9ede486c --reverse L5 (179bb1da..881787f0)-->              7b465076 (f3978a12)
"""
from __future__ import annotations

import argparse
import hashlib
import pathlib
import subprocess
import sys
import tempfile

RUNTIME = "crates/airk-retrieval/src/arbiter/runtime.rs"
EXPECTED = {
    "reviewed f3978a12": "7b4650767610eafdfea4b6c273e042008e4eae4654dde516d438c6ee2f8449b3",
    "routing 179bb1da": "258d43714a14169d8ca0dd5993890bdcaa26204c13a8e583f83bb6e63bb698e0",
    "L5 881787f0": "4d420821574d4d032175433a938f4d54bd74c7d9bb3834e599b16d68d486969f",
    "intermediate": "9ede486c03ef639d365b6d46e2a43d075408591abd327f589e34b91ff41b993d",
    "live": "c9008eef62a5af16791e3325b44ef0ada7e789c7b6bf5d50c4dc83ee32c05603",
}
HOOKS = pathlib.Path(__file__).with_name("l4-runtime-hooks.patch")


def lf_sha(data: bytes) -> str:
    assert b"\r" not in data.replace(b"\r\n", b""), "lone CR"
    return hashlib.sha256(data.replace(b"\r\n", b"\n")).hexdigest()


def git(repo: pathlib.Path, *args: str) -> bytes:
    return subprocess.run(
        ["git", "-C", str(repo), *args], check=True, capture_output=True
    ).stdout


def reverse(work: pathlib.Path, patch: bytes) -> bytes:
    """Reverse-apply `patch` to the runtime.rs copy under `work`."""
    (work / "change.patch").write_bytes(patch.replace(b"\r\n", b"\n"))
    subprocess.run(
        ["git", "apply", "-R", "--unidiff-zero", "change.patch"],
        cwd=work,
        check=True,
        capture_output=True,
    )
    return (work / RUNTIME).read_bytes()


def main() -> int:
    parser = argparse.ArgumentParser(description=__doc__.splitlines()[0])
    parser.add_argument("--repo", type=pathlib.Path, default=pathlib.Path.cwd())
    repo = parser.parse_args().repo.resolve()
    failures = 0

    def check(name: str, data: bytes) -> None:
        nonlocal failures
        actual = lf_sha(data)
        ok = actual == EXPECTED[name]
        failures += not ok
        print(f"{'ok  ' if ok else 'FAIL'} {name:18} {actual}")

    for name, commit in [
        ("reviewed f3978a12", "f3978a12"),
        ("routing 179bb1da", "179bb1da"),
        ("L5 881787f0", "881787f0"),
    ]:
        check(name, git(repo, "show", f"{commit}:{RUNTIME}"))
    live = (repo / RUNTIME).read_bytes()
    check("live", live)
    routing = git(repo, "diff", "--no-color", "f3978a12", "179bb1da", "--", RUNTIME)
    l5 = git(repo, "diff", "--no-color", "179bb1da", "881787f0", "--", RUNTIME)
    with tempfile.TemporaryDirectory(prefix="airk-row-replay-") as temp:
        work = pathlib.Path(temp)
        (work / RUNTIME).parent.mkdir(parents=True)
        # 1. The live source minus the L4 hooks is the intermediate source.
        (work / RUNTIME).write_bytes(live.replace(b"\r\n", b"\n"))
        check("intermediate", reverse(work, HOOKS.read_bytes()))
        # 2. L5's source minus the routing is the same intermediate source.
        (work / RUNTIME).write_bytes(git(repo, "show", f"881787f0:{RUNTIME}"))
        check("intermediate", reverse(work, routing))
        # 3. The intermediate source minus L5 is the last reviewed row.
        check("reviewed f3978a12", reverse(work, l5))
    names = ("context_wide", "focus::experiment")
    routed = [name for name in names if name.encode() in live]
    failures += bool(routed)
    print(f"{'FAIL' if routed else 'ok  '} live routing       {routed or 'none'}")
    return 1 if failures else 0


if __name__ == "__main__":
    sys.exit(main())
