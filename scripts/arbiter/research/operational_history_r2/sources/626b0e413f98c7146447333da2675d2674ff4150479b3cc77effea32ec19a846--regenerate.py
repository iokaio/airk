# SPDX-License-Identifier: Apache-2.0
from pathlib import Path
import difflib
import subprocess

root = Path(__file__).resolve().parent
workspace = root.parents[2]
for base_name, source_name, patch_name in [
    ("base", "source", "metadata-native-pages.patch"),
    ("f11-base", "f11-source", "verify-projections-after-f11.patch"),
]:
    base, source = root / base_name, root / source_name
    chunks, changed = [], []
    for path in sorted(source.rglob("*")):
        if not path.is_file():
            continue
        relative = path.relative_to(source).as_posix()
        old = base / relative
        before = old.read_text(encoding="utf-8").splitlines(keepends=True) if old.exists() else []
        after = path.read_text(encoding="utf-8").splitlines(keepends=True)
        if before == after:
            continue
        changed.append(relative)
        chunks.extend(difflib.unified_diff(before, after,
            fromfile="a/" + relative if old.exists() else "/dev/null", tofile="b/" + relative))
    patch = root / patch_name
    patch.write_text("".join(chunks), encoding="utf-8", newline="\n")
    result = subprocess.run(["git", "-c", "safe.directory=" + workspace.as_posix(),
        "apply", "--check", "--ignore-space-change",
        "--directory=" + base.relative_to(workspace).as_posix(),
        str(patch)], cwd=workspace, capture_output=True, text=True)
    print(patch_name, "changed_files=", len(changed), "apply_check=", result.returncode)
    if result.stdout:
        print(result.stdout)
    if result.stderr:
        print(result.stderr)
    if result.returncode:
        raise SystemExit(result.returncode)
