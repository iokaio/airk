# SPDX-License-Identifier: Apache-2.0
"""Run one qualification command and write a receipt of its consumed inputs.

Promoted from the git-ignored one-process harness (split plan U71). The receipt
records the digests of the selected input groups before and after the command,
the command's own file inputs, the runner digest, the selected-test count and
copies of Cargo timing and nextest JUnit reports produced during the run.
An immutable, flushed `<name>-start.json`, runner snapshot and provisional
`<name>.json` are written before the child launches. A hard-killed runner may
leave only a started observation; it is never a completed or passing receipt.
SIGINT/SIGBREAK and KeyboardInterrupt produce an interrupted nonpass receipt
with only the actually observed direct-child status, never descendant cleanup.

Exit status: the command's status; otherwise 3 when a consumed input, the
runner or a sealed runtime changed during the run, or 4 when a recognized test
command selected no tests. An interrupt returns 130; a launch failure returns
127, and an unknown child outcome or evidence failure returns 5 unless an
observed failing child already supplies its own nonzero status.

Example:
    python scripts/dev/receipt_runner.py --tier T0 session-contract-r1 \
        cargo nextest run -p airk-cli --lib --profile ci -E 'test(session_protocol::)'
"""
import argparse
from datetime import datetime, timezone
import errno
import hashlib
import importlib.util
import json
import os
from pathlib import Path
import re
import shutil
import signal
import stat
import subprocess
import sys
import tempfile
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
                     r"|# fail \d+|ℹ (?:tests|pass|fail) \d+|Ran \d+ tests? in .*)")
SELECTED = (re.compile(r"Starting (\d+) tests?"), re.compile(r"# tests (\d+)"),
            re.compile(r"Ran (\d+) tests? in"), re.compile(r"running (\d+) tests?"),
            re.compile(r"^\s*ℹ tests (\d+)\s*$", re.MULTILINE))
NEXTEST_START = re.compile(r"^\s*Starting (\d+) tests? across \d+ binar(?:y|ies)\b")
NEXTEST_END = re.compile(r"^\s*Summary \[")


def digest(path):
    with Path(path).open("rb") as source:
        return hashlib.file_digest(source, "sha256").hexdigest()


def command_input_files(command, root=ROOT):
    """Seal repository files named by argv; ordinary arguments need not be paths.

    Probe stat directly so permission and I/O failures are not hidden by
    pathlib versions that make is_file() return False for every OSError.
    """
    inputs = {}
    for value in command:
        path = Path(value) if Path(value).is_absolute() else root / value
        try:
            metadata = path.stat()
        except (FileNotFoundError, NotADirectoryError):
            continue
        except OSError as error:
            # Long selectors and Windows-invalid filename characters are valid
            # argv, but cannot name a file. Do not suppress access/I/O errors.
            if error.errno == errno.ENAMETOOLONG or getattr(error, "winerror", None) in (123, 206):
                continue
            raise
        if stat.S_ISREG(metadata.st_mode):
            resolved = path.resolve(strict=True)
            if resolved.is_relative_to(root):
                inputs[resolved.relative_to(root).as_posix()] = digest(path)
    return inputs


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
    counts, outside = [], []
    in_nextest = False
    for line in log_text.splitlines():
        if in_nextest:
            # A failed Nextest child prints its libtest banner inside this
            # block. It belongs to the selection already recorded at START.
            if NEXTEST_END.match(line):
                in_nextest = False
            continue
        start = NEXTEST_START.match(line)
        if start:
            counts.append(int(start.group(1)))
            in_nextest = True
        else:
            outside.append(line)
    remaining = "\n".join(outside)
    counts.extend(int(match) for pattern in SELECTED for match in pattern.findall(remaining))
    return sum(counts) if counts else None


def is_test_command(command):
    """Recognize test execution, rather than a compilation target or argv text.

    Classify the actual argv without executing or shell-parsing it. In particular,
    Cargo build/check/clippy --test and --bench only select compilation targets.
    """
    if not command:
        return False
    program = str(command[0]).replace("\\", "/").rsplit("/", 1)[-1].lower().removesuffix(".exe")
    args = list(command[1:])
    if program == "env":
        while args:
            word = args.pop(0)
            if word == "--":
                return is_test_command(args)
            if re.fullmatch(r"[A-Za-z_][A-Za-z0-9_]*=.*", word, re.DOTALL) or word in ("-i", "--ignore-environment"):
                continue
            if word in ("-u", "--unset", "-C", "--chdir"):
                if not args:
                    return False
                args.pop(0)
                continue
            if word.startswith(("--unset=", "--chdir=")):
                continue
            # env -S is a shell-like string, not this argv's executable.
            return False if word.startswith("-") else is_test_command([word, *args])
        return False
    if program == "cargo":
        while args:
            word = args.pop(0)
            if word.startswith("+") or word in ("-v", "-vv", "-q", "--verbose", "--quiet", "--offline", "--locked", "--frozen"):
                continue
            if word in ("--color", "--config", "-Z", "-C"):
                if not args:
                    return False
                args.pop(0)
                continue
            if word.startswith(("--color=", "--config=", "-Z", "-C")):
                continue
            if word == "test":
                cargo_args = args[:args.index("--")] if "--" in args else args
                return not any(flag in cargo_args for flag in ("--no-run", "--help", "-h", "--version", "-V")) and "--list" not in args
            if word == "nextest":
                return is_test_command(["cargo-nextest", *args])
            return False
        return False
    if program in ("cargo-nextest", "nextest"):
        if args[:1] == ["nextest"]:
            args.pop(0)
        return args[:1] == ["run"] and not any(flag in args for flag in ("--help", "-h", "--version", "-V", "--list"))
    if re.fullmatch(r"python(?:[0-9]+(?:\.[0-9]+)*)?|py", program):
        while args:
            word = args.pop(0)
            if word in ("-B", "-I", "-u", "-E", "-s", "-S", "-q") or (program == "py" and re.fullmatch(r"-3(?:\.[0-9]+)?", word)):
                continue
            if word in ("-W", "-X"):
                if not args:
                    return False
                args.pop(0)
                continue
            if word.startswith(("-W", "-X")):
                continue
            if word == "-m":
                return args[:1] == ["unittest"] and not any(flag in args[1:] for flag in ("--help", "-h"))
            # Only the maintained environment wrapper transparently forwards argv.
            path = Path(word) if Path(word).is_absolute() else ROOT / word
            if path.resolve() == (ROOT / "scripts/static-runtime-environment.py").resolve() and "--" in args:
                return is_test_command(args[args.index("--") + 1:])
            return False
        return False
    if program == "node":
        selected = False
        while args:
            word = args.pop(0)
            if word in ("--help", "-h", "--version", "-v", "--eval", "-e", "--print", "-p", "--check", "-c"):
                return False
            if word == "--" or not word.startswith("-"):
                return selected
            if word == "--test":
                selected = True
            elif word in ("--import", "--require", "-r", "--loader", "--test-reporter", "--test-name-pattern", "--test-skip-pattern", "--test-concurrency", "--test-timeout", "--test-reporter-destination"):
                if not args:
                    return False
                args.pop(0)
        return selected
    return False


def load_module(name, path):
    spec = importlib.util.spec_from_file_location(name, path)
    module = importlib.util.module_from_spec(spec)
    spec.loader.exec_module(module)
    return module



def cargo_timing_command(command, root=None):
    """Only Cargo or reviewed build entrypoints can own a fresh shared timing file.

    Inline programs and arbitrary scripts are not evidence of a Cargo producer.
    The maintained static-environment wrapper forwards one explicit argv after --.
    """
    root = ROOT if root is None else Path(root)
    if not command:
        return False
    program = Path(command[0]).name.lower().removesuffix(".exe")
    arguments = list(command[1:])
    if program == "cargo":
        return any(word == "--timings" or word.startswith("--timings=") for word in arguments)
    if re.fullmatch(r"python(?:[0-9]+(?:\.[0-9]+)*)?", program) or program == "py":
        while arguments and arguments[0] in ("-B", "-I", "-u", "-E", "-s", "-S"):
            arguments.pop(0)
        if not arguments or arguments[0].startswith("-"):
            return False
        script, arguments = arguments[0], arguments[1:]
        if not script.lower().endswith(".py"):
            return False
    elif program in ("pwsh", "powershell"):
        lowered = [word.lower() for word in arguments]
        if "-command" in lowered or "-c" in lowered or "-file" not in lowered:
            return False
        index = lowered.index("-file") + 1
        if index >= len(arguments):
            return False
        script, arguments = arguments[index], arguments[index + 1:]
        if not script.lower().endswith(".ps1"):
            return False
    else:
        return False
    path = Path(script)
    path = (path if path.is_absolute() else root / path).resolve()
    try:
        script = path.relative_to(root.resolve()).as_posix()
    except ValueError:
        return False
    if script == "scripts/static-runtime-environment.py":
        return "--" in arguments and cargo_timing_command(arguments[arguments.index("--") + 1:], root)
    if script in ("scripts/build-service-runtime.py", "scripts/build-service-container.py"):
        return True
    if script == "scripts/dev-check.py":
        return "--dry-run" not in arguments
    if script == "scripts/build-local-service-installer.py":
        return "--force-build" in arguments and "--runtime-dir" not in arguments
    if script in ("scripts/build-local-service-installer.ps1", "scripts/build-local-installers.ps1"):
        lowered = [word.lower() for word in arguments]
        return "-forcebuild" in lowered and "-runtimedir" not in lowered
    return False



def cargo_timing_report(command, log_text, target, started_unix):
    """Select the command's actual saved report, never a concurrent latest alias."""
    if not cargo_timing_command(command) or log_text is None:
        return None
    folder = (Path(target) / "cargo-timings").resolve()
    selected = None
    for line in log_text.splitlines():
        match = re.match(r"^\s*Timing report saved to (.+?)\s*$", line)
        if match is None:
            continue
        path = Path(match[1].strip("`"))
        path = (path if path.is_absolute() else ROOT / path).resolve()
        if (path.parent == folder and re.fullmatch(r"cargo-timing(?:-[A-Za-z0-9-]+)?\.html", path.name)
                and path.is_file() and path.stat().st_mtime >= started_unix):
            selected = path
    return selected


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
    # Preserve the actual invoking user's Git environment. Injected trust
    # configuration makes native safe-writer eligibility refuse its correctly
    # unsupported GIT_* override profile, and can overwrite a user's own values.
    # A same-user checkout needs no synthetic safe.directory exception.
    for key in ("AIRK_TEST_BINARY", "AIRK_NATIVE_TEST_BINARY", "AIRK_TEST_F09_QUALIFICATION"):
        env.pop(key, None)
    if args.static_environment:
        static = load_module("static_owner", ROOT / "scripts/static-runtime-environment.py")
        env.update(static.load_environment_file(args.static_environment.resolve(strict=True)))
    private = (args.fixture_base or out / "native-fixtures").resolve()
    private.mkdir(parents=True, exist_ok=True)
    env.update(TEMP=str(private), TMP=str(private), TMPDIR=str(private),
               AIRK_PRIVATE_CENSUS_BASE=str(private))
    return env


def write_json(path, document, *, create=False):
    """Flush one complete observation; never replace the immutable start record."""
    raw = (json.dumps(document, indent=2) + "\n").encode("utf-8")
    temporary = None
    try:
        if create:
            stream = path.open("xb")
        else:
            descriptor, name = tempfile.mkstemp(prefix=path.name + ".", suffix=".tmp", dir=path.parent)
            temporary = Path(name)
            stream = os.fdopen(descriptor, "wb")
        with stream:
            stream.write(raw)
            stream.flush()
            os.fsync(stream.fileno())
        if temporary is not None:
            os.replace(temporary, path)
            temporary = None
        if os.name != "nt":
            descriptor = os.open(path.parent, os.O_RDONLY | getattr(os, "O_DIRECTORY", 0))
            try:
                os.fsync(descriptor)
            finally:
                os.close(descriptor)
    finally:
        if temporary is not None:
            temporary.unlink(missing_ok=True)


class Interrupts:
    """Remember the first interrupt; repeated signals cannot abort final evidence."""
    def __init__(self):
        self.requested = False
        self.finalizing = False
        self.observation = None
        self.previous = {}

    def mark(self, reason):
        if not self.requested:
            self.observation = {"reason": reason, "observed_unix": time.time()}
        self.requested = True

    def receive(self, number, _frame):
        self.mark(signal.Signals(number).name)
        if not self.finalizing:
            self.finalizing = True
            raise KeyboardInterrupt

    def install(self):
        for number in (signal.SIGINT, getattr(signal, "SIGBREAK", None)):
            if number is not None:
                try:
                    self.previous[number] = signal.signal(number, self.receive)
                except ValueError:
                    # Library callers may run outside the main thread. A real
                    # KeyboardInterrupt is still recorded by the same owner.
                    pass

    def restore(self):
        for number, previous in self.previous.items():
            signal.signal(number, previous)


def child_observation(child):
    observation = {"pid": None if child is None else child.pid, "returncode": None,
                   "state": "not_observed", "observed_unix": time.time(),
                   "scope": "direct child only; descendant settlement is not certified"}
    if child is not None:
        try:
            observation["returncode"] = child.poll()
            observation["state"] = "running" if observation["returncode"] is None else "exited"
        except OSError as error:
            observation["error"] = {"type": type(error).__name__, "message": str(error)}
    return observation


def main(argv=None):
    args = parse(argv)
    out = args.out.resolve()
    out.mkdir(parents=True, exist_ok=True)
    log_path = out / (args.name + ".log")
    start_path = out / (args.name + "-start.json")
    receipt_path = out / (args.name + ".json")
    runner_path = out / (args.name + "-runner.py")
    # A retained start is an uncertain previous attempt, never permission to
    # replay it or replace its evidence. Individual exclusive opens fence races.
    for path in (log_path, start_path, receipt_path, runner_path):
        if path.exists() or path.is_symlink():
            raise FileExistsError(path)
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
    command_inputs = command_input_files(command)
    runner_bytes = Path(__file__).read_bytes()
    runner_hash = hashlib.sha256(runner_bytes).hexdigest()
    groups = [group for group in args.groups.split(",") if group]
    before = observe(groups)
    started = time.monotonic()
    started_unix = time.time()
    receipt = {
        "schema": SCHEMA, "command": command, "tier": args.tier,
        "status": "started", "passed": False, "exit_code": None, "runner_exit_code": None,
        "started_unix": started_unix,
        "started_utc": datetime.fromtimestamp(started_unix, timezone.utc).isoformat(),
        "ended_unix": None, "ended_utc": None, "elapsed_seconds": None,
        "groups": before, "inputs_unchanged": None, "changed_groups": None,
        "target_directory": env["CARGO_TARGET_DIR"],
        "incremental": env.get("CARGO_INCREMENTAL", "profile default; environment unset"),
        "log": log_path.name, "runner_sha256": runner_hash, "runner_pid": os.getpid(),
        "runner_unchanged": None, "runtime_seal": runtime_before, "runtime_unchanged": None,
        "command_inputs": command_inputs, "command_inputs_unchanged": None,
        "selected_tests": None, "test_summaries": [], "interrupted": False,
        "child": child_observation(None), "evidence_errors": [],
    }
    if Path(command[0]).is_file():
        receipt["command_executable_sha256"] = digest(command[0])
    with runner_path.open("xb") as snapshot:
        snapshot.write(runner_bytes)
        snapshot.flush()
        os.fsync(snapshot.fileno())
    with log_path.open("xb") as log:
        write_json(start_path, receipt, create=True)
        receipt["start_receipt"] = {"path": start_path.name, "sha256": digest(start_path)}
        write_json(receipt_path, receipt, create=True)
        print(json.dumps({"stage": args.name, "tier": args.tier, "command": command,
                          "start_receipt": receipt["start_receipt"],
                          "input_groups": {k: v["sha256"] for k, v in before.items()}}), flush=True)
        child, execution_error = None, None
        interrupts = Interrupts()
        interrupts.install()
        try:
            try:
                child = subprocess.Popen(command, cwd=ROOT, env=env, stdout=log, stderr=subprocess.STDOUT)
                receipt["child"] = child_observation(child)
                write_json(receipt_path, receipt)
                child.wait()
            except KeyboardInterrupt:
                interrupts.mark("KeyboardInterrupt")
            except Exception as error:
                execution_error = {"type": type(error).__name__, "message": str(error)}
            finally:
                interrupts.finalizing = True
                log.flush()
                os.fsync(log.fileno())

            def evidence(label, operation):
                try:
                    return operation()
                except KeyboardInterrupt:
                    interrupts.mark("KeyboardInterrupt")
                except Exception as error:
                    receipt["evidence_errors"].append({"operation": label, "type": type(error).__name__,
                                                       "message": str(error)})
                return None

            after = evidence("input_groups", lambda: observe(groups))
            receipt["inputs_unchanged"] = None if after is None else before == after
            receipt["changed_groups"] = None if after is None else [k for k in before if before[k] != after[k]]
            receipt["runner_unchanged"] = evidence("runner_digest", lambda: digest(Path(__file__)) == runner_hash)
            receipt["command_inputs_unchanged"] = evidence("command_inputs", lambda: all(
                (ROOT / path).is_file() and digest(ROOT / path) == value for path, value in command_inputs.items()))
            if not args.runtime:
                receipt["runtime_unchanged"] = True
            elif not interrupts.requested:
                runtime_after = evidence("runtime_seal", lambda: runtime_identity(runtime, args.runtime_target))
                receipt["runtime_unchanged"] = None if runtime_after is None else runtime_before == runtime_after
            # Interruption must not launch further runtime/version probes. The
            # immutable start retains their genuine pre-command identities.
            log_text = evidence("log", lambda: log_path.read_text(encoding="utf-8", errors="replace"))
            if log_text is not None:
                receipt["selected_tests"] = selected_tests(log_text)
                receipt["test_summaries"] = SUMMARY.findall(log_text)
            observation = child_observation(child)
            receipt["child"] = observation
            receipt["exit_code"] = observation["returncode"]
            if execution_error is not None:
                receipt["execution_error"] = execution_error
            if observation["state"] == "exited" and not interrupts.requested:
                if args.node:
                    def node_identity():
                        return {"path_sha256": digest(args.node), "version": subprocess.run(
                            [str(args.node), "--version"], capture_output=True, text=True).stdout.strip()}
                    receipt["node"] = evidence("node", node_identity)
                timing = evidence("cargo_timing_source", lambda: cargo_timing_report(
                    command, log_text, env["CARGO_TARGET_DIR"], started_unix))
                if timing is not None:
                    def copy_timing():
                        name = args.name + "-cargo-timing.html"
                        shutil.copy2(timing, out / name)
                        return name
                    receipt["cargo_timings"] = evidence("cargo_timings", copy_timing)
                junit = Path(env["CARGO_TARGET_DIR"]) / "nextest/ci/junit.xml"
                # A fresh shared-target report alone is not this command's evidence.
                if (log_text is not None and any(NEXTEST_START.match(line) for line in log_text.splitlines())
                        and junit.exists() and junit.stat().st_mtime >= started_unix):
                    def copy_junit():
                        name = args.name + "-junit.xml"
                        shutil.copy2(junit, out / name)
                        return {"path": name, "sha256": digest(junit)}
                    receipt["junit"] = evidence("junit", copy_junit)

            def disposition():
                if interrupts.requested:
                    return "interrupted", 130
                if execution_error is not None:
                    return "launch_failed" if child is None else "execution_failed", 127 if child is None else 5
                if observation["state"] != "exited":
                    return "child_outcome_unknown", 5
                if receipt["evidence_errors"]:
                    return "evidence_failed", observation["returncode"] or 5
                if observation["returncode"]:
                    return "completed", observation["returncode"]
                if not all(receipt[key] is True for key in (
                        "inputs_unchanged", "runner_unchanged", "runtime_unchanged", "command_inputs_unchanged")):
                    return "completed", 3
                if is_test_command(command) and not args.allow_no_tests and not receipt["selected_tests"]:
                    return "completed", 4
                return "completed", 0

            receipt["status"], receipt["runner_exit_code"] = disposition()
            receipt["interrupted"] = interrupts.requested
            if interrupts.requested:
                receipt["interruption"] = interrupts.observation
            receipt["passed"] = receipt["runner_exit_code"] == 0
            receipt["ended_unix"] = time.time()
            receipt["ended_utc"] = datetime.fromtimestamp(receipt["ended_unix"], timezone.utc).isoformat()
            receipt["elapsed_seconds"] = time.monotonic() - started
            receipt["elapsed_scope"] = "Runner after input freeze through final observation; excludes final receipt write"
            write_json(receipt_path, receipt)
            if interrupts.requested and not receipt["interrupted"]:
                receipt.update(status="interrupted", interrupted=True, passed=False, runner_exit_code=130,
                               interruption=interrupts.observation)
                write_json(receipt_path, receipt)
        finally:
            interrupts.restore()
    print(json.dumps({k: v for k, v in receipt.items() if k not in ("groups", "runtime_seal")}), flush=True)
    return receipt["runner_exit_code"]


if __name__ == "__main__":
    raise SystemExit(main())
