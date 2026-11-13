# SPDX-License-Identifier: Apache-2.0
"""Compare the test identities and outcomes of two sets of `cargo test` logs (CLAUDE.md:
structural steps compare selected test identities and outcomes, not counts).

    python scripts/dev/compare_test_sets.py --before a.log [--before …] --after b.log [--after …]

A test is identified by its path inside its crate (`module::tests::name`); moving modules into a
new crate keeps those paths, so the union of the after logs must equal the before set, with the
same outcome for each test. Exits 1 on any difference and lists it.
"""
import argparse
import re
import sys
from pathlib import Path

LINE = re.compile(r"^test (?P<name>\S+) \.\.\. (?P<outcome>ok|ignored|FAILED)\b")


def collect(paths):
    found = {}
    for path in paths:
        text = Path(path).read_text(encoding="utf-8", errors="replace")
        # PowerShell may wrap long lines; join a wrapped "test …" line with its continuation.
        text = re.sub(r"\ntest \n", "\ntest ", text)
        for line in text.splitlines():
            match = LINE.match(line.strip())
            if match:
                found[match["name"]] = match["outcome"]
    return found


def main(argv=None):
    parser = argparse.ArgumentParser(description=__doc__.splitlines()[0])
    parser.add_argument("--before", action="append", required=True)
    parser.add_argument("--after", action="append", required=True)
    args = parser.parse_args(argv)
    before, after = collect(args.before), collect(args.after)
    missing = sorted(set(before) - set(after))
    added = sorted(set(after) - set(before))
    changed = sorted(name for name in set(before) & set(after) if before[name] != after[name])
    for name in missing:
        print(f"missing: {name} ({before[name]})")
    for name in added:
        print(f"added: {name} ({after[name]})")
    for name in changed:
        print(f"changed: {name}: {before[name]} -> {after[name]}")
    print(f"before {len(before)}, after {len(after)}; missing {len(missing)}, added {len(added)}, changed {len(changed)}")
    if not before:
        print("no tests parsed from the before logs", file=sys.stderr)
        return 1
    return 1 if missing or added or changed else 0


if __name__ == "__main__":
    sys.exit(main())
