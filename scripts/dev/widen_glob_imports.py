# SPDX-License-Identifier: Apache-2.0
"""Widen the items a glob import (`use crate::<module>::*`) reached across a new crate boundary.

    python scripts/dev/widen_glob_imports.py --importer crates/airk-cli/src/runtime_commands \
        --owner crates/airk-installer/src/runtime_install.rs -p airk-installer -p airk-cli \
        [--target-dir DIR] [--rounds 10]

Inside one crate, `use crate::m::*` also brought `pub(crate)` items and private submodules of `m`;
from another crate it brings only `pub` items. Each round compiles, collects the names the
importer's files cannot resolve, and makes exactly those top-level definitions in the owner file
`pub` (a `#[cfg(test)] mod tests` becomes `test-support`). Prints every widened name.
"""
import argparse
import json
import re
import subprocess
import sys
from pathlib import Path

ROOT = Path(__file__).resolve().parents[2]
UNRESOLVED = re.compile(r"cannot find (?:function|value|type|struct|trait|macro|constant|module or crate) `(\w+)`"
                        r"|unlinked crate `(\w+)`")
DEFINITION = re.compile(r"^(?P<vis>pub\((?:crate|super)\)\s+|)(?P<rest>(?:(?:async|const|unsafe)\s+)*"
                        r"(?:mod|fn|const|static|struct|enum|type|trait|use)\s+(?P<name>\w+)\b.*)$")


def unresolved(packages, target_dir, importer):
    command = ["cargo", "check", "--locked", "--all-targets", "--message-format=json"]
    for package in packages:
        command += ["-p", package]
    if target_dir:
        command += ["--target-dir", target_dir]
    result = subprocess.run(command, cwd=ROOT, text=True, encoding="utf-8", stdout=subprocess.PIPE,
                            stderr=subprocess.PIPE)
    names = set()
    for line in result.stdout.splitlines():
        try:
            event = json.loads(line)
        except ValueError:
            continue
        message = event.get("message") if event.get("reason") == "compiler-message" else None
        if not message or message.get("level") != "error":
            continue
        span = next(iter(message.get("spans", [])), None)
        if not span or importer not in str(ROOT / span["file_name"]).replace("\\", "/"):
            continue
        match = UNRESOLVED.search(message["message"])
        if match:
            names.add(match.group(1) or match.group(2))
    return names


def widen(owner, names):
    lines = owner.read_text(encoding="utf-8").split("\n")
    done = set()
    for index, line in enumerate(lines):
        match = DEFINITION.match(line)
        if match and match["name"] in names and not line.startswith("pub "):
            lines[index] = "pub " + match["rest"]
            done.add(match["name"])
    text = "\n".join(lines)
    if "tests" in names and "#[cfg(test)]\npub(crate) mod tests;" in text:
        text = text.replace("#[cfg(test)]\npub(crate) mod tests;",
                            '#[cfg(any(test, feature = "test-support"))]\npub mod tests;', 1)
        done.add("tests")
    owner.write_text(text, encoding="utf-8", newline="")
    return done


def main(argv=None):
    parser = argparse.ArgumentParser(description=__doc__.splitlines()[0])
    parser.add_argument("--importer", required=True)
    parser.add_argument("--owner", required=True, type=Path)
    parser.add_argument("-p", "--package", action="append", required=True)
    parser.add_argument("--target-dir")
    parser.add_argument("--rounds", type=int, default=10)
    args = parser.parse_args(argv)
    importer = str((ROOT / args.importer).resolve()).replace("\\", "/")
    owner = args.owner if args.owner.is_absolute() else ROOT / args.owner
    for round_number in range(1, args.rounds + 1):
        names = unresolved(args.package, args.target_dir, importer)
        if not names:
            print(f"round {round_number}: no unresolved names in the importer")
            return 0
        done = widen(owner, names)
        print(f"round {round_number}: widened {sorted(done)}; not top-level in the owner: {sorted(names - done)}")
        if not done:
            return 1
    return 1


if __name__ == "__main__":
    sys.exit(main())
