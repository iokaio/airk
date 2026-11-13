# SPDX-License-Identifier: Apache-2.0
"""Widen exactly the items another crate uses after an `extract_crate.py` move.

    python scripts/dev/widen_visibility.py -p airk-host -p airk-cli [--features local-inference] \
        [--target-dir DIR] [--rounds 12]

Runs `cargo check --all-targets --message-format=json` on the packages, collects the privacy
errors (E0603 private item, E0616 private field, E0624 private method) and, at the definition
each error's note points to, replaces `pub(crate)` / `pub(super)` / `pub(in …)` with `pub`, or
adds `pub` to an item that had none. Repeats until no privacy error remains or the round limit
is reached; any other error stops it (fix those by hand). Prints every widened definition.
"""
import argparse
import json
import re
import subprocess
import sys
from pathlib import Path

ROOT = Path(__file__).resolve().parents[2]
PRIVACY = {"E0603", "E0616", "E0624"}
RESTRICTED = re.compile(r"\bpub\((?:crate|super|self|in [^)]*)\)\s+")
ITEM = re.compile(r"^(\s*)((?:(?:async|const|unsafe|extern \"C\")\s+)*)(fn|struct|enum|type|trait|const|static|mod|use|union)\b")
FIELD = re.compile(r"^(\s*)([a-z_][a-z0-9_]*\s*:)")


def check(packages, features, target_dir):
    command = ["cargo", "check", "--locked", "--all-targets", "--message-format=json"]
    for package in packages:
        command += ["-p", package]
    if features:
        command += ["--features", features]
    if target_dir:
        command += ["--target-dir", target_dir]
    result = subprocess.run(command, cwd=ROOT, text=True, encoding="utf-8", stdout=subprocess.PIPE,
                            stderr=subprocess.DEVNULL)
    privacy, other = [], []
    for line in result.stdout.splitlines():
        try:
            event = json.loads(line)
        except ValueError:
            continue
        message = event.get("message") if event.get("reason") == "compiler-message" else None
        if not message or message.get("level") != "error":
            continue
        code = (message.get("code") or {}).get("code")
        if code in PRIVACY:
            for child in message.get("children", []):
                for span in child.get("spans", []):
                    if "defined here" in child.get("message", "") or span.get("label"):
                        privacy.append((span["file_name"], span["line_start"], message["message"]))
            for span in message.get("spans", []):
                if span.get("label") and "private" in span["label"] and not message.get("children"):
                    privacy.append((span["file_name"], span["line_start"], message["message"]))
        else:
            other.append(message.get("rendered") or message["message"])
    return privacy, other


def widen(path, number):
    """Make the definition at `number` (1-based) public; the text it changed, or None."""
    file = Path(path) if Path(path).is_absolute() else ROOT / path
    lines = file.read_text(encoding="utf-8").split("\n")
    index = number - 1
    line = lines[index]
    if RESTRICTED.search(line):
        lines[index] = RESTRICTED.sub("pub ", line, count=1)
    elif re.match(r"^\s*pub\s", line):
        return None
    else:
        item = ITEM.match(line)
        field = FIELD.match(line)
        if item:
            lines[index] = f"{item.group(1)}pub {line[len(item.group(1)):]}"
        elif field:
            lines[index] = f"{field.group(1)}pub {line[len(field.group(1)):]}"
        else:
            return None
    file.write_text("\n".join(lines), encoding="utf-8", newline="")
    return f"{file.relative_to(ROOT)}:{number}: {lines[index].strip()}"


def main(argv=None):
    parser = argparse.ArgumentParser(description=__doc__.splitlines()[0])
    parser.add_argument("-p", "--package", action="append", required=True)
    parser.add_argument("--features")
    parser.add_argument("--target-dir")
    parser.add_argument("--rounds", type=int, default=12)
    args = parser.parse_args(argv)
    for round_number in range(1, args.rounds + 1):
        privacy, other = check(args.package, args.features, args.target_dir)
        if not privacy:
            if other:
                print("\n".join(other[:40]), file=sys.stderr)
                print(f"round {round_number}: no privacy errors; {len(other)} other errors remain", file=sys.stderr)
                return 1
            print(f"round {round_number}: clean")
            return 0
        changed = [widen(path, line) for path, line, _ in sorted(set(privacy))]
        changed = [entry for entry in changed if entry]
        for entry in changed:
            print(f"widened {entry}")
        if not changed:
            for path, line, message in sorted(set(privacy))[:40]:
                print(f"unresolved {path}:{line}: {message}", file=sys.stderr)
            return 1
    print("round limit reached", file=sys.stderr)
    return 1


if __name__ == "__main__":
    sys.exit(main())
