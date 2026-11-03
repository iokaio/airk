# SPDX-License-Identifier: Apache-2.0
"""Runtime staging preserves source identity and runnable Mac development bundles."""
import importlib.util
import os
import plistlib
import shutil
import subprocess
from pathlib import Path
import sys
import tempfile
import unittest
from unittest import mock


class ServiceRuntimeBuildTests(unittest.TestCase):
    def test_current_mac_bundle_has_a_separate_launcher_and_legacy_layout_stays_exact(self):
        script = Path(__file__).resolve().parents[1] / "build-service-runtime.py"
        specification = importlib.util.spec_from_file_location("standalone_image_builder", script)
        builder = importlib.util.module_from_spec(specification)
        specification.loader.exec_module(builder)
        for version, expected in [("1.8.7", 3), ("1.8.8", 4), ("1.9.0", 4)]:
            with self.subTest(version=version), tempfile.TemporaryDirectory() as temporary:
                root = Path(temporary)
                (root / "airk").write_bytes(b"compiled main before the app seal")
                (root / "airk-embed").write_bytes(b"compiled helper")
                builder.arrange_macos_bundle(root, version)
                names = builder.signing_names("macos", version)
                self.assertEqual(len(names), expected)
                launcher = root / builder.launcher_name("macos", version)
                self.assertEqual(launcher.read_bytes(), b"compiled main before the app seal")
                if version != "1.8.7":
                    self.assertEqual(launcher.name, "airk-launcher-image")
                    self.assertNotEqual(launcher, root / builder.runtime_names("macos")[0])
                    with mock.patch.object(builder.subprocess, "run") as signing:
                        builder.sign_development_macos_bundle(root)
                        self.assertEqual(signing.call_count, 5)
                        self.assertEqual(signing.call_args_list[2].args[0][-1], str(launcher))
                        self.assertEqual(signing.call_args_list[3].args[0][-1], str(root / "AIRK.app"))
                else:
                    self.assertFalse((root / "AIRK.app/Contents/MacOS/airk-launcher-image").exists())

    @unittest.skipUnless(sys.platform == "darwin" and shutil.which("clang"), "macOS standalone launcher/sealed-copy fixture")
    def test_standalone_launcher_remains_valid_after_sealing_and_copying(self):
        self._check_standalone_copy("-", None)

    @unittest.skipUnless(sys.platform == "darwin" and shutil.which("clang")
                         and os.environ.get("AIRK_TEST_MAC_SIGNING_IDENTITY")
                         and os.environ.get("AIRK_TEST_MAC_TEAM_ID"),
                         "explicit macOS Developer ID identity/team required; no notarization upload")
    def test_developer_id_standalone_launcher_survives_sealed_copy(self):
        identity = os.environ["AIRK_TEST_MAC_SIGNING_IDENTITY"]
        team = os.environ["AIRK_TEST_MAC_TEAM_ID"]
        self.assertTrue(identity.startswith("Developer ID Application:"))
        self.assertRegex(team, r"\A[A-Z0-9]{10}\Z")
        self._check_standalone_copy(identity, team)

    def _check_standalone_copy(self, identity, team):
        def checked(arguments, timeout=30):
            result = subprocess.run(arguments, capture_output=True, text=True, timeout=timeout)
            self.assertEqual(result.returncode, 0,
                             f"{arguments!r}\nstdout: {result.stdout[-4096:]}\nstderr: {result.stderr[-4096:]}")
            return result
        script = Path(__file__).resolve().parents[1] / "build-service-runtime.py"
        specification = importlib.util.spec_from_file_location("copied_launcher_builder", script)
        builder = importlib.util.module_from_spec(specification)
        specification.loader.exec_module(builder)
        with tempfile.TemporaryDirectory(prefix="airk standalone image ") as temporary:
            root = Path(temporary).resolve(strict=True)
            source = root / "fixture.c"
            source.write_text('#include <stdio.h>\nint main(void) { puts("airk 1.8.8"); return 0; }\n', encoding="utf-8")
            bundled = root / builder.runtime_names("macos")[0]
            bundled.parent.mkdir(parents=True)
            (bundled.parent.parent / "Info.plist").write_bytes(builder.macos_bundle_files("1.8.8")["AIRK.app/Contents/Info.plist"])
            checked(["clang", str(source), "-o", str(bundled)])
            image = root / builder.launcher_name("macos", "1.8.8")
            shutil.copyfile(bundled, image)
            timestamp = "--timestamp" if team else "--timestamp=none"
            options = ["--force", "--sign", identity, "--options", "runtime", timestamp]
            checked(["/usr/bin/codesign", *options, str(image)], timeout=60)
            checked(["/usr/bin/codesign", *options, str(root / "AIRK.app")], timeout=60)
            checked(["/usr/bin/codesign", "--verify", "--strict", str(root / "AIRK.app")])
            # The app main remains bundle-bound; never weaken its strict check.
            copied_main = root / "invalid-main-copy"
            shutil.copyfile(bundled, copied_main)
            rejected = subprocess.run(["/usr/bin/codesign", "--verify", "--strict", str(copied_main)], capture_output=True, text=True, timeout=30)
            self.assertNotEqual(rejected.returncode, 0, "fixture failed to reproduce the sealed-main copy boundary")
            copied = root / "airk-launcher"
            shutil.copyfile(image, copied)
            copied.chmod(0o700)
            self.assertEqual(copied.read_bytes(), image.read_bytes())
            checked(["/usr/bin/codesign", "--verify", "--strict", str(copied)])
            result = checked([str(copied), "--version"], timeout=10)
            self.assertEqual(result.stdout.strip(), "airk 1.8.8")
            if team:
                specification = importlib.util.spec_from_file_location("signed_copy_fixture", script.parent / "sign-native.py")
                signer = importlib.util.module_from_spec(specification)
                specification.loader.exec_module(signer)
                component = {"platform": "Darwin", "binary_sha256": signer.digest(image), "team_id": team}
                observed = signer.verify_copied_launcher(root, "1.8.8", component)
                self.assertTrue(observed["strict_signature"])
                self.assertTrue(observed["standalone_version_probe"])

    @unittest.skipUnless(sys.platform == "darwin" and shutil.which("clang"), "macOS native signing/load fixture")
    def test_development_mac_helper_loads_its_adhoc_signed_library(self):
        script = Path(__file__).resolve().parents[1] / "build-service-runtime.py"
        specification = importlib.util.spec_from_file_location("native_load_builder", script)
        builder = importlib.util.module_from_spec(specification)
        specification.loader.exec_module(builder)
        with tempfile.TemporaryDirectory(prefix="airk-mac-library-load-") as temporary:
            root = Path(temporary).resolve(strict=True)
            (root / "library.c").write_text("int airk_fixture_value(void) { return 42; }\n", encoding="utf-8")
            (root / "loader.c").write_text('''#include <dlfcn.h>
#include <stdio.h>
int main(int argc, char **argv) {
    if (argc != 2) return 2;
    void *library = dlopen(argv[1], RTLD_NOW | RTLD_LOCAL);
    if (!library) { fprintf(stderr, "%s\\n", dlerror()); return 3; }
    int (*value)(void) = dlsym(library, "airk_fixture_value");
    int result = value && value() == 42 ? 0 : 4;
    dlclose(library);
    return result;
}
''', encoding="utf-8")
            for name in ("airk", "airk-embed"):
                subprocess.run(["clang", "-mmacosx-version-min=13.3", str(root / "loader.c"), "-o", str(root / name)], check=True, capture_output=True, timeout=30)
            builder.arrange_macos_bundle(root, "1.6.4")
            library = root / "AIRK.app/Contents/MacOS/libonnxruntime.dylib"
            subprocess.run(["clang", "-mmacosx-version-min=13.3", "-dynamiclib", str(root / "library.c"), "-o", str(library)], check=True, capture_output=True, timeout=30)
            builder.sign_development_macos_bundle(root)
            helper = root / builder.runtime_names("macos")[1]
            result = subprocess.run([str(helper), str(library)], capture_output=True, text=True, timeout=10)
            self.assertEqual(result.returncode, 0, result.stderr)

    def test_mac_bundle_keeps_exactly_two_executables_and_static_channel_scope(self):
        script = Path(__file__).resolve().parents[1] / "build-service-runtime.py"
        specification = importlib.util.spec_from_file_location("service_runtime_builder", script)
        builder = importlib.util.module_from_spec(specification)
        specification.loader.exec_module(builder)
        with tempfile.TemporaryDirectory() as temporary:
            root = Path(temporary)
            (root / "airk").write_bytes(b"lightweight main fixture")
            (root / "airk-embed").write_bytes(b"inference helper fixture")
            builder.arrange_macos_bundle(root, "1.4.0")
            main, helper = builder.runtime_names("macos")
            self.assertEqual((root / main).read_bytes(), b"lightweight main fixture")
            self.assertEqual((root / helper).read_bytes(), b"inference helper fixture")
            self.assertFalse((root / "airk").exists())
            self.assertFalse((root / "airk-embed").exists())
            info = plistlib.loads((root / "AIRK.app/Contents/Info.plist").read_bytes())
            self.assertEqual(info["CFBundleExecutable"], "airk")
            for channel in ("stable", "preview"):
                name = f"io.iokaio.airk.service.{channel}"
                agent = plistlib.loads((root / f"AIRK.app/Contents/Library/LaunchAgents/{name}.plist").read_bytes())
                self.assertEqual(agent["Label"], name)
                self.assertEqual(agent["BundleProgram"], "Contents/MacOS/airk")
                self.assertEqual(agent["ProgramArguments"], ["airk", "runtime", "login-foreground", "--channel", channel])
                self.assertNotIn("Program", agent)
            with mock.patch.object(builder.subprocess, "run") as codesign:
                builder.sign_development_macos_bundle(root)
                self.assertEqual(codesign.call_count, 4)
                self.assertEqual(codesign.call_args_list[2].args[0][-1], str(root / "AIRK.app"))
                self.assertIn("--timestamp=none", codesign.call_args_list[0].args[0])
                for call in codesign.call_args_list[:3]:
                    command = call.args[0]
                    self.assertEqual(command[command.index("--sign") + 1], "-")
                    self.assertEqual(command[command.index("--options") + 1], "0")
    def test_changed_source_discards_owned_staging_and_preserves_inputs(self):
        script = Path(__file__).resolve().parents[1] / "build-service-runtime.py"
        specification = importlib.util.spec_from_file_location("service_runtime_builder", script)
        builder = importlib.util.module_from_spec(specification)
        specification.loader.exec_module(builder)
        with tempfile.TemporaryDirectory(prefix="airk-build-original-fixture-") as temporary:
            root = Path(temporary)
            (root / "Cargo.toml").write_text('[workspace.package]\nversion="9.8.7"\n', encoding="utf-8")
            suffix = ".exe" if sys.platform == "win32" else ""
            main = root / f"airk{suffix}"
            helper = root / f"airk-embed{suffix}"
            main.write_bytes(b"Original main build fixture.")
            helper.write_bytes(b"Original inference build fixture.")
            destination = root / "runtime"

            def artifact(name, _profile, _features):
                return helper if name == "airk-embed" else main

            def identity(arguments, **_kwargs):
                self.assertEqual(arguments[1], "--version")
                return f"{Path(arguments[0]).stem} 9.8.7\n"

            with (mock.patch.object(builder, "ROOT", root),
                  mock.patch.object(builder, "load_script"),
                  mock.patch.object(builder, "stage_inference_runtime"),
                  mock.patch.object(builder, "verify_embedded_models"),
                  mock.patch.object(builder, "check_lightweight_dependencies"),
                  mock.patch.object(builder, "cargo_binary", side_effect=artifact),
                  mock.patch.object(builder.platform, "system", return_value="Windows" if suffix else "Linux"),
                  mock.patch.object(builder.platform, "machine", return_value="x86_64"),
                  mock.patch.object(builder, "source_identity", side_effect=[
                      {"sha256": "a" * 64, "files": 1}, {"sha256": "b" * 64, "files": 1}]),
                  mock.patch.object(builder.subprocess, "check_output", side_effect=identity),
                  mock.patch.object(sys, "argv", [str(script), "--destination", str(destination)])):
                with self.assertRaisesRegex(RuntimeError, "source changed"):
                    builder.main()
            self.assertFalse(destination.exists())
            self.assertFalse(any(path.name.startswith(".airk-runtime-stage-") for path in root.iterdir()))
            self.assertEqual(main.read_bytes(), b"Original main build fixture.")
            self.assertEqual(helper.read_bytes(), b"Original inference build fixture.")


if __name__ == "__main__":
    unittest.main()
