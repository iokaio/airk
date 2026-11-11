# SPDX-License-Identifier: Apache-2.0
"""Capture and compare golden CLI outputs (exit status, stdout, stderr).

Promoted from the git-ignored one-process harness (split plan U71). Cases come
from a reviewed JSON file. Each case runs twice, without and with
AIRK_ERROR_FORMAT=json, against freshly created owner-only fixture folders, so
no case can reach the user's real installation. Output is normalized before it
is stored: the fixture base, the binary path, the home folder and the user name
become placeholders, so committed goldens carry no machine-specific paths.

    python scripts/dev/cli_golden.py capture --binary target/debug/airk.exe \
        --cases fixtures/cli-golden/cases.json --out fixtures/cli-golden/windows/airk.json
    python scripts/dev/cli_golden.py compare expected.json actual.json [--replace airk=airkctl]

Argument placeholders: {fixture}, {install_empty}, {control}, {src}, {data},
{missing}, {measurement}, {id}, {sha}.
"""
import argparse
import base64
import getpass
import json
import os
from pathlib import Path
import re
import shutil
import subprocess
import sys
import tempfile

CASES_SCHEMA = "airk-golden-cases/1"
GOLDEN_SCHEMA = "airk-golden-cli/2"
ID = "01" * 16
SHA = "b" * 64
OWNED = ("install-empty", "control")
FOLDERS = ("install-empty", "control", "src", "data")


def placeholders(base):
    return {"fixture": str(base), "install_empty": str(base / "install-empty"),
            "control": str(base / "control"), "src": str(base / "src"), "data": str(base / "data"),
            "missing": str(base / "missing"), "measurement": str(base / "measurement.json"),
            "id": ID, "sha": SHA}


def fresh(base):
    """Recreate the fixture folders; the control folders are owner-only."""
    if base.exists():
        shutil.rmtree(base)
    for leaf in FOLDERS:
        (base / leaf).mkdir(parents=True)
    (base / "measurement.json").write_text("{}", encoding="utf-8")
    for leaf in OWNED:
        folder = base / leaf
        if sys.platform == "win32":
            user = os.environ.get("USERDOMAIN", "") + "\\" + os.environ["USERNAME"]
            subprocess.run(["icacls", str(folder), "/inheritance:r", "/grant:r", user + ":(OI)(CI)F"],
                           check=True, capture_output=True)
        else:
            folder.chmod(0o700)


def path_forms(value):
    """Spellings of a path that may appear in text or JSON output."""
    text = str(value)
    forms = {text, text.replace("\\", "/"), text.replace("\\", "\\\\")}
    if sys.platform == "win32":
        forms |= {form.lower() for form in list(forms)}
    return sorted((form for form in forms if form), key=len, reverse=True)


def replacements(base, binary):
    pairs = [(form, "<BINARY>") for form in path_forms(binary)]
    pairs += [(form, "<FIXTURE>") for form in path_forms(base)]
    pairs += [(form, "<HOME>") for form in path_forms(Path.home())]
    user = getpass.getuser()
    if len(user) >= 3:
        pairs.append((user, "<USER>"))
    return pairs


def normalize(data, pairs, rules):
    """Decode and normalize; return (encoding, text)."""
    try:
        text = data.decode("utf-8")
    except UnicodeDecodeError:
        return "base64", base64.b64encode(data).decode()
    for old, new in pairs:
        text = text.replace(old, new)
    for rule in rules:
        text = re.sub(rule["pattern"], rule["replacement"], text)
    return "utf-8", text


def load_cases(path):
    document = json.loads(Path(path).read_text(encoding="utf-8"))
    if document.get("schema") != CASES_SCHEMA:
        raise ValueError("expected " + CASES_SCHEMA)
    ids = [case["id"] for case in document["cases"]]
    if len(ids) != len(set(ids)) or not ids:
        raise ValueError("case ids must be unique and non-empty")
    return document


def run_case(binary, base, case, error_format, timeout):
    env = {key: value for key, value in os.environ.items() if not key.startswith("AIRK_")}
    if error_format:
        env["AIRK_ERROR_FORMAT"] = "json"
    values = placeholders(base)
    argv = [part.format(**values) for part in case["argv"]]
    stdin = case.get("stdin")
    done = subprocess.run([str(binary), *argv], input=(stdin or "").encode(), capture_output=True,
                          cwd=base, env=env, timeout=timeout)
    return done.returncode, done.stdout, done.stderr


def capture(binary, cases_path, out, fixture_base=None, timeout=120):
    binary = Path(binary).resolve(strict=True)
    document = load_cases(cases_path)
    rules = document.get("normalize", [])
    owner = tempfile.TemporaryDirectory(prefix="airk-golden-") if fixture_base is None else None
    base = Path(owner.name if owner else fixture_base).resolve() / "fixture"
    pairs = replacements(base, binary)
    results = []
    try:
        for case in document["cases"]:
            for error_format in (False, True):
                fresh(base)
                code, stdout, stderr = run_case(binary, base, case, error_format, timeout)
                out_encoding, out_text = normalize(stdout, pairs, rules)
                err_encoding, err_text = normalize(stderr, pairs, rules)
                results.append({"id": case["id"], "argv": case["argv"], "stdin": case.get("stdin"),
                                "error_format": error_format, "exit": code,
                                "stdout_encoding": out_encoding, "stdout": out_text,
                                "stderr_encoding": err_encoding, "stderr": err_text})
    finally:
        if base.exists():
            shutil.rmtree(base)
        if owner:
            owner.cleanup()
    golden = {"schema": GOLDEN_SCHEMA, "program": binary.stem, "platform": sys.platform,
              "cases": results}
    Path(out).parent.mkdir(parents=True, exist_ok=True)
    Path(out).write_text(json.dumps(golden, indent=1, ensure_ascii=False) + "\n", encoding="utf-8",
                         newline="\n")
    return golden


def apply(text, swaps):
    for old, new in swaps:
        text = text.replace(old, new)
    return text


def compare(expected_path, actual_path, swaps=()):
    """Return a list of differences; empty means equal."""
    expected = json.loads(Path(expected_path).read_text(encoding="utf-8"))
    actual = json.loads(Path(actual_path).read_text(encoding="utf-8"))
    for document in (expected, actual):
        if document.get("schema") != GOLDEN_SCHEMA:
            raise ValueError("expected " + GOLDEN_SCHEMA)
    key = lambda case: (case["id"], case["error_format"])
    left = {key(case): case for case in expected["cases"]}
    right = {key(case): case for case in actual["cases"]}
    problems = [f"missing case {k}" for k in sorted(left.keys() - right.keys())]
    problems += [f"unexpected case {k}" for k in sorted(right.keys() - left.keys())]
    for k in sorted(left.keys() & right.keys()):
        for field in ("exit", "stdout_encoding", "stdout", "stderr_encoding", "stderr"):
            want = left[k][field]
            if isinstance(want, str) and field in ("stdout", "stderr"):
                want = apply(want, swaps)
            if want != right[k][field]:
                problems.append(f"{k} {field} differs")
    return problems


def parse_swap(value):
    old, separator, new = value.partition("=")
    if not separator or not old:
        raise argparse.ArgumentTypeError("expected OLD=NEW")
    return old, new


def main(argv=None):
    parser = argparse.ArgumentParser(description=__doc__.splitlines()[0])
    commands = parser.add_subparsers(dest="command", required=True)
    take = commands.add_parser("capture")
    take.add_argument("--binary", required=True, type=Path)
    take.add_argument("--cases", required=True, type=Path)
    take.add_argument("--out", required=True, type=Path)
    take.add_argument("--fixture-base", type=Path)
    take.add_argument("--timeout", type=int, default=120)
    check = commands.add_parser("compare")
    check.add_argument("expected", type=Path)
    check.add_argument("actual", type=Path)
    check.add_argument("--replace", action="append", type=parse_swap, default=[],
                       help="OLD=NEW applied to the expected side (e.g. a renamed program)")
    args = parser.parse_args(argv)
    if args.command == "capture":
        golden = capture(args.binary, args.cases, args.out, args.fixture_base, args.timeout)
        print(json.dumps({"cases": len(golden["cases"]),
                          "exits": sorted({case["exit"] for case in golden["cases"]})}))
        return 0
    problems = compare(args.expected, args.actual, args.replace)
    for problem in problems:
        print(problem)
    print(json.dumps({"equal": not problems, "differences": len(problems)}))
    return 1 if problems else 0


if __name__ == "__main__":
    raise SystemExit(main())
