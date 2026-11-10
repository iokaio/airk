# SPDX-License-Identifier: Apache-2.0
"""S7.5 mechanical step: point Store callers at the `store_fs` leaf.

Step S7.5 moved the Store filesystem and lock layer verbatim into
`airk_store::store_fs`:

- the ownership-file helpers (`OwnershipFile`, `acquire_*`,
  `open_existing_ownership_file`) from `service`;
- `sync_parent` from `maintenance`;
- `canonical_location` (and its `exists` helper) from `service::data_scope`;
- the setup directory handle (`SetupDirectory`, `SetupFileMode`,
  `file_identity`, `SetupEntries` and the Windows HANDLE implementation) from
  `service::registry::catalog::setup::storage`.

`QuotaPressure` and `PressureScope` moved down into `capacity`; `quota`
re-exports them, so only `capacity` itself is rewritten. `service::data_scope`
re-exports `canonical_location` for airk-kernel. The old owners import what
they still use, so their own submodules need no edits.

This rewrites qualified paths inside airk-store and nothing else:

- `crate::service::<ownership helper>` and `crate::maintenance::sync_parent`;
- `crate::service::data_scope::canonical_location` (outside `service`);
- every `...storage::<handle item>` path and `...storage::{...}` group whose
  entries all moved; three mixed groups that also import the staying
  `write_exact` are split explicitly;
- `crate::quota::{QuotaPressure,PressureScope}` inside `capacity.rs`.

The rewrite is idempotent. `--check` changes nothing and exits 1 if any old
path remains or a moved definition is missing from the leaf.
"""
import argparse
from pathlib import Path
import re
import sys

ROOT = Path(__file__).resolve().parents[2]
SRC = ROOT / "crates" / "airk-store" / "src"
LEAF = SRC / "store_fs"
OWNERSHIP = (
    "OwnershipFile", "acquire_file", "acquire_file_for", "acquire_existing_file",
    "acquire_existing_file_for", "acquire_shared_file", "open_existing_ownership_file",
)
HANDLE = ("SetupDirectory", "SetupFileMode", "file_identity", "SetupEntries")
PRESSURE = ("QuotaPressure", "PressureScope")

RULES = [
    (re.compile(r"\bcrate::service::(" + "|".join(OWNERSHIP) + r")\b"), r"crate::store_fs::\1"),
    (re.compile(r"\bcrate::maintenance::sync_parent\b"), r"crate::store_fs::sync_parent"),
    (re.compile(r"\bcrate::service::data_scope::canonical_location\b"), r"crate::store_fs::canonical_location"),
    (re.compile(r"(?<![\w:])(?:[A-Za-z_]+::)*storage::(" + "|".join(HANDLE) + r")\b"), r"crate::store_fs::\1"),
]
GROUP = re.compile(r"(?<![\w:])(?:[A-Za-z_]+::)*storage::\{([^{}]*)\}")
CAPACITY = re.compile(r"\bcrate::quota::(" + "|".join(PRESSURE) + r")\b")

# Mixed groups: the moved names go to the leaf, `write_exact` stays in storage.
MIXED = {
    "service/collection/purge.rs": [(
        "            use crate::service::registry::catalog::setup::storage::{SetupDirectory, write_exact};\n",
        "            use crate::service::registry::catalog::setup::storage::write_exact;\n"
        "            use crate::store_fs::SetupDirectory;\n",
    )],
    "service/service_data_scope_clean_break.rs": [(
        "    catalog::setup::storage::{SetupDirectory, SetupFileMode, file_identity, write_exact},\n};\n",
        "    catalog::setup::storage::write_exact,\n};\n"
        "use crate::store_fs::{SetupDirectory, SetupFileMode, file_identity};\n",
    )],
    "service/registry/catalog/split/tests.rs": [(
        "    use super::super::setup::storage::{SetupDirectory, file_identity, write_exact};\n",
        "    use super::super::setup::storage::write_exact;\n"
        "    use crate::store_fs::{SetupDirectory, file_identity};\n",
    )],
}
DEFINITIONS = {
    "mod.rs": OWNERSHIP + ("sync_parent", "canonical_location", "exists"),
    "setup_directory.rs": ("SetupFileMode", "SetupDirectory", "file_identity", "SetupEntries"),
}


def group(match):
    names = [name.strip() for name in match.group(1).split(",") if name.strip()]
    if names and all(name in HANDLE for name in names):
        return f"crate::store_fs::{{{match.group(1)}}}"
    return match.group(0)


def rewrite(relative, text):
    count = 0
    for old, new in MIXED.get(relative, []):
        found = text.count(old)
        count += found
        text = text.replace(old, new)
    for pattern, replacement in RULES:
        count += len(pattern.findall(text))
        text = pattern.sub(replacement, text)
    for match in GROUP.finditer(text):
        if group(match) != match.group(0):
            count += 1
    text = GROUP.sub(group, text)
    if relative == "capacity.rs":
        count += len(CAPACITY.findall(text))
        text = CAPACITY.sub(r"\1", text)
    return text, count


def definitions_ok():
    problems = []
    for name, items in DEFINITIONS.items():
        path = LEAF / name
        text = path.read_text(encoding="utf-8") if path.is_file() else ""
        for item in items:
            if not re.search(r"\b(?:struct|enum|fn) " + item + r"\b", text):
                problems.append(f"store_fs/{name} is missing {item}")
    capacity = (SRC / "capacity.rs").read_text(encoding="utf-8")
    for item in PRESSURE:
        if not re.search(r"\bpub (?:struct|enum) " + item + r"\b", capacity):
            problems.append(f"capacity is missing {item}")
    if not (LEAF / "setup_directory" / "windows.rs").is_file():
        problems.append("store_fs/setup_directory/windows.rs is missing")
    for problem in problems:
        print(problem)
    return not problems


def main(argv=None):
    parser = argparse.ArgumentParser(description=__doc__.splitlines()[0])
    parser.add_argument("--check", action="store_true", help="report remaining old paths without editing")
    args = parser.parse_args(argv)
    changed = 0
    for path in sorted(SRC.rglob("*.rs")):
        relative = path.relative_to(SRC).as_posix()
        if relative.startswith("store_fs/"):
            continue
        text = path.read_text(encoding="utf-8")
        new, count = rewrite(relative, text)
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
