# SPDX-License-Identifier: Apache-2.0
from pathlib import Path
import difflib
import subprocess
root = Path(__file__).resolve().parent
workspace = root.parents[2]
chunks, changed = [], []
for path in sorted((root / "source").rglob("*.ts")):
    relative = path.relative_to(root / "source").as_posix()
    old = root / "base" / relative
    before = old.read_text(encoding="utf-8").splitlines(keepends=True) if old.exists() else []
    after = path.read_text(encoding="utf-8").splitlines(keepends=True)
    if before != after:
        changed.append(relative)
        chunks.extend(difflib.unified_diff(before, after,
            fromfile="a/" + relative if old.exists() else "/dev/null", tofile="b/" + relative))
patch = root / "return-preparation.patch"
patch.write_text("".join(chunks), encoding="utf-8", newline="\n")
result = subprocess.run(["git", "-c", "safe.directory=" + workspace.as_posix(),
    "apply", "--check", "--ignore-space-change",
    "--directory=" + (root / "base").relative_to(workspace).as_posix(), str(patch)],
    cwd=workspace, capture_output=True, text=True)
print("changed_files=", len(changed), "recorded_base_apply_check=", result.returncode)
print(result.stdout, result.stderr)
raise SystemExit(result.returncode)
