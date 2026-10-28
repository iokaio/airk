# SPDX-License-Identifier: Apache-2.0
"""Keep installer smoke expectations aligned with platform login ownership."""
import importlib.util
from pathlib import Path
import unittest

spec = importlib.util.spec_from_file_location(
    "service_install_check", Path(__file__).parents[1] / "check-service-install.py")
smoke = importlib.util.module_from_spec(spec)
spec.loader.exec_module(smoke)


class ServiceInstallCheckTests(unittest.TestCase):
    def test_mac_requires_app_registration_and_fixed_channel_entry(self):
        plan = {"id": "airksvc-fixture", "platform": "smappservice_user",
                "arguments": ["runtime", "login-foreground", "--channel", "stable"]}
        smoke.check_login_plan(plan, Path("fixture"), "darwin")
        for change in ({"id": "airk-legacy"}, {"platform": "launchagent"},
                       {"arguments": ["runtime", "foreground", "--install-dir", "fixture"]},
                       {"arguments": ["runtime", "login-foreground", "--channel", "preview"]}):
            with self.subTest(change=change), self.assertRaises(RuntimeError):
                smoke.check_login_plan({**plan, **change}, Path("fixture"), "darwin")

    def test_windows_and_linux_require_exact_installation_dispatch(self):
        install = Path(__file__).parent.resolve()
        plan = {"id": "airksvc-fixture", "arguments": ["runtime", "foreground", "--install-dir", str(install)]}
        for platform in ("win32", "linux"):
            with self.subTest(platform=platform):
                smoke.check_login_plan(plan, install, platform)
                for change in ({"id": "airk-legacy"},
                               {"arguments": ["runtime", "foreground", "--install-dir", str(install / "other")]},
                               {"arguments": ["runtime", "login-foreground", "--channel", "stable"]}):
                    with self.subTest(change=change), self.assertRaises(RuntimeError):
                        smoke.check_login_plan({**plan, **change}, install, platform)


if __name__ == "__main__":
    unittest.main()
