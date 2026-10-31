# SPDX-License-Identifier: Apache-2.0
"""Check the locked Cargo and npm graphs against advisory databases.

Policy (docs/dependency-advisories.md): every vulnerability and every
unmaintained/unsound/yanked warning needs a dated disposition. A finding fails
unless a matching, unexpired exception covers it; an exception that no longer
matches any finding also fails, so stale approvals cannot accumulate. Scanner
or network failures exit 2 and are never reported as a clean result. No
project or dependency code runs: Cargo reads Cargo.lock and npm reads the lock
file only.
"""
import argparse
from datetime import date
import json
from pathlib import Path
import shutil
import subprocess
import sys

ROOT = Path(__file__).resolve().parents[1]
EXCEPTIONS = ROOT / ".cargo" / "audit-exceptions.json"
SCANNER_FAILURE = 2


class ScannerError(RuntimeError):
    """The scanner did not produce a usable result."""


def run_json(command, cwd):
    # Windows installs npm as npm.cmd; resolve the tool rather than using a shell.
    executable = shutil.which(command[0])
    if executable is None:
        raise ScannerError(f"{command[0]} is not installed or not on PATH")
    try:
        result = subprocess.run([executable, *command[1:]], cwd=cwd, capture_output=True, text=True, timeout=900, check=False)
    except (OSError, subprocess.TimeoutExpired) as error:
        raise ScannerError(f"{command[0]} unavailable: {error}") from error
    try:
        return json.loads(result.stdout)
    except json.JSONDecodeError as error:
        raise ScannerError(f"{' '.join(command)} produced no JSON (exit {result.returncode}): {result.stderr.strip()[:2000]}") from error


def cargo_findings(report):
    """Normalize cargo-audit JSON into findings and the advisory database identity."""
    if "vulnerabilities" not in report or "database" not in report:
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
    if "vulnerabilities" not in report or "metadata" not in report:
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


def main(argv=None):
    parser = argparse.ArgumentParser(description=__doc__.splitlines()[0])
    parser.add_argument("--exceptions", type=Path, default=EXCEPTIONS)
    parser.add_argument("--no-fetch", action="store_true", help="use the cached advisory database")
    args = parser.parse_args(argv)
    try:
        exceptions = load_exceptions(args.exceptions)
        cargo = ["cargo", "audit", "--json"] + (["--no-fetch"] if args.no_fetch else [])
        findings, database = cargo_findings(run_json(cargo, ROOT))
        lock = json.loads((ROOT / "packages" / "vscode" / "package-lock.json").read_text(encoding="utf-8"))
        findings += npm_findings(run_json(["npm", "audit", "--package-lock-only", "--json"], ROOT / "packages" / "vscode"), lock)
    except (ScannerError, ValueError, KeyError) as error:
        print(f"dependency audit could not complete: {error}", file=sys.stderr)
        return SCANNER_FAILURE
    failures, applied = evaluate(findings, exceptions, date.today())
    print(json.dumps({"advisory_database": database, "findings": len(findings), "covered_by_exception": applied,
                      "failures": failures}, indent=2))
    return 1 if failures else 0


if __name__ == "__main__":
    sys.exit(main())
