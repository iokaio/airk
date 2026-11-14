# SPDX-License-Identifier: Apache-2.0
"""Dependency-closure and consumed-input refusal controls, without package install."""
import importlib.util
from pathlib import Path
import tempfile
import unittest

SPEC = importlib.util.spec_from_file_location("retrieval_provision", Path(__file__).with_name("provision.py"))
provision = importlib.util.module_from_spec(SPEC)
SPEC.loader.exec_module(provision)


class ProvisionTests(unittest.TestCase):
    def test_unexpected_reused_environment_package_is_refused_even_before_install(self):
        with self.assertRaisesRegex(ValueError, "unreviewed distribution/version: extra"):
            provision.check_environment({"pip": "25.2", "extra": "1.0"}, {"numpy": "2.3.2"}, allow_missing=True)

    def test_wrong_version_is_refused_and_bootstrap_is_exact(self):
        for installed in [{"pip": "25.2", "numpy": "2.3.1"}, {"pip": "25.1", "numpy": "2.3.2"}]:
            with self.assertRaisesRegex(ValueError, "unreviewed distribution/version"):
                provision.check_environment(installed, {"numpy": "2.3.2"})
        provision.check_environment({"pip": "25.2", "numpy": "2.3.2"}, {"numpy": "2.3.2"})
        with self.assertRaisesRegex(ValueError, "missing reviewed"):
            provision.check_environment({"pip": "25.2"}, {"numpy": "2.3.2"})

    def test_changed_requirements_or_license_input_refuses_receipt(self):
        with tempfile.TemporaryDirectory() as directory:
            path = Path(directory) / "license.txt"
            path.write_bytes(b"original license fixture")
            seals = {str(path): provision.digest(path)}
            path.write_bytes(b"changed license fixture")
            with self.assertRaisesRegex(ValueError, "consumed inputs changed"):
                provision.check_seals(seals)
            path.unlink()
            with self.assertRaisesRegex(ValueError, "input became unavailable"):
                provision.check_seals(seals)


if __name__ == "__main__":
    unittest.main()
