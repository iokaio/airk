# SPDX-License-Identifier: Apache-2.0
"""S7.4 mechanical step: point Core production code at its two new leaves.

Step S7.4 moved two groups of items verbatim to new leaf modules of `airk_core`:

- the shared service vocabulary (`ProjectBinding`,
  `MAX_PROJECT_REGISTRATION_BYTES`, `ProjectRegistration`, `RegisteredProject`,
  `JobKind`, `JobState`, `RequestKey`, `JobOutcome`, `JobRecord`,
  `WorkerIdentity`, `MaintenanceObservation`) to `service_records`;
- the verification byte, deadline and source limits and the workbench
  step-report limit to `read_limits`.

`service`, `verification` and `workbench` re-export every moved item, so other
crates, tests and the wire are unchanged. This rewrites only Core production
paths, so those modules stop depending on `service`, `verification` or
`workbench` for these items:

- `service::<moved>` and `crate::service::<moved>`;
- `service::{...}` groups whose every entry moved (mixed groups are left alone);
- `crate::verification::MAX_{SOURCES,BYTES,DEADLINE_MS}`;
- `crate::workbench::MAX_STEP_REPORTS`.

Test files and inline `#[cfg(test)] mod ... { }` blocks are not touched; they
keep using the re-exported paths. The rewrite is idempotent. `--check` changes
nothing and exits 1 if any old production path remains or a moved definition is
missing from its leaf.
"""
import argparse
from pathlib import Path
import re
import sys

ROOT = Path(__file__).resolve().parents[2]
SRC = ROOT / "crates" / "airk-core" / "src"
RECORDS = (
    "ProjectBinding", "MAX_PROJECT_REGISTRATION_BYTES", "ProjectRegistration", "RegisteredProject",
    "JobKind", "JobState", "RequestKey", "JobOutcome", "JobRecord", "WorkerIdentity",
    "MaintenanceObservation",
)
LIMITS = {"verification": ("MAX_SOURCES", "MAX_BYTES", "MAX_DEADLINE_MS"), "workbench": ("MAX_STEP_REPORTS",)}
LEAVES = {"service_records": RECORDS, "read_limits": LIMITS["verification"] + LIMITS["workbench"]}
SKIP = {"service.rs", "service_records/mod.rs", "read_limits/mod.rs"}

SINGLE = re.compile(r"(?<![\w:])((?:crate::)?)service::(" + "|".join(RECORDS) + r")\b")
GROUP = re.compile(r"(?<![\w:])((?:crate::)?)service::\{([^{}]*)\}")
LIMIT = [
    (re.compile(r"\bcrate::" + module + r"::(" + "|".join(names) + r")\b"), r"crate::read_limits::\1")
    for module, names in LIMITS.items()
]
TEST_BLOCK = re.compile(r"^#\[cfg\(test\)\]\s*\n(?:#\[[^\n]*\]\s*\n)*(?:pub(?:\([^)]*\))? )?mod \w+ \{", re.M)


def is_test_file(relative):
    parts = relative.split("/")
    return parts[-1] == "tests.rs" or parts[-1].endswith("_tests.rs") or "tests" in parts[:-1]


def production_spans(text):
    """(start, end) spans outside inline test modules, found by brace matching."""
    spans, position = [], 0
    for match in TEST_BLOCK.finditer(text):
        if match.start() < position:
            continue
        depth, index = 0, match.end() - 1
        while index < len(text):
            if text[index] == "{":
                depth += 1
            elif text[index] == "}":
                depth -= 1
                if depth == 0:
                    break
            index += 1
        spans.append((position, match.start()))
        position = index + 1
    spans.append((position, len(text)))
    return spans


def moved_group(match):
    names = [name.strip() for name in match.group(2).split(",") if name.strip()]
    return bool(names) and all(name in RECORDS for name in names)


def group(match):
    if moved_group(match):
        return f"{match.group(1)}service_records::{{{match.group(2)}}}"
    return match.group(0)


def rewrite_span(text):
    count = 0
    for pattern, replacement in [(SINGLE, r"\1service_records::\2")] + LIMIT:
        count += len(pattern.findall(text))
        text = pattern.sub(replacement, text)
    count += sum(1 for match in GROUP.finditer(text) if moved_group(match))
    return GROUP.sub(group, text), count


def rewrite(text):
    pieces, count, last = [], 0, 0
    for start, end in production_spans(text):
        pieces.append(text[last:start])
        new, found = rewrite_span(text[start:end])
        pieces.append(new)
        count += found
        last = end
    pieces.append(text[last:])
    return "".join(pieces), count


def definitions_ok():
    problems = []
    for leaf, names in LEAVES.items():
        path = SRC / leaf / "mod.rs"
        text = path.read_text(encoding="utf-8") if path.is_file() else ""
        for name in names:
            if not re.search(r"\bpub (?:struct|enum|type|const|fn) " + name + r"\b", text):
                problems.append(f"{leaf} is missing {name}")
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
        if relative in SKIP or is_test_file(relative):
            continue
        text = path.read_text(encoding="utf-8")
        new, count = rewrite(text)
        if not count:
            continue
        changed += count
        print(f"{path.relative_to(ROOT).as_posix()}: {count}")
        if not args.check:
            path.write_text(new, encoding="utf-8", newline="\n")
    print(f"{'remaining' if args.check else 'rewritten'} production paths: {changed}")
    valid = definitions_ok()
    return 1 if (args.check and changed) or not valid else 0


if __name__ == "__main__":
    sys.exit(main())
