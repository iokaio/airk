# SPDX-License-Identifier: Apache-2.0
"""Count the call sites of the old index store's public functions.

Step P1.1 of the indexing performance plan needs to know which Store
functions the rest of the product calls on an index. This census finds the
public functions of the reader, the writer and the file store, and counts
textual call sites by crate, apart from tests. It reads source text only.

A count is a sizing aid, not a proof: a common name such as `head` also
matches calls on other types. The census marks such names. The compiler is
the authority once the old functions are removed.
"""
import argparse
from collections import defaultdict
import json
from pathlib import Path
import re
import sys

OWNERS = ("CurrentRead", "CurrentWriter", "FileStore")
IMPL = re.compile(r"^\s*impl(?:<[^>]*>)?\s+(?:crate::|super::|airk_store::)*(\w+)(?:<[^>]*>)?\s*\{")
FUNCTION = re.compile(r"^\s*pub(?:\((?:crate|super)\))?\s+fn\s+([a-z_][a-z0-9_]*)")
TEST_PARTS = {"tests", "benches", "examples"}


def is_test_path(path):
    parts = set(path.parts)
    name = path.name
    return bool(parts & TEST_PARTS) or name == "tests.rs" or name.endswith("_tests.rs")


def public_functions(text):
    """Yield (owner, function, visibility) for `pub fn` items inside an impl of an owner."""
    owner = None
    depth = 0
    for line in text.splitlines():
        if owner is None:
            match = IMPL.match(line)
            if match and match.group(1) in OWNERS:
                owner = match.group(1)
                depth = line.count("{") - line.count("}")
                if depth <= 0:
                    owner = None
            continue
        if depth == 1:
            match = FUNCTION.match(line)
            if match:
                visibility = "crate" if "pub(" in line.split("fn")[0] else "public"
                yield owner, match.group(1), visibility
        depth += line.count("{") - line.count("}")
        if depth <= 0:
            owner = None


def collect(crates):
    """Map function name to its owners, defining files and visibility."""
    functions = defaultdict(lambda: {"owners": set(), "files": set(), "visibility": set()})
    for path in sorted((crates / "airk-store" / "src").rglob("*.rs")):
        if is_test_path(path.relative_to(crates)):
            continue
        for owner, name, visibility in public_functions(path.read_text(encoding="utf-8")):
            entry = functions[name]
            entry["owners"].add(owner)
            entry["files"].add(path.relative_to(crates).as_posix())
            entry["visibility"].add(visibility)
    return functions


def count_calls(crates, names):
    """Count `.name(` and `::name(` occurrences by crate, apart from tests."""
    pattern = re.compile(r"(?:\.|::)(" + "|".join(sorted(map(re.escape, names))) + r")\s*\(")
    counts = {name: defaultdict(int) for name in names}
    tests = defaultdict(int)
    for path in sorted(crates.rglob("*.rs")):
        relative = path.relative_to(crates)
        crate = relative.parts[0]
        text = path.read_text(encoding="utf-8", errors="replace")
        for match in pattern.finditer(text):
            if is_test_path(relative):
                tests[match.group(1)] += 1
            else:
                counts[match.group(1)][crate] += 1
    return counts, tests


def all_function_names(crates):
    """Every `fn` name in the workspace with how many files define it."""
    definition = re.compile(r"\bfn\s+([a-z_][a-z0-9_]*)")
    seen = defaultdict(int)
    for path in crates.rglob("*.rs"):
        for name in set(definition.findall(path.read_text(encoding="utf-8", errors="replace"))):
            seen[name] += 1
    return seen


def census(crates):
    functions = collect(crates)
    counts, tests = count_calls(crates, functions.keys())
    definitions = all_function_names(crates)
    rows = []
    for name in sorted(functions):
        entry = functions[name]
        by_crate = dict(sorted(counts[name].items()))
        store = by_crate.pop("airk-store", 0)
        rows.append({
            "function": name, "owners": sorted(entry["owners"]),
            "visibility": sorted(entry["visibility"]), "files": sorted(entry["files"]),
            "calls_in_store": store, "calls_outside_store": by_crate,
            "calls_in_tests": tests.get(name, 0),
            # More than one defining file means the textual count may include other types.
            "ambiguous_name": definitions.get(name, 0) > len(entry["files"]),
        })
    return rows


def markdown(rows):
    lines = ["| Function | Owner | Calls outside Store | In Store | In tests | Shared name |",
             "|---|---|---:|---:|---:|---|"]
    for row in rows:
        outside = sum(row["calls_outside_store"].values())
        where = ", ".join(f"{crate} {count}" for crate, count in row["calls_outside_store"].items())
        lines.append("| `{0}` | {1} | {2}{3} | {4} | {5} | {6} |".format(
            row["function"], ", ".join(row["owners"]), outside,
            f" ({where})" if where else "", row["calls_in_store"], row["calls_in_tests"],
            "yes" if row["ambiguous_name"] else ""))
    return "\n".join(lines)


def main(argv=None):
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("--crates", type=Path, required=True, help="the repository's crates folder")
    parser.add_argument("--json", type=Path, help="write the full census here")
    arguments = parser.parse_args(argv)
    rows = census(arguments.crates)
    if arguments.json:
        arguments.json.write_text(json.dumps(rows, indent=2) + "\n", encoding="utf-8")
    print(markdown(rows))
    print(f"\n{len(rows)} functions; "
          f"{sum(1 for row in rows if sum(row['calls_outside_store'].values()))} called outside Store")
    return 0


if __name__ == "__main__":
    sys.exit(main())
