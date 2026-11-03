# SPDX-License-Identifier: Apache-2.0
"""Keep installer smoke expectations aligned with platform login ownership."""
import importlib.util
import json
import os
from pathlib import Path
import shutil
import tempfile
import unittest

spec = importlib.util.spec_from_file_location(
    "service_install_check", Path(__file__).parents[1] / "check-service-install.py")
smoke = importlib.util.module_from_spec(spec)
spec.loader.exec_module(smoke)


class ServiceInstallCheckTests(unittest.TestCase):
    def images(self, root, operating_system, version):
        payload, installed = root / "source", root / "installed"
        payload.mkdir()
        builder = smoke.runtime_builder()
        main, helper = builder.runtime_names(operating_system)
        launcher = builder.launcher_name(operating_system, version)
        for name in {main, helper, launcher}:
            image = payload / name
            image.parent.mkdir(parents=True, exist_ok=True)
            image.write_bytes(("Original approved fixture: " + name).encode())
            image.chmod(0o700)
        inventory = {"os": operating_system, "version": version,
                     "files": {name: smoke.sha256(payload / name) for name in {main, helper, launcher}}}
        (payload / "airk-install.json").write_text(json.dumps(inventory), encoding="utf-8")
        shutil.copytree(payload, installed)
        if launcher == main:
            for role in ("airk-service", "airk-index", "airk-source"):
                name = role + (".exe" if operating_system == "windows" else "")
                shutil.copy2(installed / main, installed / name)
        calls = []

        def invoke(image, *arguments, raw=False):
            calls.append((image.relative_to(installed).as_posix(), arguments, raw))
            return f"airk {version}" if raw else {"self_test": "passed"}

        return payload, installed, inventory, invoke, calls

    def test_current_mac_uses_approved_standalone_image_and_sealed_main_without_role_aliases(self):
        with tempfile.TemporaryDirectory() as temporary:
            payload, installed, inventory, invoke, calls = self.images(Path(temporary), "macos", "1.8.8")
            result = smoke.check_installed_images(payload, installed, inventory, invoke)
            image = smoke.runtime_builder().launcher_name("macos", "1.8.8")
            self.assertEqual(result, (True, inventory["files"][image]))
            self.assertEqual(calls, [("AIRK.app/Contents/MacOS/airk", ("runtime", "self-test"), False),
                                     (image, ("--version",), True)])
            self.assertFalse(any((installed / name).exists() for name in ["airk-service", "airk-index", "airk-source"]))
            smoke.check_payload_unchanged(payload, inventory, smoke.sha256(payload / "airk-install.json"))

    def test_legacy_mac_windows_and_linux_keep_exact_unaliased_role_checks(self):
        for operating_system, version in [("macos", "1.8.7"), ("windows", "1.8.8"), ("linux", "1.8.8")]:
            with self.subTest(operating_system=operating_system), tempfile.TemporaryDirectory() as temporary:
                payload, installed, inventory, invoke, calls = self.images(Path(temporary), operating_system, version)
                self.assertFalse(smoke.check_installed_images(payload, installed, inventory, invoke)[0])
                self.assertEqual(len(calls), 3)
                name = "airk-index" + (".exe" if operating_system == "windows" else "")
                (installed / name).write_bytes(b"changed role image")
                with self.assertRaisesRegex(RuntimeError, "modified"):
                    smoke.check_installed_images(payload, installed, inventory, invoke)

    def test_current_mac_refuses_missing_unlisted_or_modified_launcher_and_extra_aliases(self):
        for fault in ["missing", "unlisted", "modified", "extra_alias"]:
            with self.subTest(fault=fault), tempfile.TemporaryDirectory() as temporary:
                payload, installed, inventory, invoke, _ = self.images(Path(temporary), "macos", "1.8.8")
                name = smoke.runtime_builder().launcher_name("macos", "1.8.8")
                if fault == "missing":
                    (installed / name).unlink()
                elif fault == "unlisted":
                    inventory["files"].pop(name)
                elif fault == "modified":
                    (installed / name).write_bytes(b"changed standalone image")
                else:
                    (installed / "airk-service").write_bytes(b"unexpected copied sealed main")
                with self.assertRaises(RuntimeError):
                    smoke.check_installed_images(payload, installed, inventory, invoke)

    def test_current_mac_refuses_aliased_image_and_unexecutable_unix_mode(self):
        with tempfile.TemporaryDirectory() as temporary:
            root = Path(temporary)
            payload, installed, inventory, invoke, _ = self.images(root, "macos", "1.8.8")
            name = smoke.runtime_builder().launcher_name("macos", "1.8.8")
            alias = root / "unowned-alias"
            os.link(installed / name, alias)
            with self.assertRaisesRegex(RuntimeError, "aliased"):
                smoke.check_installed_images(payload, installed, inventory, invoke)
            alias.unlink()
            if os.name != "nt":
                (installed / name).chmod(0o600)
                with self.assertRaisesRegex(RuntimeError, "nonexecutable"):
                    smoke.check_installed_images(payload, installed, inventory, invoke)

    def test_copied_mac_launcher_requires_signature_exact_bytes_and_version(self):
        with tempfile.TemporaryDirectory() as temporary:
            _, installed, inventory, invoke, _ = self.images(Path(temporary), "macos", "1.8.8")
            image = installed / smoke.runtime_builder().launcher_name("macos", "1.8.8")
            observed = []
            smoke.check_copied_mac_launcher(image, smoke.sha256(image), invoke, "1.8.8", observed.append)
            self.assertEqual(observed, [image])
            with self.assertRaisesRegex(RuntimeError, "version"):
                smoke.check_copied_mac_launcher(image, smoke.sha256(image), lambda *args, **kwargs: "airk 1.8.7", "1.8.8", observed.append)
            def reject(_image):
                raise RuntimeError("injected strict signature rejection")
            with self.assertRaisesRegex(RuntimeError, "signature"):
                smoke.check_copied_mac_launcher(image, smoke.sha256(image), invoke, "1.8.8", reject)
            with self.assertRaisesRegex(RuntimeError, "modified"):
                smoke.check_copied_mac_launcher(image, "0" * 64, invoke, "1.8.8", observed.append)

    def test_lifecycle_source_preservation_check_rejects_changed_bytes(self):
        with tempfile.TemporaryDirectory() as temporary:
            payload, _, inventory, _, _ = self.images(Path(temporary), "macos", "1.8.8")
            identity = smoke.sha256(payload / "airk-install.json")
            smoke.check_payload_unchanged(payload, inventory, identity)
            main = smoke.runtime_builder().runtime_names("macos")[0]
            (payload / main).write_bytes(b"source changed during lifecycle")
            with self.assertRaisesRegex(RuntimeError, "source payload"):
                smoke.check_payload_unchanged(payload, inventory, identity)

    def test_lifecycle_source_preservation_check_rejects_unlisted_files(self):
        with tempfile.TemporaryDirectory() as temporary:
            payload, _, inventory, _, _ = self.images(Path(temporary), "macos", "1.8.8")
            identity = smoke.sha256(payload / "airk-install.json")
            (payload / "unexpected-role-copy").write_bytes(b"unlisted source mutation")
            with self.assertRaisesRegex(RuntimeError, "topology"):
                smoke.check_payload_unchanged(payload, inventory, identity)

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

    def test_fixture_parent_is_created_without_a_prior_native_build(self):
        with tempfile.TemporaryDirectory(prefix="airk-install-root-") as root:
            parent = smoke.fixture_parent(Path(root))
            self.assertEqual(parent, Path(root) / "target")
            self.assertTrue(parent.is_dir())
            self.assertEqual(smoke.fixture_parent(Path(root)), parent)


if __name__ == "__main__":
    unittest.main()
