# SPDX-License-Identifier: Apache-2.0
from pathlib import Path
import difflib
import subprocess
root = Path(__file__).resolve().parent
workspace = root.parents[2]
name = "packages/vscode/src/workbenchView.ts"
original = (root / "client-lifetime-base" / name).read_text(encoding="utf-8")
prepared = (root / "client-lifetime-source" / name).read_text(encoding="utf-8")
# Root owns the independent returned-summary save change.
assert original[original.index("    private save("):original.index("    private async run(")] == prepared[prepared.index("    private save("):prepared.index("    private async run(")]
chunks, changed = [], []
for path in sorted((root / "client-lifetime-source").rglob("*.ts")):
    relative = path.relative_to(root / "client-lifetime-source").as_posix()
    old = root / "client-lifetime-base" / relative
    before = old.read_text(encoding="utf-8").splitlines(keepends=True) if old.exists() else []
    after = path.read_text(encoding="utf-8").splitlines(keepends=True)
    if before != after:
        changed.append(relative)
        chunks.extend(difflib.unified_diff(before, after,
            fromfile="a/" + relative if old.exists() else "/dev/null", tofile="b/" + relative))
patch = root / "client-lifetime.patch"
patch.write_text("".join(chunks), encoding="utf-8", newline="\n")
result = subprocess.run(["git", "-c", "safe.directory=" + workspace.as_posix(),
    "apply", "--check", "--ignore-space-change",
    "--directory=" + (root / "client-lifetime-base").relative_to(workspace).as_posix(), str(patch)],
    cwd=workspace, capture_output=True, text=True)
print("changed_files=", len(changed), "recorded_base_apply_check=", result.returncode,
      "save_block_unchanged=True")
print(result.stdout, result.stderr)
raise SystemExit(result.returncode)
