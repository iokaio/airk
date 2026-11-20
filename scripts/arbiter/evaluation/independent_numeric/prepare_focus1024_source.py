# SPDX-License-Identifier: Apache-2.0
"""Bind the original Focus source manifest to one new retained Windows native build."""
import argparse
import hashlib
import json
from pathlib import Path
import subprocess

from maintained_helpers import private_root, worker

ROOT = Path(__file__).resolve().parents[4]
ORIGINAL_SHA = "c3ebe6fa670545ea134a54146ab0ce4c502f412f687d751386ea1700db92312e"
ORIGINAL = ROOT / "artifacts/advanced-split/models/actual-composed-service-r2/final-plan-current-policy-preparation-20261116T085632Z/metadata-request.private.json"
QUALIFICATION = "crates/airk-inference/src/inference_engine/pool/arbiter/qualification/"
ADDITIONAL = (QUALIFICATION + "focus.rs", QUALIFICATION + "software_v2.rs",
              QUALIFICATION + "software_v2/original_sources.rs",
              "crates/airk-retrieval/src/arbiter/runtime/qualification/focus_static1024.rs")


def pinned(path, sha):
    worker.regular(path)
    if worker.digest(path) != sha:
        raise ValueError(f"retained source differs: {path}")
    return {"path": str(path), "sha256": sha}


def prepare(build_path, build_sha, output):
    private_root(output.parent)
    if not output.is_absolute() or output.exists():
        raise ValueError("fresh absolute private destination required")
    actual = pinned(build_path, build_sha)
    build = json.loads(build_path.read_bytes())
    command = build["command"]
    if not (build["passed"] and build["inputs_unchanged"] and build["exit_code"] == 0 and
            "test" in command and "--no-run" in command and "airk-inference" in command and
            "--message-format=json" in command):
        raise ValueError("successful sealed native inference image build required")
    pinned(ORIGINAL, ORIGINAL_SHA)
    manifest = json.loads(ORIGINAL.read_bytes())
    if Path(manifest["repository_root"]) != ROOT:
        raise ValueError("original native source root differs")
    names = [Path(row["path"]).relative_to(ROOT).as_posix() for row in manifest["source_files"]]
    if len(set(names)) != len(names):
        raise ValueError("duplicate original source member")
    names.extend(name for name in ADDITIONAL if name not in names)
    group = build["groups"]["native"]["files"]
    manifest["source_files"] = [pinned(ROOT / name, group[name]) for name in names]
    manifest["native_build_receipt"] = actual
    manifest["source_commit"] = subprocess.check_output(
        ["git", "-C", str(ROOT), "rev-parse", "HEAD"], text=True).strip()
    with output.open("x", encoding="utf8", newline="\n") as out:
        json.dump(manifest, out, indent=2)
        out.write("\n")
    print(hashlib.sha256(output.read_bytes()).hexdigest(), len(names), "native source files")


if __name__ == "__main__":
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("--build", required=True, type=Path)
    parser.add_argument("--build-sha256", required=True)
    parser.add_argument("--output", required=True, type=Path)
    args = parser.parse_args()
    prepare(args.build, args.build_sha256, args.output)
