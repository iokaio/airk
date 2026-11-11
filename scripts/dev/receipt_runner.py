# SPDX-License-Identifier: Apache-2.0
"""Run one qualification command and write a receipt of its consumed inputs.

Promoted from the git-ignored one-process harness (split plan U71). The receipt
records the digests of the selected input groups before and after the command,
the command's own file inputs, the runner digest, the selected-test count and
copies of Cargo timing and nextest JUnit reports produced during the run.

Exit status: the command's status; otherwise 3 when a consumed input, the
runner or a sealed runtime changed during the run, or 4 when a recognized test
command selected no tests.

Example:
    python scripts/dev/receipt_runner.py --tier T0 session-contract-r1 \
        cargo nextest run -p airk-cli --lib --profile ci -E 'test(session_protocol::)'
"""
import argparse
import hashlib
import importlib.util
import json
import os
from pathlib import Path
import re
import shutil
import subprocess
import sys
import time

ROOT = Path(__file__).resolve().parents[2]
SCHEMA = "airk-readiness-command/1"
NAME = re.compile(r"[a-z0-9][a-z0-9.-]*")
GROUPS = {
    "native": ("Cargo.toml", "Cargo.lock", "rust-toolchain.toml", ".cargo", "crates",
               "LICENSE", "NOTICE", "THIRD_PARTY_NOTICES.md", "third_party"),
    "extension": ("packages/vscode",),
    "frontend": ("packages",),
    "harness": ("scripts", ".config/nextest.toml", "Dockerfile", ".dockerignore"),
    "fixtures": ("examples", "fixtures", "tools/parser-train", "supply-chain"),
}
GROUPS["native-libraries"] = GROUPS["native"]
SUMMARY = re.compile(r"(?:test result:.*|Summary \[.*|Starting \d+ tests?.*|# tests \d+|# pass \d+"
                     r"|# fail \d+|Ran \d+ tests? in .*)")
SELECTED = (re.compile(r"Starting (\d+) tests?"), re.compile(r"# tests (\d+)"),
            re.compile(r"Ran (\d+) tests? in"), re.compile(r"running (\d+) tests?"))


def digest(path):
    with Path(path).open("rb") as source:
        return hashlib.file_digest(source, "sha256").hexdigest()


def tracked_files(scopes, root=ROOT):
    if not scopes:
        return []
    raw = subprocess.check_output(
        ["git", "-c", "safe.directory=" + root.as_posix(), "ls-files", "-z", "--cached",
         "--others", "--exclude-standard", "--", *scopes], cwd=root)
    return sorted({root / os.fsdecode(name) for name in raw.split(b"\0") if name})


def observe(groups, root=ROOT):
    """Digest every selected group; compiled extension output is included."""
    result = {}
    for group in groups:
        if group not in GROUPS:
            raise ValueError("unknown input group: " + group)
        selected = tracked_files(GROUPS[group], root)
        if group == "native-libraries":
            selected = [path for path in selected
                        if not path.relative_to(root).as_posix().startswith("crates/airk-cli/")
                        or path.relative_to(root).as_posix() == "crates/airk-cli/Cargo.toml"]
        values = {path.relative_to(root).as_posix(): digest(path) if path.is_file() else "missing"
                  for path in selected}
        if group == "extension":
            for path in sorted((root / "packages/vscode/out").rglob("*")):
                if path.is_file():
                    values[path.relative_to(root).as_posix()] = digest(path)
        encoded = json.dumps(values, sort_keys=True, separators=(",", ":")).encode()
        result[group] = {"sha256": hashlib.sha256(encoded).hexdigest(), "files": values}
    return result


def selected_tests(log_text):
    """Return the selected-test count a recognized runner printed, or None."""
    counts = [int(match) for pattern in SELECTED for match in pattern.findall(log_text)]
    return sum(counts) if counts else None


def is_test_command(command):
    words = [Path(word).name.lower().removesuffix(".exe") for word in command[:4]]
    joined = " ".join(command).lower()
    return ("nextest" in words or ("cargo" in words[:1] and "test" in words)
            or "--test" in command or "unittest" in joined)


def load_module(name, path):
    spec = importlib.util.spec_from_file_location(name, path)
    module = importlib.util.module_from_spec(spec)
    spec.loader.exec_module(module)
    return module


def runtime_identity(runtime, target):
    package = load_module("native_package_owner", ROOT / "scripts/package-native.py")
    builder = package.runtime_builder()
    version = builder.tomllib.loads((ROOT / "Cargo.toml").read_text())["workspace"]["package"]["version"]
    values, executables = package.validate_runtime(runtime, version, target, builder.source_identity())
    return {"version": version, "files": values, "executables": executables}


def parse(argv):
    parser = argparse.ArgumentParser(description=__doc__.splitlines()[0])
    parser.add_argument("--groups", default="native,harness,fixtures")
    parser.add_argument("--tier", default="T1", choices=("T0", "T1", "T2", "T3"))
    parser.add_argument("--out", type=Path, default=ROOT / "artifacts/split/receipts",
                        help="receipt directory (default is git-ignored)")
    parser.add_argument("--target-dir", type=Path,
                        help="Cargo target directory (default: CARGO_TARGET_DIR or ./target)")
    parser.add_argument("--node", type=Path, help="Node executable to put first on PATH")
    parser.add_argument("--static-environment", type=Path,
                        help="static ORT environment supplement applied to the command")
    parser.add_argument("--runtime", type=Path, help="sealed runtime directory under test")
    parser.add_argument("--runtime-target", default="x86_64-pc-windows-msvc")
    parser.add_argument("--fixture-base", type=Path,
                        help="owner-only temporary directory (default: <out>/native-fixtures)")
    parser.add_argument("--release", action="store_true", help="leave CARGO_INCREMENTAL unset")
    parser.add_argument("--allow-no-tests", action="store_true")
    parser.add_argument("name")
    parser.add_argument("command", nargs=argparse.REMAINDER)
    args = parser.parse_args(argv)
    if not NAME.fullmatch(args.name) or not args.command:
        parser.error("name must match [a-z0-9][a-z0-9.-]* and a command is required")
    return args


def environment(args, out):
    env = os.environ.copy()
    paths = [str(Path.home() / ".cargo" / "bin")]
    if args.node:
        paths.insert(0, str(args.node.resolve(strict=True).parent))
    env["PATH"] = os.pathsep.join(paths + [env.get("PATH", "")])
    target = args.target_dir or Path(env.get("CARGO_TARGET_DIR") or ROOT / "target")
    env["CARGO_TARGET_DIR"] = str(Path(target).resolve())
    env["CARGO_INCREMENTAL"] = "1" if sys.platform == "win32" else "0"
    if args.release:
        env.pop("CARGO_INCREMENTAL", None)
    model = ROOT / "target/local-vector-model"
    if model.is_dir():
        env.setdefault("AIRK_BUNDLED_MODEL_DIR", str(model))
        env.setdefault("AIRK_TEST_MODEL_DIR", str(model / "minilm-l6-v2-u8-v1"))
    env.update(GIT_CONFIG_COUNT="1", GIT_CONFIG_KEY_0="safe.directory",
               GIT_CONFIG_VALUE_0=ROOT.as_posix())
    for key in ("AIRK_TEST_BINARY", "AIRK_NATIVE_TEST_BINARY", "AIRK_TEST_F09_QUALIFICATION"):
        env.pop(key, None)
    if args.static_environment:
        static = load_module("static_owner", ROOT / "scripts/static-runtime-environment.py")
        env.update(static.load_environment_file(args.static_environment.resolve(strict=True)))
    private = (args.fixture_base or out / "native-fixtures").resolve()
    private.mkdir(parents=True, exist_ok=True)
    env.update(TEMP=str(private), TMP=str(private), AIRK_PRIVATE_CENSUS_BASE=str(private))
    return env


def main(argv=None):
    args = parse(argv)
    out = args.out.resolve()
    out.mkdir(parents=True, exist_ok=True)
    env = environment(args, out)
    runtime_before = None
    if args.runtime:
        runtime = args.runtime.resolve(strict=True)
        runtime_before = runtime_identity(runtime, args.runtime_target)
        binary = runtime / ("airk.exe" if sys.platform == "win32" else "airk")
        env.update(AIRK_TEST_BINARY=str(binary), AIRK_NATIVE_TEST_BINARY=str(binary),
                   AIRK_TEST_F09_QUALIFICATION="1")
    command = list(args.command)
    command[0] = shutil.which(command[0], path=env["PATH"]) or command[0]
    command_inputs = {}
    for value in command:
        path = Path(value) if Path(value).is_absolute() else ROOT / value
        if path.is_file() and path.resolve().is_relative_to(ROOT):
            command_inputs[path.resolve().relative_to(ROOT).as_posix()] = digest(path)
    runner_bytes = Path(__file__).read_bytes()
    runner_hash = hashlib.sha256(runner_bytes).hexdigest()
    groups = [group for group in args.groups.split(",") if group]
    before = observe(groups)
    started = time.monotonic()
    started_unix = time.time()
    log_path = out / (args.name + ".log")
    with log_path.open("x", encoding="utf-8") as log:
        print(json.dumps({"stage": args.name, "tier": args.tier, "command": command,
                          "input_groups": {k: v["sha256"] for k, v in before.items()}}), flush=True)
        result = subprocess.run(command, cwd=ROOT, env=env, stdout=log, stderr=subprocess.STDOUT)
    after = observe(groups)
    runtime_after = runtime_identity(runtime, args.runtime_target) if args.runtime else None
    log_text = log_path.read_text(encoding="utf-8", errors="replace")
    count = selected_tests(log_text)
    receipt = {
        "schema": SCHEMA, "command": command, "tier": args.tier, "exit_code": result.returncode,
        "elapsed_seconds": time.monotonic() - started, "started_unix": started_unix,
        "groups": before, "inputs_unchanged": before == after,
        "changed_groups": [k for k in before if before[k] != after[k]],
        "target_directory": env["CARGO_TARGET_DIR"],
        "incremental": env.get("CARGO_INCREMENTAL", "profile default; environment unset"),
        "log": log_path.name, "runner_sha256": runner_hash,
        "runner_unchanged": digest(Path(__file__)) == runner_hash,
        "runtime_seal": runtime_before, "runtime_unchanged": runtime_before == runtime_after,
        "command_inputs": command_inputs,
        "command_inputs_unchanged": all((ROOT / path).is_file() and digest(ROOT / path) == value
                                        for path, value in command_inputs.items()),
        "selected_tests": count, "test_summaries": SUMMARY.findall(log_text),
    }
    if args.node:
        receipt["node"] = {"path_sha256": digest(args.node),
                           "version": subprocess.run([str(args.node), "--version"], capture_output=True,
                                                     text=True).stdout.strip()}
    timing = Path(env["CARGO_TARGET_DIR"]) / "cargo-timings/cargo-timing.html"
    if timing.exists() and timing.stat().st_mtime >= started_unix:
        shutil.copy2(timing, out / (args.name + "-cargo-timing.html"))
        receipt["cargo_timings"] = args.name + "-cargo-timing.html"
    junit = Path(env["CARGO_TARGET_DIR"]) / "nextest/ci/junit.xml"
    if junit.exists() and junit.stat().st_mtime >= started_unix:
        shutil.copy2(junit, out / (args.name + "-junit.xml"))
        receipt["junit"] = {"path": args.name + "-junit.xml", "sha256": digest(junit)}
    (out / (args.name + "-runner.py")).write_bytes(runner_bytes)
    (out / (args.name + ".json")).write_text(json.dumps(receipt, indent=2) + "\n", encoding="utf-8")
    print(json.dumps({k: v for k, v in receipt.items() if k not in ("groups", "runtime_seal")}),
          flush=True)
    if result.returncode:
        return result.returncode
    if (before != after or not receipt["runner_unchanged"] or not receipt["runtime_unchanged"]
            or not receipt["command_inputs_unchanged"]):
        return 3
    if is_test_command(command) and not args.allow_no_tests and not count:
        return 4
    return 0


if __name__ == "__main__":
    raise SystemExit(main())
