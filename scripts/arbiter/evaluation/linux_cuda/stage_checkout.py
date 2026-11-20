# SPDX-License-Identifier: Apache-2.0
"""Freeze a local campaign commit into the verified Linux human's fresh Cargo source."""
import argparse
import json
import os
from pathlib import Path
import pwd
import runpy
import stat
import subprocess

from configure_ort import BASE, CPU, CPU_SHA, REPO, audit, sha
from unpack import guarded


def stage(commit, destination):
    if (os.getuid(), os.geteuid(), os.getgid(), pwd.getpwuid(os.getuid()).pw_name) != (1000, 1000, 1000, "tyler"):
        raise ValueError("expected the registered Linux user")
    if destination.parent != BASE or os.path.lexists(destination) or sha(CPU) != CPU_SHA:
        raise ValueError("fresh guest staging directory and pinned process owner required")
    if len(commit) != 40 or any(c not in "0123456789abcdef" for c in commit):
        raise ValueError("exact commit required")
    os.umask(0o077)
    for parent in (BASE.parents[1], BASE.parent, BASE):
        guarded(parent, os.getuid(), stat.S_ISDIR)
    destination.mkdir(mode=0o700)
    repo = destination / "source"
    env = {"PATH": "/usr/bin:/bin", "HOME": str(BASE.parents[1]), "LC_ALL": "C",
           "GIT_TERMINAL_PROMPT": "0", "GIT_OPTIONAL_LOCKS": "0"}
    cpu = runpy.run_path(str(CPU))
    logs = []
    for step, command in [
        ("init", ["init", "--quiet", "--template=", str(repo)]),
        ("fetch", ["-C", str(repo), "fetch", "--depth=1", "--no-tags", "--no-recurse-submodules",
                   "--no-auto-maintenance", str(REPO), commit]),
        ("checkout", ["-C", str(repo), "checkout", "--quiet", "--detach", commit]),
    ]:
        proc = cpu["run_owned"](["/usr/bin/git", *command], destination, destination / (step + ".log"), 180, env)
        logs.append(proc)
        if not cpu["successful"](proc):
            raise ValueError("local source staging failed: " + step)
    def git(*args):
        return subprocess.run(["/usr/bin/git", "-C", str(repo), *args], env=env, timeout=20,
                              check=True, capture_output=True).stdout
    if git("rev-parse", "HEAD").decode().strip() != commit or git("status", "--porcelain"):
        raise ValueError("frozen Linux source identity differs")
    audit(destination, os.getuid())
    return {"commit": commit, "tree": git("rev-parse", "HEAD^{tree}").decode().strip(),
            "files": len(git("ls-files", "-z").split(b"\0")) - 1, "source": str(repo),
            "commands": logs}


if __name__ == "__main__":
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("--commit", required=True)
    parser.add_argument("--output", type=Path, required=True)
    args = parser.parse_args()
    print(json.dumps(stage(args.commit, args.output), sort_keys=True))
