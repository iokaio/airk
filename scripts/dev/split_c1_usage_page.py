# SPDX-License-Identifier: Apache-2.0
"""Split plan step C1: move the pure usage-page codec from airk-kernel to airk-core.

A structural move only: the file moves (git mv) with its `airk_core::` paths
rewritten to `crate::`, airk-core declares the module, and airk-kernel keeps
`pub use airk_core::usage_page;` so every existing path
(`airk_kernel::usage_page::…`) still resolves. No consumer edits.

Run from the repository root on a clean tree: python scripts/dev/split_c1_usage_page.py
Prints the old -> new test identity prefix map for `test_identities.py compare`.
"""
import json
from pathlib import Path
import subprocess

ROOT = Path(__file__).resolve().parents[2]
SOURCE = "crates/airk-kernel/src/usage_page.rs"
TARGET = "crates/airk-core/src/usage_page.rs"
KERNEL_DECLARATION = "pub mod usage_page;\n"
KERNEL_REEXPORT = ("/// Moved to `airk-core` (split plan C1); re-exported so existing paths resolve.\n"
                   "pub use airk_core::usage_page;\n")


def replace_once(path, old, new):
    text = path.read_text(encoding="utf-8")
    if text.count(old) != 1:
        raise SystemExit(f"{path}: expected exactly one {old!r}")
    path.write_text(text.replace(old, new), encoding="utf-8", newline="\n")


def main():
    if not (ROOT / SOURCE).is_file() or (ROOT / TARGET).exists():
        raise SystemExit("C1 already applied or tree differs")
    subprocess.run(["git", "mv", SOURCE, TARGET], cwd=ROOT, check=True)
    # Inside airk-core, the kernel's `airk_core::` paths become `crate::` paths.
    moved = ROOT / TARGET
    text = moved.read_text(encoding="utf-8")
    moved.write_text(text.replace("airk_core::", "crate::"), encoding="utf-8", newline="\n")
    replace_once(ROOT / "crates/airk-kernel/src/lib.rs", KERNEL_DECLARATION, KERNEL_REEXPORT)
    core = ROOT / "crates/airk-core/src/lib.rs"
    if "pub mod usage_page;" in core.read_text(encoding="utf-8"):
        raise SystemExit("airk-core already declares usage_page")
    replace_once(core, "pub mod rebuild;\n", "pub mod rebuild;\npub mod usage_page;\n")
    print(json.dumps({"identity_map": {"airk-kernel::lib/airk_kernel::usage_page::":
                                       "airk-core::lib/airk_core::usage_page::"}}))


if __name__ == "__main__":
    main()
