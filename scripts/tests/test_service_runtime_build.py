# SPDX-License-Identifier: Apache-2.0
"""A source race must not publish a mixed executable pair."""
import importlib.util
import plistlib
from pathlib import Path
import sys
import tempfile
import unittest
from unittest import mock


class ServiceRuntimeBuildTests(unittest.TestCase):
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
                self.assertEqual(codesign.call_count, 3)
                self.assertEqual(codesign.call_args_list[1].args[0][-1], str(root / "AIRK.app"))
                self.assertIn("--timestamp=none", codesign.call_args_list[0].args[0])
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
