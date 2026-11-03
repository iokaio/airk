# SPDX-License-Identifier: Apache-2.0
"""Advisory policy: findings need dispositions, and stale or expired ones fail."""
from datetime import date
from contextlib import ExitStack, redirect_stderr, redirect_stdout
import importlib.util
import io
import json
import os
from pathlib import Path
import subprocess
import tempfile
import unittest
from unittest import mock

SCRIPT = Path(__file__).resolve().parents[1] / "dependency-audit.py"
SPEC = importlib.util.spec_from_file_location("dependency_audit", SCRIPT)
MODULE = importlib.util.module_from_spec(SPEC)
SPEC.loader.exec_module(MODULE)

CARGO = {"database": {"advisory-count": 1, "last-commit": "abc", "last-updated": "2026-10-30T00:00:00Z"},
         "vulnerabilities": {"found": True, "count": 1, "list": [
             {"advisory": {"id": "RUSTSEC-2099-0001"}, "package": {"name": "fictional", "version": "1.0.0"}}]},
         "warnings": {"unmaintained": [{"advisory": {"id": "RUSTSEC-2099-0002"}, "package": {"name": "older", "version": "0.1.0"}}]}}
NPM = {"metadata": {"vulnerabilities": {"total": 0}}, "vulnerabilities": {}}


def exception(**changes):
    value = {"ecosystem": "cargo", "package": "fictional", "version": "1.0.0", "id": "RUSTSEC-2099-0001", "scope": "test",
             "evidence": "fixture", "reviewer": "maintainer", "reviewed": "2026-10-30", "review_by": "2027-01-01"}
    value.update(changes)
    return value


class AuditPolicyTests(unittest.TestCase):
    def test_known_findings_fail_until_each_has_a_scoped_exception(self):
        findings, database = MODULE.cargo_findings(CARGO)
        self.assertEqual(database["last-commit"], "abc")
        failures, _ = MODULE.evaluate(findings, [], date(2026, 10, 30))
        self.assertEqual(len(failures), 2)
        failures, applied = MODULE.evaluate(findings, [exception()], date(2026, 10, 30))
        self.assertEqual(len(applied), 1)
        self.assertTrue(any("RUSTSEC-2099-0002" in failure for failure in failures), "warnings also need a disposition")

    def test_exceptions_expire_and_stale_exceptions_fail(self):
        findings, _ = MODULE.cargo_findings(CARGO)
        failures, _ = MODULE.evaluate(findings[:1], [exception(review_by="2026-10-01")], date(2026, 10, 30))
        self.assertTrue(any("expired" in failure for failure in failures))
        failures, _ = MODULE.evaluate(findings[:1], [exception(), exception(version="0.9.0")], date(2026, 10, 30))
        self.assertTrue(any("stale exception" in failure for failure in failures))
        # A changed version no longer inherits an approval for the old one.
        failures, _ = MODULE.evaluate([dict(findings[0], version="1.0.1")], [exception()], date(2026, 10, 30))
        self.assertTrue(any("no disposition" in failure for failure in failures))

    def test_scanner_failures_are_distinct_from_a_clean_result(self):
        result = MODULE.checked("fixture", lambda: (_ for _ in ()).throw(MODULE.ScannerError("offline")))
        self.assertEqual(result["status"], "unavailable")
        with self.assertRaises(MODULE.ScannerError):
            MODULE.cargo_findings({"warnings": {}})
        for parser in (MODULE.cargo_findings, MODULE.npm_findings):
            with self.assertRaises(MODULE.ScannerError):
                parser({"error": {"message": "offline"}, "vulnerabilities": {}, "metadata": {}, "database": {}})

    def test_npm_findings_use_the_installed_version_and_advisory_id(self):
        report = {"metadata": {}, "vulnerabilities": {"minimatch": {"nodes": ["node_modules/minimatch"], "range": "<11.0.1",
                  "via": [{"url": "https://github.com/advisories/GHSA-g3hv-75wg-m249", "severity": "moderate"}]}}}
        lock = {"packages": {"node_modules/minimatch": {"version": "10.2.6"}}}
        (finding,) = MODULE.npm_findings(report, lock)
        self.assertEqual((finding["id"], finding["package"], finding["version"]), ("GHSA-g3hv-75wg-m249", "minimatch", "10.2.6"))
        exception = {"ecosystem": "npm", "package": "minimatch", "version": "10.2.6", "id": "GHSA-g3hv-75wg-m249", "review_by": "2027-01-01"}
        failures, applied = MODULE.evaluate([finding], [exception], date(2026, 10, 30))
        self.assertEqual((failures, len(applied)), ([], 1))
        lock["packages"]["node_modules/minimatch"]["version"] = "10.2.7"
        failures, _ = MODULE.evaluate(MODULE.npm_findings(report, lock), [exception], date(2026, 10, 30))
        self.assertTrue(any("no disposition" in failure for failure in failures), "a new version needs review again")

    def test_checked_in_exceptions_are_complete_and_dated(self):
        exceptions = MODULE.load_exceptions(MODULE.EXCEPTIONS)
        self.assertTrue(exceptions)
        for item in exceptions:
            self.assertLess(date.fromisoformat(item["reviewed"]), date.fromisoformat(item["review_by"]))


class CombinedGateTests(unittest.TestCase):
    def invoke(self, arguments=(), exceptions=(), overrides=None, snapshots=None):
        actions = {"source_review_check": {"status": "passed"},
                   "cargo_audit_check": {"status": "passed", "findings": [], "advisory_database": CARGO["database"]},
                   "npm_audit_check": {"status": "passed", "findings": []},
                   "deny_check": {"status": "passed"}, "vet_check": {"status": "passed"}}
        actions.update(overrides or {})
        output = io.StringIO()
        with ExitStack() as stack:
            stack.enter_context(mock.patch.object(MODULE, "load_exceptions", return_value=list(exceptions)))
            stack.enter_context(mock.patch.object(MODULE, "input_snapshot", side_effect=snapshots or [{"lock": "same"}, {"lock": "same"}]))
            calls = {name: stack.enter_context(mock.patch.object(MODULE, name, side_effect=value if isinstance(value, Exception) else None,
                                                                 return_value=value)) for name, value in actions.items()}
            stack.enter_context(redirect_stdout(output))
            code = MODULE.main(list(arguments))
        return code, json.loads(output.getvalue()), calls

    def test_all_requested_checks_must_pass_and_partial_scans_cannot_be_fresh(self):
        code, report, calls = self.invoke(["--require-fresh"])
        self.assertEqual(code, 0)
        self.assertTrue(report["fresh_full_advisory_check"])
        self.assertEqual(set(value["status"] for value in report["checks"].values()), {"passed"})
        self.assertTrue(all(call.called for call in calls.values()))
        code, report, _ = self.invoke(["--no-fetch"])
        self.assertEqual((code, report["status"], report["fresh_full_advisory_check"]), (0, "limited", False))
        with redirect_stderr(io.StringIO()), self.assertRaises(SystemExit) as rejected:
            MODULE.main(["--no-fetch", "--require-fresh"])
        self.assertEqual(rejected.exception.code, 2)

    def test_offline_never_calls_npm_or_labels_unchecked_npm_exceptions_stale(self):
        npm_exception = exception(ecosystem="npm", package="fictional-npm", id="GHSA-fixture")
        code, report, calls = self.invoke(["--offline"], exceptions=[npm_exception])
        self.assertEqual((code, report["status"], report["fresh_full_advisory_check"]), (0, "limited", False))
        calls["npm_audit_check"].assert_not_called()
        self.assertEqual(report["checks"]["npm_audit"]["status"], "not_run")
        calls["cargo_audit_check"].assert_called_once_with(True)
        calls["vet_check"].assert_called_once_with(True)
        self.assertIn("npm exceptions not evaluated", report["checks"]["advisory_dispositions"]["scope"])
        code, report, _ = self.invoke(["--offline"], overrides={"deny_check": MODULE.ScannerError("missing cached database")})
        self.assertEqual((code, report["status"]), (2, "unavailable"))
        with redirect_stderr(io.StringIO()), self.assertRaises(SystemExit):
            MODULE.main(["--offline", "--require-fresh"])

    def test_new_dependency_or_same_version_fork_revision_blocks_metadata_before_fetch(self):
        for finding in ("new registry dependency", "same-version Git revision changed"):
            code, report, calls = self.invoke(overrides={"source_review_check": {"status": "failed", "failures": [finding]}})
            self.assertEqual(code, 1)
            calls["deny_check"].assert_not_called()
            calls["vet_check"].assert_not_called()
            self.assertEqual(report["checks"]["cargo_deny"]["status"], "blocked")
            calls["npm_audit_check"].assert_called_once()

    def test_failure_of_one_checker_keeps_other_results_and_never_hides_stale_exceptions(self):
        code, report, calls = self.invoke(overrides={"npm_audit_check": MODULE.ScannerError("registry unavailable")})
        self.assertEqual(code, 2)
        calls["deny_check"].assert_called_once()
        calls["vet_check"].assert_called_once()
        self.assertEqual(report["checks"]["advisory_dispositions"]["status"], "blocked")
        code, report, _ = self.invoke(exceptions=[exception()])
        self.assertEqual(code, 1)
        self.assertIn("stale exception", report["failures"][0])

    def test_metadata_or_review_store_mutation_is_unavailable_not_a_pass(self):
        for name in ("Cargo.lock", "crates/airk-core/Cargo.toml", "supply-chain/config.toml", "supply-chain/imports.lock"):
            code, report, _ = self.invoke(snapshots=[{name: "before"}, {name: "after"}])
            self.assertEqual(code, 2)
            self.assertEqual(report["checks"]["input_integrity"]["changed_inputs"], [name])
            self.assertFalse(report["all_requested_checks_passed"])


class PolicyToolContracts(unittest.TestCase):
    def test_selective_workflow_keeps_tool_pins_and_fresh_canonical_gate(self):
        workflow = (MODULE.ROOT / ".github/workflows/dependency-audit.yml").read_text()
        for name, version in MODULE.PINNED_TOOLS.items():
            self.assertIn(f"{name}@{version}", workflow)
        for path in ("Cargo.toml", "Cargo.lock", "'crates/**/Cargo.toml'", "deny.toml", "'supply-chain/**'", ".cargo/config.toml",
                     "scripts/tests/test_dependency_audit.py", "scripts/tests/test_supply_chain.py"):
            self.assertEqual(workflow.count("- " + path + "\n"), 2, path)
        self.assertIn("dependency-audit.py --require-fresh --output", workflow)
        # The checker deliberately uses `rustup run` without auto-installation.
        # A fresh hosted runner must provision the repository pin beforehand.
        self.assertLess(workflow.index("run: rustup show"), workflow.index("- name: Install pinned dependency policy tools"))
        self.assertNotIn("matrix:", workflow)

    def test_cached_audit_explicitly_disables_yanked_and_requires_database_identity(self):
        with mock.patch.object(MODULE, "require_pinned_tool"), mock.patch.object(MODULE, "run_json", return_value=CARGO) as run:
            result = MODULE.cargo_audit_check(True)
            self.assertEqual(result["yanked_check"], "not run")
            self.assertTrue({"--no-fetch", "--no-yanked"}.issubset(run.call_args.args[0]))
        with mock.patch.object(MODULE, "require_pinned_tool"), mock.patch.object(MODULE, "run_json", return_value={**CARGO, "database": {}}):
            with self.assertRaises(MODULE.ScannerError):
                MODULE.cargo_audit_check(False)
        cached = {**CARGO, "database": {"advisory-count": 1, "last-commit": None, "last-updated": None}}
        observed = [subprocess.CompletedProcess([], 0, "a" * 40, ""),
                    subprocess.CompletedProcess([], 0, "2026-11-03T00:00:00Z", "")]
        with mock.patch.object(MODULE, "require_pinned_tool"), mock.patch.object(MODULE, "run_json", return_value=cached), mock.patch.object(MODULE, "run_process", side_effect=observed):
            result = MODULE.cargo_audit_check(True)
            self.assertEqual(result["advisory_database"]["identity_source"], "cached checkout; not refreshed")
            self.assertEqual(result["freshness"], "cached, not refreshed")

    def deny_policy(self, root):
        targets = ", ".join(json.dumps(value) for value in sorted(MODULE.SUPPORTED_TARGETS))
        (root / "deny.toml").write_text(f'[graph]\ntargets=[{targets}]\nall-features=true\nexclude-dev=false\nexclude-unpublished=false\n[advisories]\nignore=[]\n')

    def test_deny_requires_typed_complete_summary_and_honors_policy_exit_bitmasks(self):
        with tempfile.TemporaryDirectory() as directory:
            root = Path(directory)
            self.deny_policy(root)
            for name, code in (("sources", 8), ("licenses", 4), ("advisories", 1), (None, 0)):
                summary = {check: {"errors": int(check == name)} for check in ("advisories", "bans", "licenses", "sources")}
                result = subprocess.CompletedProcess([], code, "", json.dumps({"type": "summary", "fields": summary}))
                with mock.patch.object(MODULE, "ROOT", root), mock.patch.object(MODULE, "require_pinned_tool"), mock.patch.object(MODULE, "run_process", return_value=result) as run:
                    observed = MODULE.deny_check(True, [])
                    self.assertEqual(observed["status"], "failed" if code else "passed")
                    command = run.call_args.args[0]
                    self.assertIn("--offline", command)
                    self.assertEqual(command[-4:], ["advisories", "bans", "licenses", "sources"])
            unavailable = subprocess.CompletedProcess([], 1, "", json.dumps({"type": "log", "fields": {"level": "ERROR", "message": "database unavailable"}}))
            with mock.patch.object(MODULE, "ROOT", root), mock.patch.object(MODULE, "require_pinned_tool"), mock.patch.object(MODULE, "run_process", return_value=unavailable):
                with self.assertRaises(MODULE.ScannerError):
                    MODULE.deny_check(False, [])

    def test_vet_uses_locked_imports_complete_graph_and_typed_negative_conclusion(self):
        with tempfile.TemporaryDirectory() as directory:
            root = Path(directory)
            revision = "a" * 40
            (root / "Cargo.lock").write_text('[[package]]\nname="fictional"\nversion="1.0.0"\nsource="git+https://example.test/fork#' + revision + '"\n')
            good = {"conclusion": "success", "vetted_fully": [], "vetted_partially": [],
                    "vetted_with_exemptions": [{"name": "fictional", "version": "1.0.0@git:" + revision}]}
            cases = [(good, 0, "passed"), ({"conclusion": "fail (vetting)", "failures": [{"name": "fictional"}]}, -1, "failed")]
            for report, code, expected in cases:
                result = subprocess.CompletedProcess([], code, json.dumps(report), "")
                with mock.patch.object(MODULE, "ROOT", root), mock.patch.object(MODULE, "require_pinned_tool"), mock.patch.object(MODULE, "run_process", return_value=result) as run:
                    self.assertEqual(MODULE.vet_check(True)["status"], expected)
                    command = run.call_args.args[0]
                    self.assertTrue({"--frozen", "--locked", "--no-minimize-exemptions", "--no-registry-suggestions"}.issubset(command))
            for report in ({}, {**good, "vetted_with_exemptions": []}, {"error": {"message": "metadata unavailable"}}):
                with mock.patch.object(MODULE, "ROOT", root), mock.patch.object(MODULE, "require_pinned_tool"), mock.patch.object(MODULE, "run_process", return_value=subprocess.CompletedProcess([], 0, json.dumps(report), "")):
                    with self.assertRaises(MODULE.ScannerError):
                        MODULE.vet_check(False)

    def test_tool_selection_is_private_exact_and_never_auto_installs_rust(self):
        original_path = os.environ.get("PATH")
        selected = MODULE.tool_environment()
        self.assertEqual(os.environ.get("PATH"), original_path)
        self.assertIn(str(MODULE.ROOT / "artifacts/tools/cargo-deny/bin"), selected["PATH"])
        with mock.patch.object(MODULE, "run_process", return_value=subprocess.CompletedProcess([], 0, "cargo-deny 0.20.1\n", "")):
            with self.assertRaises(MODULE.ScannerError):
                MODULE.require_pinned_tool("cargo-deny")
        with mock.patch.object(MODULE, "run_process", return_value=subprocess.CompletedProcess([], 0, "cargo-audit-audit 0.22.2\n", "")):
            MODULE.require_pinned_tool("cargo-audit")
        with mock.patch.object(MODULE.shutil, "which", side_effect=lambda name, **_: name), mock.patch.object(MODULE.subprocess, "run", return_value=subprocess.CompletedProcess([], 0, "cargo-deny 0.20.2\n", "")) as process:
            MODULE.require_pinned_tool("cargo-deny")
            self.assertEqual(process.call_args.args[0][:5], ["rustup", "run", "1.98.0", "cargo", "deny"])
            self.assertNotIn("--install", process.call_args.args[0])


if __name__ == "__main__":
    unittest.main()
