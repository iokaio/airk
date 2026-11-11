# SPDX-License-Identifier: Apache-2.0
"""Port the evidence-read arguments of `airk` to `airkctl` (split step 3a.2c).

`airkctl` owns the CLI surface after Phase 4, and it may not link the `airk`
binary. This script copies, verbatim, the Clap definitions of the fifteen
evidence reads, their flattened option structs, `SearchMode`, and the
`query()` mapping with its helpers from `crates/airk-cli/src/main.rs` into
`crates/airkctl/src/evidence.rs`. The only edits are listed in EDITS and are
asserted to apply exactly once, so a reviewer can re-run the script and
compare. The module is declared `#[rustfmt::skip]` so its bytes stay the
script's output; `--check` needs no Rust toolchain. The old CLI keeps its copy
until Phase 4 removes it.

    python scripts/dev/split_3a2c_port_evidence.py          # write the port
    python scripts/dev/split_3a2c_port_evidence.py --check  # exit 1 on drift
"""
import argparse
from pathlib import Path
import re
import sys

ROOT = Path(__file__).resolve().parents[2]
SOURCE = ROOT / "crates" / "airk-cli" / "src" / "main.rs"
OUTPUT = ROOT / "crates" / "airkctl" / "src" / "evidence.rs"

STRUCTS = ["EvidenceOptions", "ReplyOptions", "VerificationOptions", "SearchFilters",
           "NameMatchOptions", "ReferenceOptions"]
VARIANTS = ["Parse", "Changes", "CodeFacts", "Relationships", "Status", "Search", "Files", "Read",
            "Info", "Outline", "Symbol", "Imports", "Facts", "Refs", "Context"]
FUNCTIONS = ["evidence", "query_options", "query"]

HEADER = """// SPDX-License-Identifier: Apache-2.0
//! The evidence reads of `airkctl` (split step 3a.2c): Clap arguments and the
//! argument-to-tool mapping, ported verbatim from the pre-split `airk` by
//! `scripts/dev/split_3a2c_port_evidence.py` (unformatted: `#[rustfmt::skip]`).
//! Do not edit by hand; edit the script's inputs or its reviewed EDITS and
//! re-run it. Each command becomes one M3 `ToolsCall` with the returned tool
//! name and arguments.
use anyhow::{Context, Result, ensure};
use clap::{Subcommand, ValueEnum};
use serde_json::{Value, json};
use std::{
    io::Read,
    path::{Path, PathBuf},
};

"""

# (old, new, count) edits applied to the assembled text, each asserted.
EDITS = [
    ("fn query(command: Command) -> Result<(&'static str, Value)> {",
     "/// The tool and arguments one evidence read sends.\n"
     "pub fn query(command: EvidenceCommand) -> Result<(&'static str, Value)> {", 1),
    ("Command::", "EvidenceCommand::", None),
    # Every variant of this enum is a read, so the catch-all arms of the
    # copied mapping are unreachable here.
    ("        _ => None,\n    };\n    let verification", "    };\n    let verification", 1),
    ("        _ => anyhow::bail!(\"not a read-only tool alias\"),\n", "", 1),
    ("#[derive(Clone, Copy, ValueEnum)]\nenum SearchMode {",
     "#[derive(Clone, Copy, ValueEnum)]\npub enum SearchMode {", 1),
] + [
    # The public command enum's fields name these flattened option types.
    (f"\nstruct {name} {{", f"\npub struct {name} {{", 1) for name in STRUCTS
]


def block_end(text, start):
    """Index just past the brace block that opens at or after `start`."""
    depth = 0
    index = text.index("{", start)
    in_string = False
    while index < len(text):
        char = text[index]
        if in_string:
            if char == "\\":
                index += 1
            elif char == '"':
                in_string = False
        elif char == '"':
            in_string = True
        elif char == "{":
            depth += 1
        elif char == "}":
            depth -= 1
            if depth == 0:
                return index + 1
        index += 1
    raise ValueError("unbalanced block")


def with_leading(text, start, indent):
    """Extend `start` back over the doc comments and attributes above it."""
    lines = text[:start].split("\n")
    lines.pop()  # the partial line holding the item itself
    taken = 0
    for line in reversed(lines):
        stripped = line.strip()
        if line.startswith(indent) and (stripped.startswith("///") or stripped.startswith("#[")
                                        or (stripped.startswith(("help", "long", "value", "requires",
                                                                 "conflicts", "num_args", "default",
                                                                 "global", "hide", "after"))
                                            and taken)):
            taken += 1
        else:
            break
    begin = start - len(text[:start].split("\n")[-1])
    for line in lines[len(lines) - taken:]:
        begin -= len(line) + 1
    return begin


def item(text, pattern, indent=""):
    match = re.search(pattern, text, re.MULTILINE)
    if not match:
        raise ValueError("missing item: " + pattern)
    begin = with_leading(text, match.start(), indent)
    end = block_end(text, match.start())
    if text[end:end + 1] == ",":
        end += 1
    return text[begin:end]


def render(source):
    parts = [HEADER]
    for name in STRUCTS:
        parts.append(item(source, rf"^struct {name} \{{") + "\n\n")
    parts.append(item(source, r"^enum SearchMode \{") + "\n\n")
    command_start = source.index("enum Command {")
    command_end = block_end(source, command_start)
    commands = source[command_start:command_end]
    variants = []
    for name in VARIANTS:
        variants.append(item(commands, rf"^    {name} \{{", "    "))
    parts.append("/// The fifteen evidence reads, as `airk` declared them.\n"
                 "#[derive(Subcommand)]\npub enum EvidenceCommand {\n"
                 + "\n".join(variants) + "\n}\n\n")
    for name in FUNCTIONS:
        parts.append(item(source, rf"^fn {name}\(") + "\n")
    text = "".join(parts)
    for old, new, count in EDITS:
        found = text.count(old)
        if count is None:
            if not found:
                raise ValueError("edit did not apply: " + old)
        elif found != count:
            raise ValueError(f"edit applies {found} times, expected {count}: {old}")
        text = text.replace(old, new)
    return text


def main(argv=None):
    parser = argparse.ArgumentParser(description=__doc__.splitlines()[0])
    parser.add_argument("--check", action="store_true")
    args = parser.parse_args(argv)
    text = render(SOURCE.read_text(encoding="utf-8"))
    if args.check:
        current = OUTPUT.read_text(encoding="utf-8") if OUTPUT.exists() else ""
        if current != text:
            print("crates/airkctl/src/evidence.rs differs from the port; run the script", file=sys.stderr)
            return 1
        return 0
    OUTPUT.write_bytes(text.encode("utf-8"))
    return 0


if __name__ == "__main__":
    raise SystemExit(main())
