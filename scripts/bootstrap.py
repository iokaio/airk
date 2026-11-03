# SPDX-License-Identifier: Apache-2.0
"""Run the documented contributor checks with explicit downloads and isolated logs.

This wrapper installs no tools, modifies no global configuration and never starts
the installed AIRK service. Offline is the default; --download permits the owning
provisioners, Cargo and npm to fetch their pinned development inputs.
"""
import argparse
from datetime import datetime, timezone
import importlib.util
import json
import os
from pathlib import Path
import platform
import re
import shutil
import subprocess
import sys
import time
if sys.version_info < (3, 11):
    raise SystemExit("Bootstrap requires Python 3.11 or newer; rerun with the documented interpreter.")
import tomllib
import uuid

ROOT = Path(__file__).resolve().parents[1]


def dependency_policy_owner():
    """Read the canonical tool pins/probes without running an audit or download."""
    specification = importlib.util.spec_from_file_location("airk_bootstrap_dependency_policy", ROOT / "scripts/dependency-audit.py")
    module = importlib.util.module_from_spec(specification)
    specification.loader.exec_module(module)
    return module


def pins(root=ROOT):
    rust = tomllib.loads((root / "rust-toolchain.toml").read_text(encoding="utf-8"))["toolchain"]
    ci = (root / ".github/workflows/ci.yml").read_text(encoding="utf-8")
    nodes = set(re.findall(r"node-version:\s*['\"]([0-9.]+)['\"]", ci))
    nextests = set(re.findall(r"tool:\s*cargo-nextest@([0-9.]+)", ci))
    if len(nodes) != 1 or len(nextests) != 1:
        raise ValueError("Expected one current Node and nextest pin in CI; update bootstrap deliberately")
    return {"rust": rust["channel"], "components": rust["components"],
            "node": nodes.pop(), "nextest": nextests.pop()}


def npm_command(node, npm):
    # Execute the actual npm JS entry rather than invoking a Windows .cmd through
    # a shell. All commands keep argv boundaries, including paths with spaces.
    resolved = Path(npm).resolve()
    candidates = [resolved] if resolved.suffix == ".js" else []
    candidates += [Path(npm).parent / "node_modules/npm/bin/npm-cli.js",
                   Path(node).parent / "node_modules/npm/bin/npm-cli.js"]
    for candidate in candidates:
        if candidate.is_file():
            return [node, str(candidate)]
    if os.name != "nt":
        return [npm]
    raise ValueError("Cannot locate npm-cli.js beside npm/node; install the documented Node distribution")


def prerequisites(selected, locate=shutil.which, probe=None):
    probe = probe or (lambda args: subprocess.check_output(args, cwd=ROOT, text=True,
                                                          stderr=subprocess.STDOUT, timeout=30).strip())
    errors, commands = [], {}
    if sys.version_info < (3, 11):
        errors.append("Install Python 3.11 or newer, then rerun this script with that interpreter.")
    for name, help_text in [("git", "Install Git and make it available in this terminal."),
                            ("rustup", "Install rustup and the repository's pinned Rust toolchain."),
                            ("node", f"Install Node.js {selected['node']} and npm."),
                            ("npm", "Install npm with the documented Node.js distribution."),
                            ("pwsh", "Install PowerShell 7 and make pwsh available in this terminal.")]:
        path = locate(name)
        if path:
            commands[name] = [path]
        else:
            errors.append(help_text)
    if "node" in commands:
        try:
            actual = probe(commands["node"] + ["--version"]).removeprefix("v")
            if actual != selected["node"]:
                errors.append(f"Use pinned Node.js {selected['node']} (found {actual}).")
        except (OSError, subprocess.SubprocessError) as error:
            errors.append(f"Cannot execute Node.js: {error}")
    if "node" in commands and "npm" in commands:
        try:
            commands["npm"] = npm_command(commands["node"][0], commands["npm"][0])
            probe(commands["npm"] + ["--version"])
        except (OSError, ValueError, subprocess.SubprocessError) as error:
            errors.append(f"Cannot execute npm: {error}")
    if "rustup" in commands:
        rustup = commands["rustup"]
        try:
            installed = probe(rustup + ["toolchain", "list"])
            if not any(line.split()[0].startswith(selected["rust"] + "-")
                       or line.split()[0] == selected["rust"] for line in installed.splitlines() if line.split()):
                raise ValueError("toolchain is not installed")
            components = probe(rustup + ["component", "list", "--toolchain", selected["rust"], "--installed"])
            if any(not any(line.startswith(component + "-") or line == component
                           for line in components.splitlines()) for component in selected["components"]):
                raise ValueError("required components are not installed")
            commands["cargo"] = rustup + ["run", selected["rust"], "cargo"]
            actual = probe(commands["cargo"] + ["nextest", "--version"])
            if not re.search(r"\bcargo-nextest " + re.escape(selected["nextest"]) + r"(?:\s|$)", actual):
                raise ValueError("nextest version differs or is absent")
        except (OSError, ValueError, subprocess.SubprocessError) as error:
            errors.append(f"Prepare Rust explicitly: rustup toolchain install {selected['rust']} "
                          f"--component {','.join(selected['components'])}; then cargo +{selected['rust']} "
                          f"install cargo-nextest --version {selected['nextest']} --locked. ({error})")
    if errors:
        raise ValueError("\n".join(errors))
    # Avoid secondary tool errors (or rustup's implicit toolchain acquisition)
    # when the underlying contributor prerequisites are not ready yet.
    policy = dependency_policy_owner()
    for name, version in policy.PINNED_TOOLS.items():
        try:
            policy.require_pinned_tool(name)
        except (policy.ScannerError, OSError, subprocess.SubprocessError) as error:
            errors.append(f"Prepare pinned {name} {version} explicitly under artifacts/tools/{name} or on PATH. ({error})")
    if errors:
        raise ValueError("\n".join(errors))
    return commands


def steps(args, commands, directory):
    python = [sys.executable]
    cargo, npm = commands["cargo"], commands["npm"]
    model = Path(os.environ.get("AIRK_BUNDLED_MODEL_DIR", ROOT / "target/local-vector-model"))
    downloads = ["--download"] if args.download else []
    result = [("dependency-policy", python + ["scripts/dependency-audit.py", "--require-fresh" if args.download else "--offline",
                                               "--output", str(directory / "dependency-policy.json")]),
              ("model-inputs", python + ["scripts/provision-local-model.py", "--destination", str(model), *downloads]),
              ("runtime-inputs", python + ["scripts/provision-onnx-runtime.py", *downloads]),
              ("npm-inputs", npm + ["ci", "--ignore-scripts", "--no-audit", "--no-fund",
                                      "--prefix", "packages/vscode", *([] if args.download else ["--offline"])]),
              ("runtime-build", python + ["scripts/build-service-runtime.py", "--destination", str(directory / "runtime"), "--profile", "debug"])]
    if args.no_tests:
        return result + [("extension-build", npm + ["run", "compile", "--prefix", "packages/vscode"])]
    result.append(("format", cargo + ["fmt", "--all", "--check"]))
    if not args.fast:
        result.append(("clippy", cargo + ["clippy", "--workspace", "--all-targets", "--all-features", "--locked", "--", "-D", "warnings"]))
    return result + [("native-tests", cargo + ["nextest", "run", "--workspace", "--all-features", "--locked", "--profile", "ci"]),
                     ("doctests", cargo + ["test", "--doc", "--workspace", "--locked"]),
                     ("extension-tests", npm + ["test", "--prefix", "packages/vscode"]),
                     ("python-tests", python + ["-m", "unittest", "discover", "-s", "scripts/tests", "-v"]),
                     ("repository", commands["pwsh"] + ["-NoProfile", "-File", "scripts/check-repo.ps1"]),
                     ("whitespace", commands["git"] + ["-c", f"safe.directory={ROOT.as_posix()}", "diff", "--check"])]


def counts(name, tail, code):
    if name == "native-tests":
        summaries = re.findall(r"\d+ tests? run:\s*([^\r\n]+)", tail)
        if summaries:
            # Nextest inserts failed/flaky observations between passed and
            # skipped. Parse the whole final summary independently of ordering;
            # flaky is a subset of passed, not another successful-test count.
            observed = {key: int(value) for value, key in
                        re.findall(r"(\d+)\s+(passed|failed|flaky|skipped)\b", summaries[-1])}
            if observed:
                return {"passed": observed.get("passed", 0), "skipped": observed.get("skipped", 0),
                        "failed": observed.get("failed", 0 if code == 0 else None),
                        "flaky": observed.get("flaky", 0)}
    if name == "extension-tests":
        values = {key: re.findall(r"(?:ℹ|#)\s+" + key + r"\s+(\d+)", tail) for key in ["pass", "fail", "skipped"]}
        if all(values.values()):
            return {"passed": int(values["pass"][-1]), "failed": int(values["fail"][-1]), "skipped": int(values["skipped"][-1])}
    if name == "python-tests" and code == 0:
        total = re.search(r"Ran (\d+) tests? in", tail)
        skipped = re.search(r"OK \(skipped=(\d+)\)", tail)
        if total:
            skip = int(skipped[1]) if skipped else 0
            return {"passed": int(total[1]) - skip, "skipped": skip, "failed": 0}
    return None  # Never invent passing counts from an exit status alone.


def execute(name, command, directory, environment):
    started = time.monotonic()
    log = directory / (name + ".log")
    print(f"\n{ name }: {subprocess.list2cmdline(command)}", flush=True)
    tail = ""
    with log.open("w", encoding="utf-8") as stream:
        child = subprocess.Popen(command, cwd=ROOT, env=environment, stdout=subprocess.PIPE,
                                 stderr=subprocess.STDOUT, text=True, encoding="utf-8", errors="replace")
        try:
            for line in child.stdout:
                stream.write(line)
                print(line, end="", flush=True)
                tail = (tail + line)[-65536:]
            code = child.wait()
        finally:
            child.stdout.close()
            if child.poll() is None:
                child.terminate()
                child.wait()
    return {"name": name, "command": command, "exit_code": code,
            "seconds": round(time.monotonic() - started, 3), "log": str(log),
            "counts": counts(name, re.sub(r"\x1b\[[0-9;]*m", "", tail), code)}


def dependency_policy_report(directory):
    """Keep the complete owner's report; never infer security from suite counts."""
    path = directory / "dependency-policy.json"
    try:
        value = json.loads(path.read_text(encoding="utf-8"))
        if not isinstance(value, dict) or value.get("schema") != "airk.dependency-policy-check/1":
            raise ValueError("Dependency policy returned an unsupported report schema")
        return value
    except (OSError, ValueError) as error:
        return {"status": "unavailable", "error": f"Dependency policy report is unavailable: {error}",
                "fresh_full_advisory_check": False}


def main(argv=None):
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("--check-only", action="store_true", help="Check tools/pins only; no provisioning, build or tests")
    parser.add_argument("--no-tests", action="store_true", help="Verify/provision inputs and build only")
    parser.add_argument("--fast", action="store_true", help="Skip Clippy; not complete phase acceptance")
    parser.add_argument("--download", action="store_true", help="Permit explicit pinned development-input downloads; default is offline")
    args = parser.parse_args(argv)
    started = time.monotonic()
    selected = pins()
    commands = prerequisites(selected)
    print("Contributor tool checks passed. Native C/C++ SDK availability is established by the build.", flush=True)
    if args.check_only:
        return 0
    directory = ROOT / "artifacts/bootstrap" / (datetime.now(timezone.utc).strftime("%Y%m%dT%H%M%SZ-") + uuid.uuid4().hex[:8])
    directory.mkdir(parents=True, exist_ok=False)
    environment = dict(os.environ, PYTHONUTF8="1", RUSTUP_TOOLCHAIN=selected["rust"])
    environment["PATH"] = str(Path(commands["rustup"][0]).parent) + os.pathsep + environment.get("PATH", "")
    if not args.download:
        environment["CARGO_NET_OFFLINE"] = "true"
    receipt = {"schema": "airk.contributor-bootstrap/1", "started_utc": datetime.now(timezone.utc).isoformat(),
               "commit": subprocess.check_output(commands["git"] + ["-c", f"safe.directory={ROOT.as_posix()}", "rev-parse", "HEAD"], cwd=ROOT, text=True).strip(),
               "host": {"os": platform.platform(), "architecture": platform.machine(), "logical_processors": os.cpu_count(),
                        "python": platform.python_version()},
               "limitations": ["Step durations are local observations, not controlled performance qualification.",
                               "Native build receipts retain exact source/payload identity; this commit alone does not describe dirty inputs.",
                               "No CPU model, RAM, storage type, affinity or physical I/O instrumentation is collected by this wrapper."],
               "pins": selected, "download_permitted": args.download, "dependency_policy": None,
               "steps": [], "complete": False}
    try:
        for name, command in steps(args, commands, directory):
            result = execute(name, command, directory, environment)
            receipt["steps"].append(result)
            if name == "dependency-policy":
                receipt["dependency_policy"] = dependency_policy_report(directory)
                if not result["exit_code"] and receipt["dependency_policy"].get("all_requested_checks_passed") is not True:
                    # A successful exit without a usable report cannot authorize
                    # dependency code/provisioning to run or be called verified.
                    return 2
            if result["exit_code"]:
                return result["exit_code"]
        receipt["complete"] = True
        return 0
    finally:
        if receipt["dependency_policy"] is None and (directory / "dependency-policy.json").exists():
            receipt["dependency_policy"] = dependency_policy_report(directory)
        receipt["seconds"] = round(time.monotonic() - started, 3)
        (directory / "report.json").write_text(json.dumps(receipt, indent=2) + "\n", encoding="utf-8")
        print(f"\nBootstrap {'completed' if receipt['complete'] else 'failed/incomplete'} in {receipt['seconds']}s; receipt: {directory / 'report.json'}")
        for result in receipt["steps"]:
            print(f"  {result['name']}: exit={result['exit_code']}, {result['seconds']}s, counts={result['counts'] or 'not counted; inspect log'}")
        if receipt["dependency_policy"]:
            policy = receipt["dependency_policy"]
            print(f"Dependency policy: {policy.get('status', 'unknown')}; fresh full advisory qualification: "
                  f"{policy.get('fresh_full_advisory_check', False)}; scope: {policy.get('network_mode', 'unavailable')}.")
        print("Gated/ignored tests, installed-editor and other platforms remain separately qualified.")


if __name__ == "__main__":
    try:
        raise SystemExit(main())
    except (OSError, ValueError, subprocess.SubprocessError) as error:
        print(f"Bootstrap could not complete: {error}", file=sys.stderr)
        raise SystemExit(1)
