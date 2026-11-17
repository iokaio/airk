# SPDX-License-Identifier: Apache-2.0
"""Read-only admission for private recipe preparation; launcher owns identity/ACL guards."""
import os
from pathlib import Path
import subprocess


def private_run_root(repo):
    value = os.environ.get("AIRK_RECIPE_RUN_ROOT")
    if not value:
        raise ValueError("AIRK_RECIPE_RUN_ROOT must explicitly name an existing ignored private directory")
    supplied = Path(value)
    if not supplied.is_absolute():
        raise ValueError("AIRK_RECIPE_RUN_ROOT must be absolute")
    path = supplied.resolve(strict=True)
    repo = Path(repo).resolve(strict=True)
    if not path.is_dir() or path == repo / "artifacts" or not path.is_relative_to(repo / "artifacts"):
        raise ValueError("AIRK_RECIPE_RUN_ROOT must be an ignored directory below repository artifacts")
    relative = path.relative_to(repo).as_posix()
    ignored = subprocess.run(["git", "-C", str(repo), "check-ignore", "-q", "--", relative], check=False)
    tracked = subprocess.run(["git", "-C", str(repo), "ls-files", "-z", "--", relative], capture_output=True, check=True)
    if ignored.returncode != 0 or tracked.stdout:
        raise ValueError("AIRK_RECIPE_RUN_ROOT must be ignored and contain no tracked files")
    return path


def fresh_output(run_root, path):
    path = Path(path).resolve()
    if path == run_root or not path.is_relative_to(run_root) or not path.parent.is_dir() or path.exists():
        raise ValueError("fresh output inside AIRK_RECIPE_RUN_ROOT required")
    return path
