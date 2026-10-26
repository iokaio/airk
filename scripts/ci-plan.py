# SPDX-License-Identifier: Apache-2.0
"""Classify complete PR diffs; unknown paths and non-PR builds fail closed to full CI."""
import argparse
import importlib.util
import json
import os
from pathlib import Path, PurePosixPath
import subprocess


def repair_scope():
    specification = importlib.util.spec_from_file_location("release_repair_scope", Path(__file__).with_name("release-1.4.4-repair.py"))
    repair = importlib.util.module_from_spec(specification)
    specification.loader.exec_module(repair)
    return repair.scope()


def pinned_repair_plan(event):
    # Only this 1.4.4 tooling repair may reuse its already-qualified binaries.
    # The complete cumulative diff, version and exact runtime inputs must match;
    # a later code/version change restores the ordinary full producer plan.
    if event not in ("pull_request", "push"):
        return None
    try:
        repair_scope()
    except (ValueError, FileNotFoundError, subprocess.CalledProcessError):
        return None
    return dict(native=False, container=False, extension=False, contracts=True)


def classify(paths, event):
    plan = dict(native=False, container=False, extension=False, contracts=False)
    if event != "pull_request" or not paths:
        return dict.fromkeys(plan, True)
    for path in paths:
        if path.startswith("packages/vscode/"):
            plan["extension"] = True
            # TS/manifest changes require the real native adapter on every OS.
            if not (path.endswith(".md") or path.startswith("packages/vscode/media/")):
                plan["native"] = True
        elif path.startswith("scripts/"):
            plan["contracts"] = True
            # Packaging and smoke tests operate on compiled outputs. Changes to
            # them need actual payload verification, not only mocked contracts.
            if not path.startswith("scripts/tests/"):
                return dict.fromkeys(plan, True)
        elif path.startswith("docs/images/"):
            plan["extension"] = True
            plan["contracts"] = True
        elif (path.startswith("docs/") and path.endswith(".md")) or ("/" not in path and path.endswith(".md")):
            pass
        else:
            return dict.fromkeys(plan, True)
    return plan


def require_results(plan, results):
    expected = {"plan": "success", "quick": "success"}
    expected.update({name: "success" if needed else "skipped" for name, needed in plan.items()})
    if results != expected:
        raise ValueError(f"Required check results differ: expected {expected}, received {results}")


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("command", choices=("plan", "gate"))
    args = parser.parse_args()
    if args.command == "gate":
        needs = json.loads(os.environ["AIRK_NEEDS"])
        plan = json.loads(needs["plan"]["outputs"]["plan"])
        if set(plan) != {"native", "container", "extension", "contracts"} or any(type(v) is not bool for v in plan.values()):
            raise ValueError("Invalid CI plan")
        require_results(plan, {name: value["result"] for name, value in needs.items()})
        return
    event = os.environ.get("GITHUB_EVENT_NAME", "workflow_dispatch")
    paths = []
    if event == "pull_request":
        payload = json.loads(Path(os.environ["GITHUB_EVENT_PATH"]).read_text(encoding="utf-8"))
        base = payload["pull_request"]["base"]["sha"]
        head = payload["pull_request"]["head"]["sha"]
        # Complete history is checked out by this job. Disabling rename detection
        # makes both sides visible, including moves from code into documentation.
        result = subprocess.check_output(["git", "diff", "--name-only", "--no-renames", "-z", f"{base}...{head}"])
        paths = [p for p in result.decode("utf-8", errors="strict").split("\0") if p]
        if any(PurePosixPath(p).is_absolute() or ".." in PurePosixPath(p).parts for p in paths):
            raise ValueError("Invalid diff path")
    plan = pinned_repair_plan(event) or classify(paths, event)
    value = json.dumps(plan, separators=(",", ":"))
    print(value)
    with open(os.environ["GITHUB_OUTPUT"], "a", encoding="utf-8") as output:
        output.write(f"plan={value}\n")
        for name, required in plan.items():
            output.write(f"{name}={str(required).lower()}\n")


if __name__ == "__main__":
    main()
