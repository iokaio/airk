# SPDX-License-Identifier: Apache-2.0
"""One-off Root-operated optional-source mount. Default verifies only; no Cargo."""
import argparse
import hashlib
import importlib.util
import json
import os
from pathlib import Path

ROOT = Path(r"D:/code/Github/airk")
MANIFEST = Path(__file__).with_name("mount-manifest.json")

def digest(data):
    return hashlib.sha256(data).hexdigest()

def safe(path):
    path = Path(path)
    path.relative_to(ROOT)
    # Refuse alias/reparse traversal rather than resolving away evidence.
    for current in (path, *path.parents):
        if current == ROOT.parent:
            break
        if current.is_symlink() or (hasattr(current, "is_junction") and current.is_junction()):
            raise ValueError(f"alias refused: {current}")
    return path

def read(path):
    path = safe(path)
    if not path.exists():
        if os.path.lexists(path):
            raise ValueError(f"nonregular absent path: {path}")
        return None
    if not path.is_file():
        raise ValueError(f"not a file: {path}")
    return path.read_bytes()

def checked(path, expected):
    data = read(path)
    actual = None if data is None else digest(data)
    if actual != expected:
        raise ValueError(f"hash mismatch: {path}: expected {expected}, actual {actual}")
    return data

def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("--manifest-sha256", required=True)
    parser.add_argument("--apply", action="store_true")
    parser.add_argument("--backup", type=Path)
    args = parser.parse_args()
    m = json.loads(checked(MANIFEST, args.manifest_sha256))
    if Path(m["main_root"]) != ROOT or len(m["rows"]) != 25:
        raise ValueError("unexpected one-off mount scope")
    # ALL input/base/proposal reads and validation finish before ANY copy or mkdir.
    for pin in m["inputs"]:
        checked(pin["path"], pin["sha256"])
    snapshots = []
    seen = set()
    for row in m["rows"]:
        target = safe(row["target"])
        if target in seen:
            raise ValueError("duplicate target")
        seen.add(target)
        before = checked(target, row["base_sha256"])
        after = checked(row["proposal"], row["proposal_sha256"])
        if row["mode"] == "append":
            extra = checked(row["original_append"]["path"], row["original_append"]["sha256"])
            if after != before + b"\n" + extra or extra in before:
                raise ValueError("contract append changed existing bytes or duplicates text")
        snapshots.append((target, before, after, row))
    # Authenticate canonical generator outputs against actual R150 native schema.
    spec = importlib.util.spec_from_file_location("reviewed_contract_types", ROOT / "scripts/dev/contract_types.py")
    generator = importlib.util.module_from_spec(spec)
    spec.loader.exec_module(generator)
    schema_row = next(row for row in m["rows"] if Path(row["target"]) == ROOT / "packages/contracts-gen/arbiter.schema.json")
    isolated = ROOT / "artifacts/advanced-split/core-contract-gate-r149"
    generator.ROOT = isolated
    generator.CONTRACTS["arbiter"] = Path(schema_row["proposal"])
    generated = generator.generate("arbiter").encode("utf-8")
    generator.declaration_only(generated.decode("utf-8"), "arbiter")
    for rel in ("packages/contracts-gen/arbiter.ts", "packages/vscode/src/generated/arbiter.d.ts"):
        row = next(x for x in m["rows"] if Path(x["target"]) == ROOT / rel)
        if checked(row["proposal"], row["proposal_sha256"]) != generated:
            raise ValueError("generated types do not match native schema")
    print(json.dumps({"preflight": "passed", "targets": len(snapshots), "apply": args.apply}))
    if not args.apply:
        return
    if args.backup is None:
        raise ValueError("--apply requires fresh private --backup directory")
    backup = safe(args.backup)
    backup.relative_to(ROOT / "artifacts/advanced-split/model-download-origin-preparation-r1")
    if os.path.lexists(backup):
        raise ValueError("backup must be fresh")
    backup.mkdir(parents=True, exist_ok=False)
    for target, before, after, row in snapshots:
        if before is not None:
            dest = backup / target.relative_to(ROOT)
            dest.parent.mkdir(parents=True, exist_ok=True)
            with dest.open("xb") as stream:
                stream.write(before)
    # Recheck ALL target baselines once more before the first Main write.
    for target, before, after, row in snapshots:
        checked(target, row["base_sha256"])
    changed = []
    try:
        for target, before, after, row in snapshots:
            checked(target, row["base_sha256"])
            target.parent.mkdir(parents=True, exist_ok=True)
            with target.open("wb" if before is not None else "xb") as stream:
                stream.write(after)
                stream.flush()
                os.fsync(stream.fileno())
            changed.append((target, before, after))
            checked(target, row["proposal_sha256"])
    except BaseException:
        # Restore only our own exact bytes; never overwrite newer concurrent work.
        for target, before, after in reversed(changed):
            checked(target, digest(after))
            if before is None:
                target.unlink()
            else:
                target.write_bytes(before)
        raise
    receipt = {"complete": True, "manifest_sha256": args.manifest_sha256,
               "targets": [{"path": str(t), "before_sha256": None if b is None else digest(b),
                            "after_sha256": digest(a)} for t, b, a, row in snapshots],
               "qualification_granted": False}
    (backup / "mount-receipt.json").write_text(json.dumps(receipt, indent=2) + "\n", encoding="utf-8")

if __name__ == "__main__":
    main()
