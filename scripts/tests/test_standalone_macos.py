# SPDX-License-Identifier: Apache-2.0
import base64
import hashlib
import importlib.util
import json
import os
from pathlib import Path
import shutil
import subprocess
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


native_tests = load("standalone_macos_native_tests", "scripts/tests/test_package_native.py")
native = load("standalone_macos_native_package", "scripts/package-native.py")
SPLIT_VERSION = "1.8.24"
BASH = shutil.which("bash")


class SplitServiceMacInstaller(unittest.TestCase):
    def test_split_wrapper_pins_the_payload_and_verifies_it_before_the_setup_role_runs(self):
        pins = ("a" * 64, "b" * 64, "c" * 64)
        for target, architecture in package.TARGETS.items():
            with self.subTest(target=target):
                files = package.split_installer_files(SPLIT_VERSION, target, *pins, 12)
                script = files["Install AIRK.command"].decode()
                readme = files["README.txt"].decode()
                self.assertNotIn("@@", script + readme)
                for name, pin in zip(("inventory_sha256", "roster_sha256", "airk_sha256"), pins):
                    self.assertIn(f"{name}='{pin}'", script)
                self.assertIn("file_count='12'", script)
                self.assertIn(f'"$(/usr/bin/uname -m)" == "{architecture}"', script)
                self.assertIn('"requested_by":"dmg"', script)
                first_native = script.index('run_setup "$image" --plan')
                for check in ("/usr/bin/codesign --verify --strict", "/usr/bin/shasum -a 256 -c -s payload.sha256",
                              '"$airk_sha256"', '"$inventory_sha256"', "-links +1"):
                    self.assertLess(script.index(check), first_native, check)
                self.assertLess(script.index("--prepare"), script.index("( place )"))
                for absent in ("sudo", "python", ".vsix", ".zshrc", ".bash_profile", "runtime-source"):
                    self.assertNotIn(absent, script)
                self.assertIn("Login startup is not implemented", readme)
                self.assertIn("airkctl service uninstall --yes", readme)
        target = next(iter(package.TARGETS))
        for arguments in ((SPLIT_VERSION, "unknown", *pins, 12), ("1.8.14", target, *pins, 12),
                          (SPLIT_VERSION, target, "a;whoami", pins[1], pins[2], 12),
                          (SPLIT_VERSION, target, *pins, "12"), (SPLIT_VERSION, target, *pins, 0)):
            with self.subTest(arguments=arguments), self.assertRaises(ValueError):
                package.split_installer_files(*arguments)

    @unittest.skipIf(BASH is None, "bash is not available on this host")
    def test_split_wrapper_is_valid_bash(self):
        script = package.split_installer_files(SPLIT_VERSION, next(iter(package.TARGETS)), "a" * 64, "b" * 64,
                                               "c" * 64, 3)["Install AIRK.command"]
        # On stdin, so a bash that cannot open host paths (WSL's launcher) still checks it.
        result = subprocess.run([BASH, "-n"], input=script, capture_output=True)
        self.assertEqual(result.returncode, 0, result.stderr.decode("utf-8", "replace"))

    def test_split_staging_places_the_signed_bundle_notices_and_inventory(self):
        target = "aarch64-apple-darwin"
        source = {"sha256": "1" * 64, "files": 4}
        with tempfile.TemporaryDirectory() as temporary:
            base = Path(temporary)
            runtime = base / "runtime"
            runtime.mkdir()
            _, build = native_tests.split_runtime(runtime, target, source)
            files, _ = native.validate_runtime(runtime, SPLIT_VERSION, target, source)
            contents = base / "contents"
            inventory, wrappers = package.stage_service(runtime.resolve(), contents, files, SPLIT_VERSION, target,
                                                        build["admin_assets"])
            self.assertEqual({path.name for path in contents.iterdir()},
                             {"Install AIRK.command", "README.txt", "payload.sha256", "payload"})
            self.assertEqual((contents / "Install AIRK.command").read_bytes(), wrappers["Install AIRK.command"])
            bundle = {name for name in files if name.startswith("AIRK.app/")}
            legal = {"notices/" + name for name in ("LICENSE", "NOTICE", "THIRD_PARTY_NOTICES.md")}
            notices = {name for name in files if name.startswith("notices/")}
            self.assertEqual(set(inventory["files"]), bundle | legal | notices)
            self.assertEqual(inventory["executables"], {"airk": "AIRK.app/Contents/MacOS/airk",
                                                        "airkctl": "AIRK.app/Contents/MacOS/airkctl"})
            self.assertEqual(inventory["signing"], {"platform": "macos", "team_id": "5PX7D99PXA"})
            self.assertEqual((inventory["schema"], inventory["target"]), ("airk-service-payload/1", target))
            self.assertEqual(inventory["admin_assets"], {"inventory_sha256": build["admin_assets"]["inventory_sha256"],
                                                         "bundle_sha256": build["admin_assets"]["bundle_sha256"]})
            placed = contents / "payload/payload-inventory.json"
            self.assertEqual(json.loads(placed.read_bytes()), inventory)
            script = wrappers["Install AIRK.command"].decode()
            self.assertIn(f"inventory_sha256='{hashlib.sha256(placed.read_bytes()).hexdigest()}'", script)
            self.assertIn(f"file_count='{len(inventory['files']) + 1}'", script)
            if os.name != "nt":
                self.assertEqual((contents / "payload/AIRK.app/Contents/MacOS/airkctl").stat().st_mode & 0o777, 0o755)
            with self.assertRaisesRegex(ValueError, "outside the service layout"):
                package.service_layout(runtime.resolve(), {"airk-embed": "0" * 64}, base / "refused")


if __name__ == "__main__":
    unittest.main()
