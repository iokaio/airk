# SPDX-License-Identifier: Apache-2.0
"""S7.3 mechanical step: point fence readers at the `checkpoint_fence` leaf.

The read side of `service_runtime::checkpoint` (the fence file name, `Fence`,
`FenceOrigin`, the bounded reader `bytes`, `fence`, `admission_closed` and
`fence_operation`) moved verbatim to `airk_cli::checkpoint_fence`. The writer,
recovery, packets and `settle` stay. This rewrites every qualified path to the
two moved public readers and nothing else:

- `crate::service_runtime::checkpoint::<reader>` anywhere in the crate;
- `service_runtime::checkpoint::<reader>` where `service_runtime` is in scope
  through an import (the `runtime_install` subtree).

The rewrite is idempotent. `--check` changes nothing and exits 1 if any old
path remains or a moved definition is missing from the leaf.
"""
import argparse
from pathlib import Path
import re
import sys

ROOT = Path(__file__).resolve().parents[2]
SRC = ROOT / "crates" / "airk-cli" / "src"
LEAF = SRC / "checkpoint_fence" / "mod.rs"
WRITER = SRC / "service_runtime" / "checkpoint.rs"
READERS = ("admission_closed", "fence_operation")
MOVED = READERS + ("fence", "bytes")
OLD = re.compile(r"(?<![\w:])(?:crate::)?service_runtime::checkpoint::(" + "|".join(READERS) + r")\b")


def definitions_ok():
    leaf = LEAF.read_text(encoding="utf-8") if LEAF.is_file() else ""
    writer = WRITER.read_text(encoding="utf-8")
    problems = []
    for name in MOVED:
        if not re.search(r"\bfn " + name + r"\(", leaf):
            problems.append(f"checkpoint_fence is missing fn {name}")
        if re.search(r"(?m)^(?:pub\S* )?fn " + name + r"\(", writer):
            problems.append(f"service_runtime/checkpoint.rs still defines fn {name}")
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
        count = len(OLD.findall(text))
        if not count:
            continue
        changed += count
        print(f"{path.relative_to(ROOT).as_posix()}: {count}")
        if not args.check:
            path.write_text(OLD.sub(r"crate::checkpoint_fence::\1", text), encoding="utf-8", newline="\n")
    print(f"{'remaining' if args.check else 'rewritten'} qualified paths: {changed}")
    valid = definitions_ok()
    return 1 if (args.check and changed) or not valid else 0


if __name__ == "__main__":
    sys.exit(main())
