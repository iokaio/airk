# SPDX-License-Identifier: Apache-2.0
"""Advisory policy: findings need dispositions, and stale or expired ones fail."""
from datetime import date
import importlib.util
import json
from pathlib import Path
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
        with tempfile.TemporaryDirectory() as directory:
            path = Path(directory) / "exceptions.json"
            path.write_text(json.dumps({"schema": 1, "exceptions": []}), encoding="utf-8")
            with mock.patch.object(MODULE, "run_json", side_effect=MODULE.ScannerError("offline")):
                self.assertEqual(MODULE.main(["--exceptions", str(path)]), MODULE.SCANNER_FAILURE)
            with self.assertRaises(MODULE.ScannerError):
                MODULE.cargo_findings({"warnings": {}})
            with mock.patch.object(MODULE, "run_json", side_effect=[{"database": {}, "vulnerabilities": {"list": []}, "warnings": {}}, NPM]):
                self.assertEqual(MODULE.main(["--exceptions", str(path)]), 0)

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


if __name__ == "__main__":
    unittest.main()
