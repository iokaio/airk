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


REEXPORT = {"E0364", "E0365"}
DEFINITION = r"^\s*pub\((?:crate|super|self|in [^)]*)\)\s+(?:(?:async|const|unsafe)\s+)*(?:fn|struct|enum|type|trait|const|static|mod|union)\s+{}\b"


def definitions(file_name, name):
    """(file, line) of each restricted-visibility definition of `name` in the crate of `file_name`."""
    path = Path(file_name) if Path(file_name).is_absolute() else ROOT / file_name
    source = next(parent for parent in path.parents if parent.name == "src")
    pattern = re.compile(DEFINITION.format(re.escape(name)))
    found = []
    for candidate in sorted(source.rglob("*.rs")):
        for number, line in enumerate(candidate.read_text(encoding="utf-8").split("\n"), start=1):
            if pattern.match(line):
                found.append((str(candidate), number))
    return found


def field_definitions(struct, field):
    """(file, line) of field `field` inside `struct <struct>` definitions in the workspace crates."""
    header = re.compile(r"^\s*(?:pub(?:\([^)]*\))?\s+)?struct\s+" + re.escape(struct) + r"\b")
    member = re.compile(r"^\s*(?:pub\((?:crate|super|self|in [^)]*)\)\s+)?" + re.escape(field) + r"\s*:")
    found = []
    for source in sorted((ROOT / "crates").glob("*/src")):
        for candidate in sorted(source.rglob("*.rs")):
            lines = candidate.read_text(encoding="utf-8").split("\n")
            for start, line in enumerate(lines):
                if not header.match(line):
                    continue
                depth = 0
                for number in range(start, len(lines)):
                    depth += lines[number].count("{") - lines[number].count("}")
                    if number > start and member.match(lines[number]) and not lines[number].lstrip().startswith("pub "):
                        found.append((str(candidate), number + 1))
                    if number > start and depth <= 0:
                        break
    return found


def check(packages, features, target_dir):
    command = ["cargo", "check", "--locked", "--all-targets", "--message-format=json"]
    for package in packages:
        command += ["-p", package]
    if features:
        command += ["--features", features]
    if target_dir:
        command += ["--target-dir", target_dir]
    result = subprocess.run(command, cwd=ROOT, text=True, encoding="utf-8", stdout=subprocess.PIPE,
                            stderr=subprocess.PIPE)
    privacy, other = [], []
    if result.returncode != 0 and '"reason":"compiler-message"' not in result.stdout:
        # Cargo stopped before compiling (for example a stale lockfile): never report "clean".
        other.append(result.stderr.strip()[-4000:] or f"cargo exited {result.returncode}")
    for line in result.stdout.splitlines():
        try:
            event = json.loads(line)
        except ValueError:
            continue
        message = event.get("message") if event.get("reason") == "compiler-message" else None
        if not message or message.get("level") != "error":
            continue
        code = (message.get("code") or {}).get("code")
        if code in REEXPORT:
            # `X` is only public within the crate: widen X's definition in the re-exporting crate.
            name = re.search(r"`([A-Za-z0-9_]+)`", message["message"])
            span = next(iter(message.get("spans", [])), None)
            if name and span:
                for found in definitions(span["file_name"], name.group(1)):
                    privacy.append(found + (message["message"],))
                continue
        if code == "E0451":
            # fields `a`, `b` and `c` of struct `S` are private (a struct literal in another crate).
            names = re.findall(r"`([A-Za-z0-9_:<>, ]+)`", message["message"])
            if len(names) >= 2:
                struct = re.sub(r"<.*", "", names[-1]).split("::")[-1]
                for field in names[:-1]:
                    for found in field_definitions(struct, field):
                        privacy.append(found + (message["message"],))
            continue
        if code == "E0616":
            # field `x` of struct `S` is private: widen the field inside `struct S`'s definition.
            names = re.findall(r"`([A-Za-z0-9_:<>, ]+)`", message["message"])
            if len(names) >= 2:
                struct = re.sub(r"<.*", "", names[1]).split("::")[-1]
                for found in field_definitions(struct, names[0]):
                    privacy.append(found + (message["message"],))
            continue
        if code in PRIVACY:
            # Only the note that points at the definition; never the use site.
            for child in message.get("children", []):
                if "defined here" not in child.get("message", ""):
                    continue
                for span in child.get("spans", []):
                    privacy.append((span["file_name"], span["line_start"], message["message"]))
            # rustc also labels the definition on a secondary span ("private method defined here").
            for span in message.get("spans", []):
                if not span.get("is_primary") and "defined here" in (span.get("label") or ""):
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
