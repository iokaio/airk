# SPDX-License-Identifier: Apache-2.0
"""Record and compare Rust test identities and outcomes per test binary.

Structural steps compare selected test identities and outcomes, never counts
(docs/simplify-plan.md section 3.10). This helper builds the selected test
binaries once, lists every test in each, optionally runs them, and writes a
JSON record keyed by `<package>::<kind>/<target>`. `compare` reports added,
removed and changed identities between two records.

    python scripts/dev/test_identities.py record --output before.json -- \
        --workspace --no-default-features --lib --bins --tests
    python scripts/dev/test_identities.py record --run --output lib.json -- -p airk-store --lib
    python scripts/dev/test_identities.py compare before.json after.json
"""
import argparse
import json
import os
from pathlib import Path
import re
import subprocess
import sys

ROOT = Path(__file__).resolve().parents[2]
SCHEMA = "airk-test-identities/1"
RESULT = re.compile(r"^test (.+?) \.\.\. (ok|FAILED|ignored)(?:,.*)?$")


def package_name(package_id):
    # "path+file:///.../crates/airk-store#airk-store@1.8.16" or "...#1.8.16"
    fragment = package_id.rsplit("#", 1)[-1]
    if "@" in fragment:
        return fragment.split("@", 1)[0]
    return package_id.rstrip("/").rsplit("/", 1)[-1].split("#", 1)[0]


def build(cargo, arguments):
    command = [cargo, "test", "--no-run", "--message-format=json-render-diagnostics", *arguments]
    print("+ " + " ".join(command), file=sys.stderr, flush=True)
    process = subprocess.run(command, cwd=ROOT, stdout=subprocess.PIPE, text=True, encoding="utf-8", errors="replace")
    if process.returncode:
        raise SystemExit(f"cargo test --no-run failed with exit code {process.returncode}")
    binaries = {}
    for line in process.stdout.splitlines():
        if not line.startswith("{"):
            continue
        message = json.loads(line)
        if message.get("reason") != "compiler-artifact" or not message.get("executable"):
            continue
        if not message.get("profile", {}).get("test"):
            continue
        target = message["target"]
        kind = target["kind"][0]
        identity = f"{package_name(message['package_id'])}::{kind}/{target['name']}"
        binaries[identity] = {
            "executable": message["executable"],
            "directory": str(Path(target["src_path"]).resolve().parent),
            "manifest": message.get("manifest_path"),
        }
    return binaries


def list_tests(binary):
    directory = Path(binary["manifest"]).parent if binary.get("manifest") else Path(binary["directory"])
    output = subprocess.run(
        [binary["executable"], "--list", "--format", "terse"], cwd=directory,
        capture_output=True, text=True, encoding="utf-8", errors="replace", check=True,
    ).stdout
    return sorted(line[: -len(": test")] for line in output.splitlines() if line.endswith(": test"))


def run_tests(binary, threads):
    directory = Path(binary["manifest"]).parent if binary.get("manifest") else Path(binary["directory"])
    command = [binary["executable"]]
    if threads:
        command.append(f"--test-threads={threads}")
    process = subprocess.run(command, cwd=directory, capture_output=True, text=True, encoding="utf-8", errors="replace")
    outcomes = {}
    for line in process.stdout.splitlines():
        match = RESULT.match(line.strip())
        if match:
            outcomes[match.group(1)] = match.group(2)
    return outcomes, process.returncode, process.stdout[-4000:] + process.stderr[-4000:]


def record(args):
    binaries = build(args.cargo, args.cargo_arguments)
    result = {"schema": SCHEMA, "cargo_arguments": args.cargo_arguments, "binaries": {}}
    failures = 0
    for identity in sorted(binaries):
        binary = binaries[identity]
        tests = list_tests(binary)
        entry = {"tests": {name: "listed" for name in tests}}
        if args.run and tests:
            outcomes, code, tail = run_tests(binary, args.test_threads)
            for name in tests:
                entry["tests"][name] = outcomes.get(name, "not_reported")
            entry["exit_code"] = code
            if code:
                failures += 1
                entry["output_tail"] = tail
        result["binaries"][identity] = entry
        selected = sum(1 for _ in tests)
        print(f"{identity}: {selected} selected", file=sys.stderr, flush=True)
    total = sum(len(entry["tests"]) for entry in result["binaries"].values())
    result["selected_total"] = total
    Path(args.output).write_text(json.dumps(result, indent=1, sort_keys=True) + "\n", encoding="utf-8")
    print(f"{len(result['binaries'])} binaries, {total} selected tests -> {args.output}", file=sys.stderr)
    if total == 0:
        print("zero selected tests is a failure", file=sys.stderr)
        return 1
    return 1 if failures else 0


def flatten(path):
    data = json.loads(Path(path).read_text(encoding="utf-8"))
    return {(binary, name): outcome for binary, entry in data["binaries"].items() for name, outcome in entry["tests"].items()}


def compare(args):
    before, after = flatten(args.before), flatten(args.after)
    removed = sorted(set(before) - set(after))
    added = sorted(set(after) - set(before))
    changed = sorted(key for key in set(before) & set(after) if "listed" not in (before[key], after[key]) and before[key] != after[key])
    for label, keys in (("removed", removed), ("added", added)):
        print(f"{label}: {len(keys)}")
        for binary, name in keys[: args.limit]:
            print(f"  {binary} {name}")
    print(f"outcome changed: {len(changed)}")
    for key in changed[: args.limit]:
        print(f"  {key[0]} {key[1]}: {before[key]} -> {after[key]}")
    if args.allow_removed_prefix:
        unexpected = [key for key in removed if not any(f"{key[0]} {key[1]}".startswith(prefix) for prefix in args.allow_removed_prefix)]
        print(f"removed outside the declared prefixes: {len(unexpected)}")
        for binary, name in unexpected[: args.limit]:
            print(f"  {binary} {name}")
        return 1 if unexpected or added or changed else 0
    return 1 if removed or added or changed else 0


def main(argv=None):
    parser = argparse.ArgumentParser(description=__doc__.splitlines()[0])
    sub = parser.add_subparsers(dest="command", required=True)
    rec = sub.add_parser("record")
    rec.add_argument("--output", required=True)
    rec.add_argument("--run", action="store_true", help="also run each binary and record outcomes")
    rec.add_argument("--test-threads", type=int)
    rec.add_argument("--cargo", default=os.environ.get("CARGO", "cargo"))
    rec.add_argument("cargo_arguments", nargs=argparse.REMAINDER)
    cmp = sub.add_parser("compare")
    cmp.add_argument("before")
    cmp.add_argument("after")
    cmp.add_argument("--limit", type=int, default=200)
    cmp.add_argument("--allow-removed-prefix", action="append",
                     help="'<binary> <test prefix>' removals declared by a deletion step")
    args = parser.parse_args(argv)
    if args.command == "record":
        if args.cargo_arguments and args.cargo_arguments[0] == "--":
            args.cargo_arguments = args.cargo_arguments[1:]
        return record(args)
    return compare(args)


if __name__ == "__main__":
    raise SystemExit(main())
