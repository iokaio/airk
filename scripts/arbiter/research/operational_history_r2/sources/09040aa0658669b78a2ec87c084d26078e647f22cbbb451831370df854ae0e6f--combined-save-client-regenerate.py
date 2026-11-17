# SPDX-License-Identifier: Apache-2.0
from pathlib import Path
import difflib
import hashlib
import json
import subprocess
root = Path(__file__).resolve().parent
workspace = root.parents[2]
manifest = json.loads((root / "combined-save-client-BASE.json").read_text(encoding="utf-8"))
for name, expected in manifest["inputs"].items():
    assert hashlib.sha256((root / name).read_bytes()).hexdigest() == expected, "original package changed; consciously rebase combined source"
chunks, changed = [], []
for path in sorted((root / "combined-save-client-source").rglob("*.ts")):
    relative = path.relative_to(root / "combined-save-client-source").as_posix()
    old = root / "combined-save-client-base" / relative
    before = old.read_text(encoding="utf-8").splitlines(keepends=True) if old.exists() else []
    after = path.read_text(encoding="utf-8").splitlines(keepends=True)
    if before != after:
        changed.append(relative)
        chunks.extend(difflib.unified_diff(before, after,
            fromfile="a/" + relative if old.exists() else "/dev/null", tofile="b/" + relative))
patch = root / "combined-save-client.patch"
patch.write_text("".join(chunks), encoding="utf-8", newline="\n")
result = subprocess.run(["git", "-c", "safe.directory=" + workspace.as_posix(),
    "apply", "--check", "--ignore-space-change",
    "--directory=" + (root / "combined-save-client-base").relative_to(workspace).as_posix(), str(patch)],
    cwd=workspace, capture_output=True, text=True)
print("changed_files=", len(changed), "recorded_current_base_apply_check=", result.returncode)
print(result.stdout, result.stderr)
raise SystemExit(result.returncode)
