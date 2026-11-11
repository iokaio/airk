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
    python scripts/dev/cli_golden.py differential --old airk.exe --new airkctl.exe         --cases fixtures/cli-golden/differential.json [--out report.json]

`differential` (split plan P0.3b) runs every case with both binaries against the
same fixture folders and, unless the case file sets "service": false, one live
fixture service started with the old binary's `serve`; it compares exit status,
stdout and stderr after normalization and the case file's reviewed renames.

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
import time

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


DIFFERENTIAL_SCHEMA = "airk-differential-cases/1"


def start_service(binary, base, timeout):
    """Start `serve` on the fixture control root and wait for observed health."""
    control = str(base / "control")
    env = {key: value for key, value in os.environ.items() if not key.startswith("AIRK_")}
    child = subprocess.Popen([str(binary), "serve", "--runtime-dir", control], cwd=base, env=env,
                             stdin=subprocess.DEVNULL, stdout=subprocess.DEVNULL, stderr=subprocess.PIPE)
    deadline = time.monotonic() + timeout
    while time.monotonic() < deadline:
        if child.poll() is not None:
            raise RuntimeError("fixture service exited: " + child.stderr.read().decode(errors="replace")[-500:])
        health = subprocess.run([str(binary), "service", "health", "--runtime-dir", control], cwd=base, env=env,
                                capture_output=True, timeout=30)
        if health.returncode == 0:
            return child
        time.sleep(0.25)
    child.kill()
    child.wait()
    raise RuntimeError("fixture service did not report health before the deadline")


def stop_service(binary, base, child):
    env = {key: value for key, value in os.environ.items() if not key.startswith("AIRK_")}
    subprocess.run([str(binary), "service", "shutdown", "--runtime-dir", str(base / "control")], cwd=base,
                   env=env, capture_output=True, timeout=60)
    try:
        child.wait(timeout=60)
    except subprocess.TimeoutExpired:
        child.kill()
        child.wait()


def differential(old, new, cases_path, fixture_base=None, timeout=120):
    """Run each case with the old and the new binary against one fixture.

    Both sides see the same owner-only fixture folders and, unless the case file
    sets `"service": false`, the same live fixture service started by the old
    binary. Each side is normalized with its own binary path; the reviewed
    `replace` pairs then map old program names to new ones before comparison.
    """
    old, new = Path(old).resolve(strict=True), Path(new).resolve(strict=True)
    document = json.loads(Path(cases_path).read_text(encoding="utf-8"))
    if document.get("schema") != DIFFERENTIAL_SCHEMA:
        raise ValueError("expected " + DIFFERENTIAL_SCHEMA)
    ids = [case["id"] for case in document["cases"]]
    if not ids or len(ids) != len(set(ids)):
        raise ValueError("case ids must be unique and non-empty")
    rules = document.get("normalize", [])
    swaps = [(pair["old"], pair["new"]) for pair in document.get("replace", [])]
    owner = tempfile.TemporaryDirectory(prefix="airk-differential-") if fixture_base is None else None
    base = Path(owner.name if owner else fixture_base).resolve() / "fixture"
    results, problems, service = [], [], None
    try:
        fresh(base)
        if document.get("service", True):
            service = start_service(old, base, timeout)
        for case in document["cases"]:
            observed = {}
            for side, binary in (("old", old), ("new", new)):
                code, stdout, stderr = run_case(binary, base, {"argv": case[side], "stdin": case.get("stdin")},
                                                False, timeout)
                pairs = replacements(base, binary)
                observed[side] = {"exit": code, "stdout": normalize(stdout, pairs, rules),
                                  "stderr": normalize(stderr, pairs, rules)}
            for stream in ("stdout", "stderr"):
                encoding, text = observed["old"][stream]
                if encoding == "utf-8":
                    observed["old"][stream] = (encoding, apply(text, swaps))
            differences = [field for field in ("exit", "stdout", "stderr")
                           if observed["old"][field] != observed["new"][field]]
            problems += [f"{case['id']} {field} differs" for field in differences]
            results.append({"id": case["id"], "equal": not differences, "old": observed["old"],
                            "new": observed["new"]})
    finally:
        if service is not None:
            stop_service(old, base, service)
        if base.exists():
            shutil.rmtree(base, ignore_errors=True)
        if owner:
            owner.cleanup()
    return problems, results


def main(argv=None):
    parser = argparse.ArgumentParser(description=__doc__.splitlines()[0])
    commands = parser.add_subparsers(dest="command", required=True)
    pair = commands.add_parser("differential")
    pair.add_argument("--old", required=True, type=Path, help="the old binary (today's airk)")
    pair.add_argument("--new", required=True, type=Path, help="the new binary (airkctl)")
    pair.add_argument("--cases", required=True, type=Path)
    pair.add_argument("--out", type=Path, help="optional JSON report of both sides per case")
    pair.add_argument("--fixture-base", type=Path)
    pair.add_argument("--timeout", type=int, default=120)
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
    if args.command == "differential":
        problems, results = differential(args.old, args.new, args.cases, args.fixture_base, args.timeout)
        if args.out:
            args.out.parent.mkdir(parents=True, exist_ok=True)
            args.out.write_text(json.dumps({"schema": "airk-differential-report/1", "cases": results},
                                           indent=1, ensure_ascii=False) + "\n", encoding="utf-8", newline="\n")
        for problem in problems:
            print(problem)
        print(json.dumps({"equal": not problems, "cases": len(results), "differences": len(problems)}))
        return 1 if problems else 0
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
