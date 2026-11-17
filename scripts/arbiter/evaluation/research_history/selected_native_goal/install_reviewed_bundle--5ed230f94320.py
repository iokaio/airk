# SPDX-License-Identifier: Apache-2.0
"""Private reviewed-source installer. Default verifies only; Root owns --apply."""
import argparse
import hashlib
import json
from pathlib import Path
import shutil

ALLOWED = {
    "crates/airk-service/src/service_runtime/arbiter/joint_scoring.rs",
    "crates/airk-service/src/service_runtime/arbiter/joint_scoring/tests.rs",
    "crates/airk-service/src/service_runtime/evaluation_fixture_tests.rs",
    "crates/airk-service/src/service_runtime/arbiter.rs",
    "crates/airk-service/src/service_runtime/arbiter/evaluation.rs",
    "crates/airk-service/src/service_runtime/arbiter/CONTRACT.md",
    "crates/airk-kernel/src/inference_owner/context_action_tests.rs",
    "crates/airk-kernel/src/inference_owner/context_action_calibration_tests.rs",
    "crates/airk-kernel/src/inference_owner/context_action_tests/selected.rs",
    "crates/airk-kernel/src/inference_owner/context_action_calibration_tests/selected.rs",
}

def digest(data):
    return hashlib.sha256(data).hexdigest()

def plain_path(root, relative):
    path = Path(relative)
    if path.is_absolute() or ".." in path.parts:
        raise ValueError("relative contained paths required")
    result = root / path
    for ancestor in (result, *result.parents):
        if ancestor == root:
            break
        if ancestor.is_symlink() or (ancestor.exists() and ancestor.is_junction()):
            raise ValueError("link/junction path refused")
    if not result.resolve().is_relative_to(root):
        raise ValueError("path escapes repository")
    return result

def prepare(root, manifest_path, expected):
    root = root.resolve(strict=True)
    raw = manifest_path.read_bytes()
    if digest(raw) != expected:
        raise ValueError("manifest SHA mismatch")
    manifest = json.loads(raw)
    rows = manifest["files"]
    if not rows or len(rows) > len(ALLOWED):
        raise ValueError("invalid reviewed source count")
    prepared, seen = [], set()
    for row in rows:
        name = row["target"].replace("\\", "/")
        if name not in ALLOWED or name in seen:
            raise ValueError("duplicate or unowned target")
        seen.add(name)
        target = plain_path(root, name)
        proposed = plain_path(root, row["proposed"])
        if not proposed.is_relative_to(root / "artifacts/advanced-split"):
            raise ValueError("proposal outside private preparation")
        content = proposed.read_bytes()
        if digest(content) != row["sha256"]:
            raise ValueError("proposed source drift: " + name)
        baseline = row["base_sha256"]
        if baseline is None:
            if target.exists():
                raise ValueError("new target already exists: " + name)
        elif not target.is_file() or digest(target.read_bytes()) != baseline:
            raise ValueError("live source drift: " + name)
        prepared.append((row, target, proposed, content))
    return prepared

def mount(root, manifest_path, expected, backup):
    rows = prepare(root, manifest_path, expected)
    backup = backup.resolve()
    if not backup.is_relative_to(root / "artifacts/advanced-split") or backup.exists():
        raise ValueError("fresh contained private backup required")
    backup.mkdir(parents=True)
    for row, target, _, _ in rows:
        if target.exists():
            saved = backup / row["target"]
            saved.parent.mkdir(parents=True, exist_ok=True)
            shutil.copyfile(target, saved)
            if digest(saved.read_bytes()) != row["base_sha256"]:
                raise ValueError("backup drift")
    # All target/proposal inputs must still be unchanged after complete backups.
    prepare(root, manifest_path, expected)
    result = {"manifest_sha256": expected, "complete": False, "installed": []}
    receipt = backup / "mount-receipt.json"
    receipt.write_text(json.dumps(result, indent=2) + "\n")
    for row, target, _, content in rows:
        if row["base_sha256"] is None:
            if target.exists():
                raise ValueError("new source appeared during mount")
        elif digest(target.read_bytes()) != row["base_sha256"]:
            raise ValueError("source changed during mount")
        target.parent.mkdir(parents=True, exist_ok=True)
        with target.open("xb" if row["base_sha256"] is None else "wb") as stream:
            stream.write(content)
        if digest(target.read_bytes()) != row["sha256"]:
            raise ValueError("installed source mismatch")
        result["installed"].append(row["target"])
        receipt.write_text(json.dumps(result, indent=2) + "\n")
    result["complete"] = True
    receipt.write_text(json.dumps(result, indent=2) + "\n")
    return result

def main():
    parser = argparse.ArgumentParser()
    parser.add_argument("--repo", type=Path, required=True)
    parser.add_argument("--manifest", type=Path, required=True)
    parser.add_argument("--manifest-sha256", required=True)
    parser.add_argument("--apply", action="store_true")
    parser.add_argument("--backup", type=Path)
    args = parser.parse_args()
    root = args.repo.resolve(strict=True)
    if args.apply:
        if args.backup is None:
            parser.error("--apply requires a fresh --backup")
        result = mount(root, args.manifest, args.manifest_sha256, args.backup)
    else:
        rows = prepare(root, args.manifest, args.manifest_sha256)
        result = {"verify_only": True, "count": len(rows), "targets": [r[0]["target"] for r in rows]}
    print(json.dumps(result))

if __name__ == "__main__":
    main()
