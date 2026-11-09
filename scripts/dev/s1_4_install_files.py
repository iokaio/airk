# SPDX-License-Identifier: Apache-2.0
"""Milestone S1.4 mechanical step: point callers at `install_files`.

`safe_file`, `main_name`, `MAC_LAUNCHER_IMAGE` and `launcher_path` moved
verbatim from `airk_cli::runtime_install` to the leaf module
`airk_cli::install_files`. This rewrites every qualified path
(`crate::runtime_install::X` inside the library, `airk_cli::runtime_install::X`
in integration tests and binaries) and nothing else. Unqualified uses inside the
`runtime_install` subtree keep resolving through that module's import.

The rewrite is idempotent. `--check` changes nothing and exits 1 if any
qualified old path remains, so reviewers can re-run it against the commit.
"""
import argparse
from pathlib import Path
import re
import sys

ROOT = Path(__file__).resolve().parents[2]
CRATE = ROOT / "crates" / "airk-cli"
MOVED = ("safe_file", "main_name", "MAC_LAUNCHER_IMAGE", "launcher_path")
OLD = re.compile(r"\b(crate|airk_cli)::runtime_install::(" + "|".join(MOVED) + r")\b")


def main(argv=None):
    parser = argparse.ArgumentParser(description=__doc__.splitlines()[0])
    parser.add_argument("--check", action="store_true", help="report remaining old paths without editing")
    args = parser.parse_args(argv)
    changed = 0
    for path in sorted(CRATE.rglob("*.rs")):
        if "target" in path.parts:
            continue
        text = path.read_text(encoding="utf-8")
        count = len(OLD.findall(text))
        if not count:
            continue
        changed += count
        relative = path.relative_to(ROOT).as_posix()
        print(f"{relative}: {count}")
        if not args.check:
            path.write_text(OLD.sub(r"\1::install_files::\2", text), encoding="utf-8", newline="\n")
    print(f"{'remaining' if args.check else 'rewritten'} qualified paths: {changed}")
    return 1 if args.check and changed else 0


if __name__ == "__main__":
    sys.exit(main())
