# SPDX-License-Identifier: Apache-2.0
"""Verify a completed static ORT handoff and apply it only to one child process."""
import argparse
import importlib.util
import json
import os
from pathlib import Path
import subprocess
import sys

ROOT = Path(__file__).resolve().parents[1]
STATIC_KEYS = frozenset({"LIBONNXRUNTIME_NO_PKG_CONFIG", "ORT_SKIP_DOWNLOAD",
                         "ORT_PREFER_DYNAMIC_LINK", "ORT_LIB_LOCATION", "ORT_LIB_PROFILE"})


def provisioner_owner():
    specification = importlib.util.spec_from_file_location(
        "airk_static_environment_owner", ROOT / "scripts/provision-static-onnx-runtime.py")
    module = importlib.util.module_from_spec(specification)
    specification.loader.exec_module(module)
    return module


def destination_from_location(location, owner=None):
    """Locate the owning receipt, without treating a directory name as proof."""
    location = Path(location)
    if not location.is_absolute() or not location.name:
        raise ValueError("Static ORT handoff library location must be an absolute owned build directory")
    return (owner or provisioner_owner()).plain(location).parent


def load_environment_file(path, destination=None, target=None):
    """JSON is evidence, not authority: reverify its entire owning build result."""
    owner = provisioner_owner()
    report = owner.read_json(path)
    exported = report.get("environment") if isinstance(report, dict) else None
    if (not isinstance(exported, dict) or set(exported) != STATIC_KEYS
            or any(not isinstance(value, str) or any(ord(char) < 32 for char in value)
                   for value in exported.values())):
        raise ValueError("Static ORT handoff must contain exactly the five supported environment values")
    expected = {"LIBONNXRUNTIME_NO_PKG_CONFIG": "1", "ORT_SKIP_DOWNLOAD": "1",
                "ORT_PREFER_DYNAMIC_LINK": "0", "ORT_LIB_PROFILE": "Release"}
    if any(exported[name] != value for name, value in expected.items()):
        raise ValueError("Static ORT handoff does not select the pinned static release libraries")
    # Both the regular recipe and reviewed Neo adoption place their library
    # tree directly under its receipt owner (b or b2). Only the canonical
    # validator decides which exact tree that completed build actually owns.
    derived = destination_from_location(exported["ORT_LIB_LOCATION"], owner)
    selected = owner.plain(destination) if destination is not None else derived
    if selected != derived:
        raise ValueError("Static ORT handoff belongs to another destination")
    native = owner.host_target()
    if target is not None and target != native:
        raise ValueError("Static ORT handoff must match the native host target")
    verified = owner.verify_build(selected, native)
    if report != verified:
        raise ValueError("Static ORT handoff differs from the verified complete build result")
    return dict(verified["environment"])


def main(argv=None):
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("--environment-file", type=Path, required=True)
    parser.add_argument("--destination", type=Path)
    parser.add_argument("--target")
    parser.add_argument("command", nargs=argparse.REMAINDER)
    args = parser.parse_args(argv)
    environment = load_environment_file(args.environment_file, args.destination, args.target)
    command = args.command
    if command and command[0] == "--":
        command = command[1:]
    if not command:
        print(json.dumps(environment, sort_keys=True))
        return 0
    # Keep the invoking shell, global settings and unrelated environment intact.
    return subprocess.call(command, cwd=ROOT, env={**os.environ, **environment})


if __name__ == "__main__":
    try:
        raise SystemExit(main())
    except (OSError, ValueError, subprocess.SubprocessError) as error:
        print(f"Static ORT environment refused: {error}", file=sys.stderr)
        raise SystemExit(1)
