# SPDX-License-Identifier: Apache-2.0
"""Classify complete PR diffs; unknown paths and non-PR builds fail closed to full CI."""
import argparse
import importlib.util
import json
import os
from pathlib import Path, PurePosixPath
import subprocess
import tempfile

# One row per native host. PRs gate the first three; main push builds macOS
# Intel and promotes the PR's tested artifacts for the others when the merged
# tree is identical. Every other main/manual build covers all four.
NATIVE_ROWS = (
    dict(os="ubuntu-24.04", target="x86_64-unknown-linux-gnu", executable="airk", cargo_jobs=4, test_threads=4),
    dict(os="windows-2025", target="x86_64-pc-windows-msvc", executable="airk.exe", cargo_jobs=4, test_threads=4),
    dict(os="macos-15", target="aarch64-apple-darwin", executable="AIRK.app/Contents/MacOS/airk",
         cargo_jobs=2, test_threads=2),
    dict(os="macos-15-intel", target="x86_64-apple-darwin", executable="AIRK.app/Contents/MacOS/airk",
         cargo_jobs=4, test_threads=4, timeout_minutes=120),
)
PR_TARGETS = tuple(row["target"] for row in NATIVE_ROWS[:3])
MAIN_TARGETS = tuple(row["target"] for row in NATIVE_ROWS[3:])
# Dependency caches are written only by main push. A change to their key inputs,
# or to CI itself, therefore rebuilds every target on main instead of promoting.
FULL_MAIN_INPUTS = ("Cargo.lock", "Cargo.toml", "rust-toolchain.toml", "Dockerfile", "scripts/ci-prune-target.py",
                    ".github/workflows/ci.yml", ".config/nextest.toml")
FULL_MAIN_PREFIXES = (".cargo/",)


def candidates():
    specification = importlib.util.spec_from_file_location("ci_candidates", Path(__file__).with_name("ci-candidates.py"))
    module = importlib.util.module_from_spec(specification)
    specification.loader.exec_module(module)
    return module


def requires_full_main(paths):
    return not paths or any(path in FULL_MAIN_INPUTS or path.endswith("/Cargo.toml")
                            or path.startswith(FULL_MAIN_PREFIXES) for path in paths)


def try_promotion(commit, destination):
    """Any doubt, including API failure, leaves main to build every target."""
    try:
        if subprocess.check_output(["git", "rev-parse", "HEAD"], text=True).strip() != commit:
            raise ValueError("Promotion requires the exact main checkout")
        tree = subprocess.check_output(["git", "rev-parse", "HEAD^{tree}"], text=True).strip()
        with tempfile.TemporaryDirectory(prefix="airk-promote-") as temporary:
            record = candidates().promote(commit, tree, Path(temporary))
    except Exception as error:  # The fallback is the complete four-target build.
        print(f"::notice title=Complete main build::{type(error).__name__}: {error}")
        return False
    destination.mkdir(parents=True, exist_ok=False)
    (destination / "promotion.json").write_text(json.dumps(record, indent=2) + "\n", encoding="utf-8")
    print(f"Promoted PR #{record['pull_request']} CI {record['producer']['run_id']} for main tree {tree}")
    return True


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


def native_targets(event, promoted, qualification=False):
    """PR gates exclude only macOS Intel; main builds it alone only after promotion."""
    if qualification or event not in ("pull_request", "push"):
        return tuple(row["target"] for row in NATIVE_ROWS)
    if event == "pull_request":
        return PR_TARGETS
    return MAIN_TARGETS if promoted else tuple(row["target"] for row in NATIVE_ROWS)


def promoted_plan():
    # The promoted PR run already passed container, extension and contract gates
    # on this exact tree; main repeats only the macOS Intel native candidate.
    return dict(native=True, container=False, extension=False, contracts=False)


def native_matrix(targets):
    rows = [row for row in NATIVE_ROWS if row["target"] in targets]
    if not rows or len(rows) != len(set(targets)):
        raise ValueError("Native matrix lacks a requested target")
    return {"include": rows}


def require_results(plan, results):
    expected = {"plan": "success", "quick": "success"}
    expected.update({name: "success" if needed else "skipped" for name, needed in plan.items()})
    if results != expected:
        raise ValueError(f"Required check results differ: expected {expected}, received {results}")


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("command", choices=("plan", "gate"))
    parser.add_argument("--promotion", type=Path, help="Verified promotion record written by ci-candidates.py promote")
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
    if event in ("pull_request", "push"):
        payload = json.loads(Path(os.environ["GITHUB_EVENT_PATH"]).read_text(encoding="utf-8"))
        if event == "pull_request":
            span = f'{payload["pull_request"]["base"]["sha"]}...{payload["pull_request"]["head"]["sha"]}'
        else:
            before, after = str(payload.get("before", "")), str(payload.get("after", ""))
            span = f"{before}..{after}" if before.strip("0") and after else None
        # Complete history is checked out by this job. Disabling rename detection
        # makes both sides visible, including moves from code into documentation.
        try:
            result = subprocess.check_output(["git", "diff", "--name-only", "--no-renames", "-z", span]) if span else b""
        except subprocess.CalledProcessError:
            if event == "pull_request":
                raise
            result = b""  # An unknown push base selects the complete main build.
        paths = [p for p in result.decode("utf-8", errors="strict").split("\0") if p]
        if any(PurePosixPath(p).is_absolute() or ".." in PurePosixPath(p).parts for p in paths):
            raise ValueError("Invalid diff path")
    promoted = (event == "push" and os.environ.get("GITHUB_REF") == "refs/heads/main" and args.promotion is not None
                and not requires_full_main(paths) and try_promotion(os.environ["GITHUB_SHA"], args.promotion))
    # Main and manual builds never narrow their checks by changed paths.
    paths = paths if event == "pull_request" else []
    qualification = os.environ.get("AIRK_QUALIFICATION") == "true"
    plan = pinned_repair_plan(event) or (promoted_plan() if promoted else classify(paths, event))
    value = json.dumps(plan, separators=(",", ":"))
    matrix = json.dumps(native_matrix(native_targets(event, promoted, qualification)), separators=(",", ":"))
    print(value)
    print(matrix)
    with open(os.environ["GITHUB_OUTPUT"], "a", encoding="utf-8") as output:
        output.write(f"plan={value}\nnative_matrix={matrix}\npromoted={str(promoted).lower()}\n")
        for name, required in plan.items():
            output.write(f"{name}={str(required).lower()}\n")


if __name__ == "__main__":
    main()
