# SPDX-License-Identifier: Apache-2.0
"""Guarded Windows command stage for the fresh CUDA build."""

import json
from pathlib import Path
import re
import subprocess
import time


def require(condition, message):
    if not condition:
        raise ValueError(message)


def run_stage(*, argv, cwd, root, stage, environment, cpu, guard_paths,
              power_shell, tree_waiter, deadline):
    """Run one command and retain authentic parent and descendant settlement."""
    require(re.fullmatch(r"[a-z][a-z0-9-]*", stage), "invalid build stage name")
    require(root.is_absolute() and cwd.is_absolute() and cwd.is_relative_to(root),
            "command working directory must be within the fresh build")
    require(argv and Path(argv[0]).is_absolute(), "absolute command image required")
    for executable in (power_shell, tree_waiter):
        cpu.verify_file(cpu.plain(executable["path"], regular=True), executable)

    logs = root / "logs"
    prefix = logs / stage
    paths = {name: prefix.with_suffix(suffix) for name, suffix in {
        "stdout": ".stdout.log", "stderr": ".stderr.log", "result": ".tree.json",
        "started": ".started.json", "request": ".request.json", "outer": ".outer.log",
        "receipt": ".receipt.json",
    }.items()}
    guard_paths(list(paths.values()), False, deadline)
    require(not any(path.exists() for path in paths.values()), "stage outputs already exist")
    request = {
        "schema": "airk.private.process-tree-command/1",
        "executable": argv[0],
        "arguments": subprocess.list2cmdline(argv[1:]),
        "cwd": str(cwd),
        **{key: str(paths[key]) for key in ("stdout", "stderr", "result", "started")},
        "started_event": None,
        "done_event": None,
    }
    with paths["request"].open("x", encoding="utf-8", newline="\n") as stream:
        json.dump(request, stream, indent=2)
        stream.write("\n")
    guard_paths([paths["request"]], True, deadline)
    wrapped = [power_shell["path"], "-NoLogo", "-NoProfile", "-NonInteractive",
               "-File", tree_waiter["path"], "-Request", str(paths["request"])]
    remaining = int(deadline - time.monotonic())
    require(remaining > 0, "original fresh-build deadline reached before dispatch")
    result = cpu.run_owned(wrapped, cwd, paths["outer"], remaining, environment)
    created = [path for name, path in paths.items()
               if name not in ("receipt", "request") and path.exists()]
    if created:
        guard_paths(created, True, deadline)
    tree = cpu.read_json(paths["result"]) if paths["result"] in created else None
    proof = {
        "stage": stage, "argv": argv, "cwd": str(cwd),
        "original_deadline_monotonic": deadline,
        "owned": result, "actual_tree_result": tree,
        "files": [{"path": str(path), "bytes": path.stat().st_size,
                   "sha256": cpu.digest(path)} for path in (paths["request"], *created)],
    }
    guard_paths([paths["receipt"]], False, deadline)
    with paths["receipt"].open("x", encoding="utf-8", newline="\n") as stream:
        json.dump(proof, stream, indent=2, allow_nan=False)
        stream.write("\n")
    guard_paths([paths["receipt"]], True, deadline)
    require(time.monotonic() < deadline, "fresh-build stage proof exceeded original deadline")
    require(cpu.successful(result) and isinstance(tree, dict)
            and tree.get("schema") == "airk.private.process-tree-result/1"
            and type(tree.get("exit_code")) is int and tree["exit_code"] == 0
            and tree.get("wait") == "Start-Process-Wait-process-and-descendants",
            "fresh-build stage did not naturally settle: " + stage)
    return proof
