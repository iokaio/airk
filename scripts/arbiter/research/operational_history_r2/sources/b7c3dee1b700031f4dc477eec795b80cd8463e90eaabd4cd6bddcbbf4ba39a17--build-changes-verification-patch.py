# SPDX-License-Identifier: Apache-2.0
"""Refresh the staged QA #3 diff from reviewed, formatted copies only."""
from pathlib import Path
import difflib

root = Path(__file__).resolve().parents[2]
base = Path(__file__).resolve().parent / "changes-verification-copies"
patch = []
for path in sorted(base.rglob("*.rs")):
    name = path.relative_to(base).as_posix()
    original = root / name
    before = original.read_text(encoding="utf-8") if original.exists() else ""
    after = path.read_text(encoding="utf-8")
    patch.extend(difflib.unified_diff(
        before.splitlines(keepends=True), after.splitlines(keepends=True),
        fromfile="a/" + name if before else "/dev/null", tofile="b/" + name))
destination = base.parent / "changes-verification.patch"
destination.write_text("".join(patch), encoding="utf-8", newline="\n")
print(destination)
