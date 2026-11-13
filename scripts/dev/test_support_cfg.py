# SPDX-License-Identifier: Apache-2.0
"""Open one extracted crate's test-only helpers to the tests of crates built on it.

    python scripts/dev/test_support_cfg.py crates/airk-inference [--dry-run]

In every non-test source file of the crate, the `test` predicate inside each `#[cfg(...)]`
attribute becomes `any(test, feature = "test-support")`, so `airk-cli`'s tests (which enable the
crate's `test-support` feature as a dev-dependency) can use the helpers they used when everything
was one crate. Attributes on the crate's own test-module declarations (`mod tests`, `mod *_tests`)
stay `cfg(test)`: those hold `#[test]` functions only. Production builds never enable the feature.
"""
import argparse
import re
import sys
from pathlib import Path

ROOT = Path(__file__).resolve().parents[2]
ATTRIBUTE = re.compile(r"#\[cfg\((?P<expr>.*)\)\]\s*$")
TEST_WORD = re.compile(r"(?<![\w\"])test(?![\w\"])")
TEST_MODULE = re.compile(r"^\s*(?:pub(?:\([^)]*\))?\s+)?mod\s+(?:tests|\w+_tests)\s*[;{]")
SUPPORT = 'any(test, feature = "test-support")'


def convert(text):
    lines = text.split("\n")
    changed = 0
    for index, line in enumerate(lines):
        match = ATTRIBUTE.search(line)
        if not match or not TEST_WORD.search(match["expr"]) or "test-support" in match["expr"]:
            continue
        following = next((later for later in lines[index + 1:] if not later.strip().startswith("#[")), "")
        if TEST_MODULE.match(following):
            continue
        expression = TEST_WORD.sub(SUPPORT, match["expr"])
        lines[index] = line[:match.start()] + f"#[cfg({expression})]" + line[match.end():]
        changed += 1
    return "\n".join(lines), changed


def main(argv=None):
    parser = argparse.ArgumentParser(description=__doc__.splitlines()[0])
    parser.add_argument("crate", type=Path)
    parser.add_argument("--dry-run", action="store_true")
    args = parser.parse_args(argv)
    source = (args.crate if args.crate.is_absolute() else ROOT / args.crate) / "src"
    total = 0
    for path in sorted(source.rglob("*.rs")):
        if path.name == "tests.rs" or path.name.endswith("_tests.rs") or "tests" in path.relative_to(source).parts[:-1]:
            continue
        text, changed = convert(path.read_text(encoding="utf-8"))
        if changed:
            total += changed
            print(f"{path.relative_to(ROOT)}: {changed}")
            if not args.dry_run:
                path.write_text(text, encoding="utf-8", newline="")
    print(f"{total} attributes")
    return 0


if __name__ == "__main__":
    sys.exit(main())
