# SPDX-License-Identifier: Apache-2.0
from pathlib import Path
import difflib

root = Path(__file__).resolve().parents[2]
module = (root / "artifacts/v190/p11a-profile_families.rs").read_text(encoding="utf-8")
lib_path = root / "crates/airk-core/src/lib.rs"
before = lib_path.read_text(encoding="utf-8")
anchor = "pub mod publication;\n"
assert before.count(anchor) == 1
assert "pub mod profile_families;" not in before
after = before.replace(anchor, "pub mod profile_families;\n" + anchor)
patch = "".join(difflib.unified_diff(
    before.splitlines(keepends=True), after.splitlines(keepends=True),
    fromfile="a/crates/airk-core/src/lib.rs", tofile="b/crates/airk-core/src/lib.rs",
))
patch += "".join(difflib.unified_diff(
    [], module.splitlines(keepends=True),
    fromfile="/dev/null", tofile="b/crates/airk-core/src/profile_families.rs",
))
(root / "artifacts/v190/p11a-family-reporting.patch").write_text(patch, encoding="utf-8", newline="\n")
print("Prepared inert Core patch; tracked files were only read.")
