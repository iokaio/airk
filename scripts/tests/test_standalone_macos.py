# SPDX-License-Identifier: Apache-2.0
import base64
import hashlib
import importlib.util
import os
from pathlib import Path
from types import SimpleNamespace
import tempfile
import unittest
from unittest.mock import patch

ROOT = Path(__file__).resolve().parents[2]


def load(name, path):
    specification = importlib.util.spec_from_file_location(name, ROOT / path)
    result = importlib.util.module_from_spec(specification)
    specification.loader.exec_module(result)
    return result


package = load("standalone_macos", "scripts/package-standalone-macos.py")
ci = load("standalone_macos_ci", "scripts/ci-macos-sign.py")


class StandaloneMacInstaller(unittest.TestCase):
    def test_bootstrap_uses_distributor_pin_and_native_guided_setup_for_both_architectures(self):
        pin = "ab" * 32
        for target, architecture in package.TARGETS.items():
            with self.subTest(target=target):
                executable_pin = "cd" * 32
                files = package.installer_files("1.8.14", target, pin, executable_pin)
                script = files["Install AIRK.command"].decode()
                self.assertNotIn("@@", script)
                self.assertIn('exec "$airk" setup --runtime-source "$runtime_dir" --approved-sha256', script)
                self.assertIn("'" + pin + "'", script)
                self.assertIn('"$actual_manifest" != "' + pin + '"', script)
                self.assertIn('"$actual_executable" != "' + executable_pin + '"', script)
                self.assertIn('! -f "$airk" || -L "$airk"', script)
                self.assertLess(script.index('"$actual_executable" !='), script.index('exec "$airk"'))
                self.assertLess(script.index('/usr/bin/codesign --verify --strict'), script.index('exec "$airk"'))
                self.assertIn('"$(/usr/bin/uname -m)" != "' + architecture + '"', script)
                self.assertNotIn("python", script)
                self.assertNotIn("sudo", script)
                self.assertNotIn(".vsix", script)
                self.assertIn('installed executable with "uninstall"', files["README.txt"].decode())
        for version, target, digest, executable in [("1.8.14;whoami", next(iter(package.TARGETS)), pin, pin),
                ("1.8.14", "unknown", pin, pin), ("1.8.14", next(iter(package.TARGETS)), "a;whoami", pin),
                ("1.8.14", next(iter(package.TARGETS)), pin, "wrong-executable-pin")]:
            with self.assertRaises(ValueError):
                package.installer_files(version, target, digest, executable)

    def test_staging_copies_only_verified_native_bytes_and_rejects_changes_and_escape(self):
        with tempfile.TemporaryDirectory() as temporary:
            base = Path(temporary)
            runtime = base / "runtime"
            runtime.mkdir()
            native = b"fictional signed native payload; never executed"
            (runtime / "airk").write_bytes(native)
            files = {"airk": hashlib.sha256(native).hexdigest()}
            package.stage(runtime.resolve(), base / "contents", files, ("airk",), {"README.txt": b"native only"})
            self.assertEqual((base / "contents/runtime/airk").read_bytes(), native)
            self.assertEqual((runtime / "airk").read_bytes(), native)
            (runtime / "airk").write_bytes(b"changed")
            with self.assertRaisesRegex(ValueError, "changed"):
                package.stage(runtime.resolve(), base / "changed", files, ("airk",), {})
            (base / "foreign").write_bytes(native)
            with self.assertRaisesRegex(ValueError, "escaped"):
                package.stage(runtime.resolve(), base / "escape", {"../foreign": files["airk"]}, (), {})

    def test_image_verification_requires_timestamp_team_and_exact_public_certificate(self):
        public = b"fictional public certificate"
        identity = hashlib.sha1(public).hexdigest()
        details = "Authority=Developer ID Application: Example\nTeamIdentifier=5PX7D99PXA\nTimestamp=reviewed\n"

        def tool(arguments, **_):
            for argument in arguments:
                if str(argument).startswith("--extract-certificates="):
                    Path(str(argument).split("=", 1)[1] + "0").write_bytes(public)
            return ""

        with patch.object(package, "run", side_effect=tool), \
                patch.object(package, "CERTIFICATE_SHA256", hashlib.sha256(public).hexdigest()), \
                patch.object(package.subprocess, "run", return_value=SimpleNamespace(stderr=details)):
            result = package.verify_image_signature(Path("inert.dmg"), identity, package.TEAM, Path("keychain"))
            self.assertTrue(result["timestamp_present"])
            self.assertEqual(result["identity"], identity)
        for changed in [details.replace("Timestamp=reviewed\n", ""), details.replace(package.TEAM, "OTHERTEAM0")]:
            with patch.object(package, "run", side_effect=tool), \
                    patch.object(package.subprocess, "run", return_value=SimpleNamespace(stderr=changed)):
                with self.assertRaisesRegex(ValueError, "Developer ID"):
                    package.verify_image_signature(Path("inert.dmg"), identity, package.TEAM, Path("keychain"))
        with patch.object(package, "run", side_effect=tool), \
                patch.object(package.subprocess, "run", return_value=SimpleNamespace(stderr=details)), \
                patch.object(package, "CERTIFICATE_SHA256", "00" * 32):
            with self.assertRaisesRegex(ValueError, "certificate"):
                package.verify_image_signature(Path("inert.dmg"), identity, package.TEAM, Path("keychain"))

    def test_non_mac_packaging_cannot_start_platform_tools(self):
        with patch.object(package.sys, "platform", "win32"), patch.object(package, "run") as tools:
            with self.assertRaisesRegex(ValueError, "Mac runner"):
                package.package_installer(SimpleNamespace(target="aarch64-apple-darwin"))
            tools.assert_not_called()

    def test_ci_installer_hook_reuses_existing_keychain_and_contains_no_credentials_or_editor_actions(self):
        self.assertEqual(package.CERTIFICATE_SHA256, ci.CERTIFICATE_SHA256)
        self.assertEqual(package.TEAM, ci.TEAM)
        with patch.dict(os.environ, {}, clear=True), patch.object(ci.subprocess, "run") as run:
            ci.package_standalone_installer("aarch64-apple-darwin", "a" * 40, Path("existing.keychain"))
            command = run.call_args.args[0]
            self.assertIn("--signed-dir", command)
            self.assertEqual(command[command.index("--keychain") + 1], "existing.keychain")
            self.assertEqual(command[command.index("--identity") + 1], "a" * 40)
            self.assertIn("artifacts/standalone/aarch64-apple-darwin", command)
            self.assertFalse(any("vsix" in str(value).lower() for value in command))
        for secret in ci.SECRET_NAMES:
            with patch.dict(os.environ, {secret: "must not reach child"}), patch.object(ci.subprocess, "run") as run:
                with self.assertRaisesRegex(ValueError, "secrets"):
                    ci.package_standalone_installer("aarch64-apple-darwin", "a" * 40, Path("keychain"))
                run.assert_not_called()
        with patch.object(ci, "runner_directory") as root:
            with self.assertRaisesRegex(ValueError, "complete native runtime"):
                ci.sign("aarch64-apple-darwin", standalone=True)
            root.assert_not_called()

    def test_installer_failure_still_removes_keychain_before_any_native_execution_probe(self):
        secrets = {"MACOS_CERTIFICATE_P12": base64.b64encode(b"inert fixture").decode(),
                   "MACOS_CERTIFICATE_PASSWORD": "fixture", "APPLE_ID": "fixture@example.invalid",
                   "APPLE_APP_SPECIFIC_PASSWORD": "fixture"}
        for fail in (False, True):
            with self.subTest(fail=fail), tempfile.TemporaryDirectory() as temporary:
                credentials = Path(temporary) / "credentials"

                def security(*arguments):
                    if arguments[0] == "list-keychains" and "-s" not in arguments:
                        return '"/tmp/original.keychain"'
                    if arguments[0] == "create-keychain":
                        Path(arguments[-1]).touch()
                    return ""

                def installer(*arguments):
                    self.assertTrue((credentials / "signing.keychain-db").exists())
                    self.assertFalse(any(name in os.environ for name in ci.SECRET_NAMES))
                    if fail:
                        raise RuntimeError("installer failed")

                def execution(*arguments):
                    self.assertFalse(credentials.exists())

                with patch.dict(os.environ, secrets), patch.object(ci, "runner_directory", return_value=credentials), \
                        patch.object(ci, "security", side_effect=security), \
                        patch.object(ci, "private_run", return_value=""), \
                        patch.object(ci, "imported_identity", return_value="a" * 40), \
                        patch.object(ci.subprocess, "run"), patch.object(ci, "verify_signed_output"), \
                        patch.object(ci, "package_standalone_installer", side_effect=installer) as packaged, \
                        patch.object(ci, "qualify_signed_runtime", side_effect=execution) as probed:
                    if fail:
                        with self.assertRaisesRegex(RuntimeError, "installer failed"):
                            ci.sign("aarch64-apple-darwin", paired=True, standalone=True)
                    else:
                        ci.sign("aarch64-apple-darwin", paired=True, standalone=True)
                    self.assertEqual(packaged.call_count, 1)
                    self.assertEqual(probed.call_count, int(not fail))
                    self.assertFalse(credentials.exists())


if __name__ == "__main__":
    unittest.main()
