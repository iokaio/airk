# SPDX-License-Identifier: Apache-2.0
"""S7.2 mechanical step: point callers at the `install_layout` leaf.

The control-binding core of `runtime_install::layout` (the layout receipts,
control binding, registry pin and the successor receipt reader) moved verbatim
to `airk_cli::install_layout`, together with the `validate_control_root`,
`record_control_root` and `record_registry_root` facades. The duplicate
`runtime_install::log_parent` facade was removed; callers use the leaf's
`log_parent`. This rewrites the qualified call sites and nothing else:

- `crate::runtime_install::<facade>` anywhere in the crate;
- `layout::<moved function>` inside the `runtime_install` subtree, including
  `runtime_macos_login.rs`, which it mounts with `#[path]`;
- the one unqualified `validate_control_root(` inside that subtree.

Initialization, `control()`, `prepare_managed_control` and the successor writer
stay in `runtime_install::layout`, so `layout::` paths to them are untouched.
The rewrite is idempotent. `--check` changes nothing and exits 1 if any old
path remains or a moved definition is missing from the leaf.
"""
import argparse
from pathlib import Path
import re
import sys

ROOT = Path(__file__).resolve().parents[2]
SRC = ROOT / "crates" / "airk-cli" / "src"
LEAF = SRC / "install_layout" / "mod.rs"
STAYED = SRC / "runtime_install" / "layout.rs"
FACADES = ("validate_control_root", "record_control_root", "record_registry_root", "log_parent")
MOVED = (
    "abort", "initialization_control", "inspect", "installation_for_control",
    "issuer_rotation_control", "log_parent", "managed_origin", "record_control",
    "record_registry", "require_managed_ready", "retained_parameters",
    "retained_private_paths", "saved_control", "validate_catalog_control", "validate_control",
)
CRATE_WIDE = re.compile(r"\bcrate::runtime_install::(" + "|".join(FACADES) + r")\b")
SUBTREE = [
    (re.compile(r"(?<![\w:])layout::(" + "|".join(MOVED) + r")\b"), r"crate::install_layout::\1"),
    (re.compile(r"(?<![\w:])(validate_control_root)\("), r"crate::install_layout::\1("),
]


# Files mounted into `runtime_install` from outside its directory with `#[path]`.
MOUNTED = {"runtime_macos_login.rs"}


def in_subtree(path):
    relative = path.relative_to(SRC).as_posix()
    return relative in {"runtime_install.rs"} | MOUNTED or relative.startswith("runtime_install/")


def rewrite(path, text):
    count = len(CRATE_WIDE.findall(text))
    text = CRATE_WIDE.sub(r"crate::install_layout::\1", text)
    if in_subtree(path):
        for pattern, replacement in SUBTREE:
            count += len(pattern.findall(text))
            text = pattern.sub(replacement, text)
    return text, count


def definitions_ok():
    leaf = LEAF.read_text(encoding="utf-8") if LEAF.is_file() else ""
    stayed = STAYED.read_text(encoding="utf-8")
    problems = []
    for name in MOVED + FACADES[:3]:
        if not re.search(r"\bfn " + name + r"\(", leaf):
            problems.append(f"install_layout is missing fn {name}")
        if re.search(r"\bfn " + name + r"\(", stayed):
            problems.append(f"runtime_install/layout.rs still defines fn {name}")
    for problem in problems:
        print(problem)
    return not problems


def main(argv=None):
    parser = argparse.ArgumentParser(description=__doc__.splitlines()[0])
    parser.add_argument("--check", action="store_true", help="report remaining old paths without editing")
    args = parser.parse_args(argv)
    changed = 0
    for path in sorted(SRC.rglob("*.rs")):
        text = path.read_text(encoding="utf-8")
        new, count = rewrite(path, text)
        if not count:
            continue
        changed += count
        print(f"{path.relative_to(ROOT).as_posix()}: {count}")
        if not args.check:
            path.write_text(new, encoding="utf-8", newline="\n")
    print(f"{'remaining' if args.check else 'rewritten'} qualified paths: {changed}")
    valid = definitions_ok()
    return 1 if (args.check and changed) or not valid else 0


if __name__ == "__main__":
    sys.exit(main())
