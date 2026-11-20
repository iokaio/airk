# SPDX-License-Identifier: Apache-2.0
"""Check the distinction between repository rights and source-file provenance."""

from pathlib import Path
import runpy
import unittest


DIRECTORY = Path(__file__).resolve().parents[1] / "arbiter/analysis/public_source_admission"
AUDIT = runpy.run_path(str(DIRECTORY / "audit_contextbench_confirmation_rights.py"))


class RightsTests(unittest.TestCase):
    def basis(self, path="src/a.rs", root="MIT", package="MIT", grants=(), content=b"", notices=()):
        return AUDIT["private_source_basis"](path, root, package, set(grants), AUDIT["file_signals"](content), notices)

    def test_root_license_alone_and_unreviewed_notices_withheld(self):
        self.assertEqual(self.basis(package=None), "withhold_scope_review")
        self.assertEqual(self.basis(notices=["src/NOTICE"]), "withhold_additional_notice_review")

    def test_package_dual_license_requires_both_root_grants(self):
        self.assertEqual(self.basis(root="NOASSERTION", package="MIT OR Apache-2.0", grants=["LICENSE-MIT"]),
                         "withhold_scope_review")
        self.assertEqual(self.basis(root="NOASSERTION", package="MIT/Apache-2.0",
                                    grants=["LICENSE-MIT", "LICENSE-APACHE"]),
                         "package_dual_declaration_and_both_root_grants")

    def test_external_signals_and_generated_lockfiles_remain_unresolved(self):
        self.assertEqual(self.basis(path="lib/vendor/tool.js"), "withhold_external_source_review")
        self.assertEqual(self.basis(path="yarn.lock"), "withhold_generated_dependency_inventory")
        self.assertEqual(self.basis(content=b"/* SPDX-License-Identifier: GPL-3.0-only */"),
                         "withhold_external_source_review")

    def test_concordant_file_header(self):
        self.assertEqual(self.basis(root="Apache-2.0", package=None,
                                    content=b"// Licensed under the Apache License, Version 2.0"),
                         "file_apache_header_and_root")
        self.assertEqual(self.basis(root="MIT", content=b"Licensed under the Apache License, Version 2.0"),
                         "withhold_header_scope_review")


if __name__ == "__main__":
    unittest.main()
