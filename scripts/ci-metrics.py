# SPDX-License-Identifier: Apache-2.0
"""Record command durations and summarize nextest JUnit timings without test payloads."""
import argparse
import json
import os
from pathlib import Path
import subprocess
import time
import xml.etree.ElementTree as ET


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("command", choices=("run", "summary"))
    parser.add_argument("--name")
    parser.add_argument("--file", type=Path, default=Path("artifacts/ci/timings.jsonl"))
    parser.add_argument("--junit", type=Path, default=Path("target/nextest/ci/junit.xml"))
    args, command = parser.parse_known_args()
    if args.command == "run":
        if command[:1] == ["--"]:
            command = command[1:]
        if not command or not args.name:
            parser.error("run requires --name and a command after --")
        started = time.monotonic()
        code = 1
        try:
            code = subprocess.call(command)
        finally:
            args.file.parent.mkdir(parents=True, exist_ok=True)
            with args.file.open("a", encoding="utf-8") as output:
                output.write(json.dumps(dict(name=args.name, seconds=round(time.monotonic() - started, 3), exit_code=code)) + "\n")
        raise SystemExit(code)
    lines = ["## CI measurements", "", "| Phase | Seconds | Exit |", "|---|---:|---:|"]
    if args.file.exists():
        for line in args.file.read_text(encoding="utf-8").splitlines():
            row = json.loads(line)
            lines.append(f'| {row["name"]} | {row["seconds"]:.1f} | {row["exit_code"]} |')
    if args.junit.exists():
        tests = sorted(ET.parse(args.junit).iter("testcase"), key=lambda t: float(t.get("time", 0)), reverse=True)
        lines += ["", "### Slowest tests", "", "| Test | Seconds |", "|---|---:|"]
        for test in tests[:20]:
            name = (test.get("classname", "") + "::" + test.get("name", "")).replace("|", "\\|")
            lines.append(f'| {name} | {float(test.get("time", 0)):.1f} |')
    value = "\n".join(lines) + "\n"
    print(value)
    if summary := os.environ.get("GITHUB_STEP_SUMMARY"):
        with open(summary, "a", encoding="utf-8") as output:
            output.write(value)


if __name__ == "__main__":
    main()
