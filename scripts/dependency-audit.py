# SPDX-License-Identifier: Apache-2.0
"""Check locked dependencies, source/license policy and frozen review coverage.

Policy (docs/dependency-advisories.md): every vulnerability and every
unmaintained/unsound/yanked warning needs a dated disposition. A finding fails
unless a matching, unexpired exception covers it; an exception that no longer
matches any finding also fails, so stale approvals cannot accumulate. Scanner
or network failures exit 2 and are never reported as a clean result. No
project or dependency build code runs. Cargo metadata/license inspection is
read-only and locked; Cargo Vet imports and baseline exceptions are never
automatically refreshed or certified by this checker.
"""
import argparse
from datetime import date, datetime, timezone
import hashlib
import importlib.util
import json
import os
from pathlib import Path
import shutil
import subprocess
import sys
import tomllib

ROOT = Path(__file__).resolve().parents[1]
EXCEPTIONS = ROOT / ".cargo" / "audit-exceptions.json"
SCANNER_FAILURE = 2
PINNED_TOOLS = {"cargo-audit": "0.22.2", "cargo-deny": "0.20.2", "cargo-vet": "0.10.3"}
SUPPORTED_TARGETS = {"x86_64-pc-windows-msvc", "x86_64-unknown-linux-gnu",
                     "aarch64-apple-darwin", "x86_64-apple-darwin"}


class ScannerError(RuntimeError):
    """The scanner did not produce a usable result."""


def tool_environment():
    """Prefer private pinned installations without changing the user's PATH."""
    environment = dict(os.environ)
    private = [str(ROOT / "artifacts" / "tools" / tool / suffix)
               for tool in PINNED_TOOLS for suffix in ("bin", "")]
    private.append(str(Path(environment.get("CARGO_HOME", Path.home() / ".cargo")) / "bin"))
    environment["PATH"] = os.pathsep.join([*private, environment.get("PATH", "")])
    environment.setdefault("RAYON_NUM_THREADS", "2")
    return environment


def run_process(command, cwd):
    # Windows installs npm as npm.cmd; resolve the tool rather than using a shell.
    environment = tool_environment()
    executable = shutil.which(command[0], path=environment["PATH"])
    if executable is None:
        raise ScannerError(f"{command[0]} is not installed or not on PATH")
    arguments = [executable, *command[1:]]
    if command[0] == "cargo":
        rustup = shutil.which("rustup", path=environment["PATH"])
        if rustup is None:
            raise ScannerError("rustup is required to select the installed pinned toolchain without automatic installation")
        channel = tomllib.loads((ROOT / "rust-toolchain.toml").read_text(encoding="utf-8"))["toolchain"]["channel"]
        # `rustup run` installs nothing unless --install is explicitly supplied.
        arguments = [rustup, "run", channel, "cargo", *command[1:]]
    try:
        return subprocess.run(arguments, cwd=cwd, env=environment,
                              capture_output=True, text=True, timeout=900, check=False)
    except (OSError, subprocess.TimeoutExpired) as error:
        raise ScannerError(f"{command[0]} unavailable: {error}") from error

def run_json(command, cwd):
    result = run_process(command, cwd)
    if result.returncode not in (0, 1):
        raise ScannerError(f"{command[0]} scanner failed (exit {result.returncode}): {result.stderr[-2000:]}")
    try:
        value = json.loads(result.stdout)
        if not isinstance(value, dict):
            raise ScannerError(f"{command[0]} returned a non-object report")
        return value
    except json.JSONDecodeError as error:
        raise ScannerError(f"{' '.join(command)} produced no JSON (exit {result.returncode}): {result.stderr.strip()[:2000]}") from error


def require_pinned_tool(name):
    if name == "cargo-vet":
        configured = json.loads((ROOT / "supply-chain/tool.json").read_text(encoding="utf-8"))
        if configured.get("version") != PINNED_TOOLS[name]:
            raise ScannerError("Cargo Vet distribution pin differs from the runner pin")
    result = run_process(["cargo", name.removeprefix("cargo-"), "--version"], ROOT)
    expected = f"{name} {PINNED_TOOLS[name]}"
    # Cargo Audit's clap subcommand identifies `cargo audit --version` this way.
    reported_name = "cargo-audit-audit" if name == "cargo-audit" else name
    if result.returncode or result.stdout.strip() != f"{reported_name} {PINNED_TOOLS[name]}":
        raise ScannerError(f"Expected {expected}; found {result.stdout.strip()[:200] or 'no usable version'}")


def input_snapshot(exceptions):
    paths = {ROOT / "Cargo.toml", ROOT / "Cargo.lock", ROOT / "rust-toolchain.toml", ROOT / "deny.toml", exceptions,
             ROOT / ".cargo/config.toml",
             ROOT / "scripts/dependency-audit.py", ROOT / ".github/workflows/dependency-audit.yml",
             ROOT / "packages/vscode/package.json", ROOT / "packages/vscode/package-lock.json"}
    paths.update((ROOT / "crates").glob("*/Cargo.toml"))
    store = ROOT / "supply-chain"
    if store.exists():
        paths.update(path for path in store.rglob("*") if path.suffix in (".toml", ".json", ".lock", ".py"))
    snapshot = {}
    for path in paths:
        if path.is_symlink():
            raise ScannerError(f"Dependency policy input is linked: {path}")
        snapshot[str(path.resolve())] = hashlib.sha256(path.read_bytes()).hexdigest() if path.is_file() else None
    return snapshot


def source_review_check():
    module_path = ROOT / "supply-chain/guard.py"
    if not module_path.is_file():
        raise ScannerError("Committed dependency source policy checker is missing")
    specification = importlib.util.spec_from_file_location("airk_dependency_source_policy", module_path)
    module = importlib.util.module_from_spec(specification)
    specification.loader.exec_module(module)
    try:
        result = module.validate_lock_sources(ROOT / "Cargo.lock", ROOT / "supply-chain/source-policy.json")
    except ValueError as error:
        return {"status": "failed", "failures": [str(error)]}
    return {"status": "passed", "source_policy": result}


def deny_check(no_fetch, exceptions):
    require_pinned_tool("cargo-deny")
    policy = tomllib.loads((ROOT / "deny.toml").read_text(encoding="utf-8"))
    graph = policy.get("graph", {})
    if (set(graph.get("targets", [])) != SUPPORTED_TARGETS or graph.get("all-features") is not True
            or graph.get("exclude-dev") is not False or graph.get("exclude-unpublished") is not False):
        return {"status": "failed", "failures": ["cargo-deny must cover the whole workspace, all features and all four shipped targets"]}
    ignored = {entry["id"] if isinstance(entry, dict) else entry for entry in policy.get("advisories", {}).get("ignore", [])}
    scoped = {entry["id"] for entry in exceptions if entry["ecosystem"] == "cargo"}
    if ignored != scoped:
        return {"status": "failed", "failures": ["cargo-deny advisory ignores must exactly match the dated Cargo exception IDs"]}
    command = ["cargo", "deny", "--format", "json", "--locked", "--workspace", "--all-features",
               "--config", str(ROOT / "deny.toml")]
    if no_fetch:
        command.append("--offline")
    command += ["check", "--show-stats", "advisories", "bans", "licenses", "sources"]
    result = run_process(command, ROOT)
    summaries, diagnostics = [], []
    for line in result.stderr.splitlines():
        try:
            item = json.loads(line)
        except json.JSONDecodeError:
            continue
        if isinstance(item, dict) and item.get("type") == "summary":
            summaries.append(item.get("fields"))
        elif isinstance(item, dict) and item.get("type") == "diagnostic":
            diagnostics.append(item.get("fields"))
    checks = {"advisories", "bans", "licenses", "sources"}
    if len(summaries) != 1 or not isinstance(summaries[0], dict) or set(summaries[0]) != checks:
        raise ScannerError(f"cargo-deny did not complete all four policy checks (exit {result.returncode}): {result.stderr[-2000:]}")
    summary = summaries[0]
    if any(not isinstance(summary[name], dict) or type(summary[name].get("errors")) is not int
           or summary[name]["errors"] < 0 for name in checks):
        raise ScannerError("cargo-deny returned invalid policy counts")
    errors = sum(summary[name]["errors"] for name in checks)
    expected_exit = sum(bit for name, bit in [("advisories", 1), ("bans", 2), ("licenses", 4), ("sources", 8)]
                        if summary[name]["errors"])
    if result.returncode != expected_exit:
        raise ScannerError("cargo-deny exit status disagrees with its completed policy summary")
    return {"status": "failed" if errors else "passed", "command": command,
            "exit_code": result.returncode, "summary": summary, "diagnostics": diagnostics,
            "advisories_refreshed": not no_fetch}


def vet_check(no_fetch):
    require_pinned_tool("cargo-vet")
    command = ["cargo", "vet", "check", "--locked", "--no-minimize-exemptions", "--no-registry-suggestions",
               "--cache-dir", str(ROOT / "artifacts/dependency-policy/vet-cache"), "--output-format=json"]
    if no_fetch:
        command.append("--frozen")
    result = run_process(command, ROOT)
    try:
        report = json.loads(result.stdout)
    except json.JSONDecodeError as error:
        raise ScannerError(f"cargo-vet produced no usable JSON (exit {result.returncode}): {result.stderr[-2000:]}") from error
    if not isinstance(report, dict) or report.get("error"):
        raise ScannerError("cargo-vet did not produce a usable check result")
    if report.get("conclusion") == "success" and result.returncode == 0:
        fields = ("vetted_fully", "vetted_partially", "vetted_with_exemptions")
        if any(not isinstance(report.get(field), list) for field in fields):
            raise ScannerError("cargo-vet success report omits review coverage")
        observed = {(entry["name"], entry["version"])
                    for field in fields for entry in report[field]}
        lock = tomllib.loads((ROOT / "Cargo.lock").read_text(encoding="utf-8"))
        expected = {(entry["name"], entry["version"] + ("@git:" + entry["source"].rsplit("#", 1)[-1]
                     if entry["source"].startswith("git+") else ""))
                    for entry in lock["package"] if entry.get("source")}
        if observed != expected:
            raise ScannerError("cargo-vet reported coverage does not match the complete locked external graph")
    elif report.get("conclusion") == "fail (vetting)" and result.returncode != 0 and isinstance(report.get("failures"), list) and report["failures"]:
        pass  # cargo-vet uses exit -1 for a normal unsatisfied review policy.
    else:
        raise ScannerError("cargo-vet check conclusion is missing or disagrees with its exit status")
    return {"status": "passed" if result.returncode == 0 else "failed", "command": command,
            "report": report, "imports": "committed and locked", "graph": "workspace, all features, no target filter"}


def cargo_findings(report):
    """Normalize cargo-audit JSON into findings and the advisory database identity."""
    if (not isinstance(report, dict) or report.get("error") or not isinstance(report.get("vulnerabilities"), dict)
            or not isinstance(report.get("database"), dict)):
        raise ScannerError("cargo audit report is missing its vulnerability or database sections")
    findings = []
    for entry in report["vulnerabilities"].get("list", []):
        findings.append({"ecosystem": "cargo", "kind": "vulnerability", "id": entry["advisory"]["id"],
                         "package": entry["package"]["name"], "version": entry["package"]["version"]})
    for kind, entries in (report.get("warnings") or {}).items():
        for entry in entries:
            advisory = entry.get("advisory") or {}
            findings.append({"ecosystem": "cargo", "kind": kind, "id": advisory.get("id") or f"{kind}:{entry['package']['name']}",
                             "package": entry["package"]["name"], "version": entry["package"]["version"]})
    return findings, report["database"]


def npm_findings(report, lock=None):
    """Exceptions match the installed version from the lockfile, never the advisory's affected range."""
    if (not isinstance(report, dict) or report.get("error") or not isinstance(report.get("vulnerabilities"), dict)
            or not isinstance(report.get("metadata"), dict)):
        raise ScannerError("npm audit report is missing its vulnerability or metadata sections")
    packages = (lock or {}).get("packages", {})
    findings = []
    for name, entry in report["vulnerabilities"].items():
        versions = sorted({packages.get(node, {}).get("version", "") for node in entry.get("nodes", [])} - {""}) or [entry.get("range", "*")]
        for via in entry.get("via", []):
            if isinstance(via, dict):
                url = via.get("url") or ""
                identifier = url.rsplit("/", 1)[-1] if "/advisories/" in url else (url or via.get("title") or name)
                for version in versions:
                    findings.append({"ecosystem": "npm", "kind": "vulnerability", "id": identifier,
                                     "package": name, "version": version, "severity": via.get("severity")})
    return findings


def evaluate(findings, exceptions, today):
    """Return (failures, applied). Exceptions match ecosystem, package, version and advisory id."""
    failures, applied, used = [], [], set()
    for finding in findings:
        match = None
        for index, exception in enumerate(exceptions):
            if all(exception.get(key) == finding[key] for key in ("ecosystem", "package", "version", "id")):
                match = index
                break
        if match is None:
            failures.append(f"{finding['ecosystem']} {finding['kind']} {finding['id']} in {finding['package']} {finding['version']} has no disposition")
            continue
        exception = exceptions[match]
        used.add(match)
        if date.fromisoformat(exception["review_by"]) < today:
            failures.append(f"exception for {finding['id']} in {finding['package']} expired on {exception['review_by']}; review it again")
        else:
            applied.append(finding)
    for index, exception in enumerate(exceptions):
        if index not in used:
            failures.append(f"stale exception for {exception.get('id')} in {exception.get('package')} {exception.get('version')} no longer matches; remove it")
    return failures, applied


def load_exceptions(path):
    data = json.loads(path.read_text(encoding="utf-8"))
    if data.get("schema") != 1 or not isinstance(data.get("exceptions"), list):
        raise ValueError("audit exceptions need schema 1 and an exceptions list")
    required = {"ecosystem", "package", "version", "id", "scope", "evidence", "reviewer", "reviewed", "review_by"}
    for exception in data["exceptions"]:
        missing = required - exception.keys()
        if missing:
            raise ValueError(f"audit exception for {exception.get('id')} is missing {sorted(missing)}")
        date.fromisoformat(exception["reviewed"])
        date.fromisoformat(exception["review_by"])
    return data["exceptions"]


def cargo_audit_check(no_fetch):
    require_pinned_tool("cargo-audit")
    command = ["cargo", "audit", "--json", "--db", str(ROOT / "artifacts/dependency-policy/cargo-audit-db")]
    if no_fetch:
        command += ["--no-fetch", "--no-yanked"]
    findings, database = cargo_findings(run_json(command, ROOT))
    # Cargo Audit intentionally omits commit metadata when database fetching is
    # disabled. Read the existing checkout identity; this never implies freshness.
    if no_fetch and (not database.get("last-commit") or not database.get("last-updated")):
        identity = {}
        for key, arguments in (("last-commit", ["rev-parse", "HEAD"]),
                               ("last-updated", ["show", "-s", "--format=%cI", "HEAD"])):
            observed = run_process(["git", "-C", str(ROOT / "artifacts/dependency-policy/cargo-audit-db"), *arguments], ROOT)
            if observed.returncode or not observed.stdout.strip():
                raise ScannerError("Cannot identify the cached cargo-audit advisory checkout")
            identity[key] = observed.stdout.strip()
        database = {**database, **identity, "identity_source": "cached checkout; not refreshed"}
    if (not database.get("last-commit") or not database.get("last-updated")
            or type(database.get("advisory-count")) is not int):
        raise ScannerError("cargo-audit returned no usable advisory database identity")
    return {"status": "passed", "command": command, "findings": findings, "advisory_database": database,
            "freshness": "cached, not refreshed" if no_fetch else "fetch requested and completed",
            "yanked_check": "not run" if no_fetch else "requested"}


def npm_audit_check():
    lock = json.loads((ROOT / "packages/vscode/package-lock.json").read_text(encoding="utf-8"))
    command = ["npm", "audit", "--package-lock-only", "--json"]
    findings = npm_findings(run_json(command, ROOT / "packages/vscode"), lock)
    return {"status": "passed", "command": command, "findings": findings, "freshness": "registry response observed"}


def checked(name, action):
    try:
        return action()
    except (ScannerError, OSError, ValueError, KeyError, TypeError) as error:
        return {"status": "unavailable", "error": f"{name}: {error}"}


def main(argv=None):
    parser = argparse.ArgumentParser(description=__doc__.splitlines()[0])
    parser.add_argument("--exceptions", type=Path, default=EXCEPTIONS)
    parser.add_argument("--no-fetch", action="store_true", help="use cached Rust advisories and frozen vet; npm still needs its registry feed")
    parser.add_argument("--offline", action="store_true", help="strict cached Rust-only checks; npm is explicitly not run and qualification remains limited")
    parser.add_argument("--require-fresh", action="store_true", help="refuse partial cached advisory qualification (used by CI)")
    parser.add_argument("--output", type=Path, help="also save the complete JSON result to this file")
    args = parser.parse_args(argv)
    if args.require_fresh and (args.no_fetch or args.offline):
        parser.error("--require-fresh cannot be combined with --no-fetch or --offline")
    try:
        before = input_snapshot(args.exceptions)
        exceptions = load_exceptions(args.exceptions)
        if args.output and (str(args.output.resolve()) in before or args.output.resolve().is_relative_to(ROOT / "supply-chain")):
            raise ScannerError("Audit output cannot overwrite a protected dependency policy input")
    except (ScannerError, OSError, ValueError, KeyError) as error:
        print(f"dependency audit could not complete: {error}", file=sys.stderr)
        return SCANNER_FAILURE
    results = {"source_review": checked("source review", source_review_check)}
    cached = args.no_fetch or args.offline
    results["cargo_audit"] = checked("cargo audit", lambda: cargo_audit_check(cached))
    results["npm_audit"] = ({"status": "not_run", "reason": "Strict offline mode does not contact the npm advisory feed"}
                            if args.offline else checked("npm audit", npm_audit_check))
    if results["source_review"]["status"] == "passed":
        results["cargo_deny"] = checked("cargo deny", lambda: deny_check(cached, exceptions))
        results["cargo_vet"] = checked("cargo vet", lambda: vet_check(cached))
    else:
        for name in ("cargo_deny", "cargo_vet"):
            results[name] = {"status": "blocked", "reason": "Resolve the source policy before Cargo metadata may fetch or inspect new sources"}
    findings = [finding for name in ("cargo_audit", "npm_audit") for finding in results[name].get("findings", [])]
    failures, applied = [], []
    advisory_checks = ("cargo_audit",) if args.offline else ("cargo_audit", "npm_audit")
    if all(results[name]["status"] == "passed" for name in advisory_checks):
        scoped_exceptions = [entry for entry in exceptions if entry["ecosystem"] == "cargo"] if args.offline else exceptions
        failures, applied = evaluate(findings, scoped_exceptions, datetime.now(timezone.utc).date())
        results["advisory_dispositions"] = {"status": "failed" if failures else "passed", "failures": failures,
                                             "scope": "Cargo only; npm exceptions not evaluated" if args.offline else "Cargo and npm"}
    else:
        results["advisory_dispositions"] = {"status": "blocked", "reason": "Cannot decide stale exceptions from an incomplete advisory scan"}
    after = checked("input integrity", lambda: {"status": "passed", "snapshot": input_snapshot(args.exceptions)})
    if after["status"] == "passed":
        changed = sorted(name for name in before.keys() | after["snapshot"].keys()
                         if before.get(name) != after["snapshot"].get(name))
        results["input_integrity"] = {"status": "unavailable" if changed else "passed", "changed_inputs": changed,
                                      "reason": "Checkers must not update manifests, locks or review policy"}
    else:
        results["input_integrity"] = after
    statuses = {result["status"] for name, result in results.items() if not (args.offline and name == "npm_audit")}
    exit_code = SCANNER_FAILURE if "unavailable" in statuses else 1 if statuses != {"passed"} else 0
    report = {"schema": "airk.dependency-policy-check/1", "observed_utc": datetime.now(timezone.utc).isoformat(),
              "status": "unavailable" if exit_code == 2 else "failed" if exit_code else "limited" if cached else "passed",
              "all_requested_checks_passed": exit_code == 0,
              "fresh_full_advisory_check": exit_code == 0 and not cached,
              "network_mode": "strict offline; npm not checked" if args.offline else "cached Rust, live npm" if cached else "live advisories, locked vet imports",
              "review_baseline": "Existing exemptions/source baselines are not independent dependency audits",
              "tool_pins": PINNED_TOOLS, "input_sha256": before, "checks": results,
              "advisory_database": results["cargo_audit"].get("advisory_database"), "findings": len(findings),
              "covered_by_exception": applied, "failures": failures}
    encoded = json.dumps(report, indent=2) + "\n"
    if args.output:
        args.output.parent.mkdir(parents=True, exist_ok=True)
        args.output.write_text(encoded, encoding="utf-8")
    print(encoded, end="")
    return exit_code


if __name__ == "__main__":
    sys.exit(main())
