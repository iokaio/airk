# SPDX-License-Identifier: Apache-2.0
import hashlib
import importlib.util
import json
from pathlib import Path
import tempfile
import unittest
import zipfile

spec = importlib.util.spec_from_file_location("package_extension", Path(__file__).parents[1] / "package-extension.py")
package = importlib.util.module_from_spec(spec)
spec.loader.exec_module(package)


class VsixContracts(unittest.TestCase):
    def test_walkthrough_commands_and_media_are_packaged(self):
        manifest = json.loads((package.EXTENSION / "package.json").read_text(encoding="utf-8"))
        commands = {entry["command"] for entry in manifest["contributes"]["commands"]}
        walkthroughs = manifest["contributes"]["walkthroughs"]
        self.assertEqual(len(walkthroughs), 1)
        steps = walkthroughs[0]["steps"]
        self.assertEqual(len(steps), 4)
        self.assertEqual(len({step["id"] for step in steps}), len(steps))
        for step in steps:
            event, = step["completionEvents"]
            self.assertTrue(event.startswith("onCommand:"))
            command = event.removeprefix("onCommand:")
            self.assertIn(command, commands)
            self.assertIn("(command:" + command + ")", step["description"])
            image = step["media"]["image"]
            self.assertTrue(step["media"]["altText"])
            self.assertIn(image, {"images/" + name for name in package.ILLUSTRATIONS})
            self.assertTrue((package.ROOT / "docs" / image).is_file())

    def test_runtime_import_closure_includes_controls_without_test_extras(self):
        with tempfile.TemporaryDirectory() as directory:
            compiled = Path(directory) / "compiled"
            compiled.mkdir()
            for name in package.MODULES:
                (compiled / name).write_text('"use strict";\n', encoding="utf-8")
            (compiled / "extension.js").write_text(
                'require("vscode"); require("./retention"); require("./extractionPolicy"); require("./history");', encoding="utf-8")
            (compiled / "history.js").write_text('require("./native.js");', encoding="utf-8")
            (compiled / "native.js").write_text('require("node:fs"); require("./model");', encoding="utf-8")
            (compiled / "test").mkdir()
            (compiled / "test" / "private.test.js").write_text("not runtime", encoding="utf-8")
            (compiled / "extension.js.map").write_text("not runtime", encoding="utf-8")
            stage = Path(directory) / "stage"
            package.stage_runtime_modules(compiled, stage)
            self.assertEqual({p.name for p in stage.iterdir()}, set(package.MODULES))
            for name in package.MODULES:
                self.assertEqual((stage / name).read_bytes(), (compiled / name).read_bytes())
            for missing in ("./futureControl", "./test/private.test", "../outside"):
                (compiled / "history.js").write_text('require("' + missing + '");', encoding="utf-8")
                rejected = Path(directory) / "rejected"
                with self.assertRaisesRegex(ValueError, "Runtime import outside allowlisted stage"):
                    package.stage_runtime_modules(compiled, rejected)
                self.assertFalse(rejected.exists(), "Validate closure before staging any runtime modules")

    @unittest.skipUnless((package.EXTENSION / "out" / "extension.js").is_file(), "compiled extension unavailable")
    def test_actual_compiled_runtime_imports_are_all_staged(self):
        with tempfile.TemporaryDirectory() as directory:
            stage = Path(directory) / "out"
            package.stage_runtime_modules(package.EXTENSION / "out", stage)
            self.assertEqual({p.name for p in stage.iterdir()}, set(package.MODULES))

    def test_platform_identity_payload_tampering_and_permission_checks(self):
        with tempfile.TemporaryDirectory() as directory:
            archive = Path(directory) / "fixture.vsix"
            files = {"package.json": b"{}", "README.md": b"fixture readme", "LICENSE": b"fixture license", "native/airk": b"fixture executable"}
            expected = {name: hashlib.sha256(data).hexdigest() for name, data in files.items()}
            with zipfile.ZipFile(archive, "w") as zipped:
                for name, data in files.items():
                    output_name = "readme.md" if name == "README.md" else "LICENSE.txt" if name == "LICENSE" else name
                    info = zipfile.ZipInfo("extension/" + output_name)
                    info.external_attr = (0o100755 if name == "native/airk" else 0o100644) << 16
                    zipped.writestr(info, data)
                zipped.writestr("[Content_Types].xml", "<Types/>")
                zipped.writestr("extension.vsixmanifest", '<PackageManifest><Identity Id="airk" Publisher="iokaio" Version="0.1.0" TargetPlatform="linux-x64"/><DisplayName>AIRK (Beta)</DisplayName><Property Id="Microsoft.VisualStudio.Code.PreRelease" Value="true"/></PackageManifest>')
            package.verify_vsix(archive, expected, "airk", "linux-x64", "0.1.0", True, "AIRK (Beta)")
            with self.assertRaisesRegex(ValueError, "pre-release flag differs"):
                package.verify_vsix(archive, expected, "airk", pre_release=False)
            with self.assertRaisesRegex(ValueError, "display name differs"):
                package.verify_vsix(archive, expected, "airk", display_name="Other label")
            with self.assertRaisesRegex(ValueError, "platform differs"):
                package.verify_vsix(archive, expected, "airk", "darwin-arm64", "0.1.0")
            with self.assertRaisesRegex(ValueError, "version differs"):
                package.verify_vsix(archive, expected, "airk", "linux-x64", "0.2.0")
            with self.assertRaisesRegex(ValueError, "payload differs"):
                package.verify_vsix(archive, dict(expected, **{"native/airk": "0" * 64}), "airk")
            with zipfile.ZipFile(archive, "a") as zipped:
                zipped.writestr("extension/private.env", "fixture must fail")
            with self.assertRaisesRegex(ValueError, "payload differs"):
                package.verify_vsix(archive, expected, "airk")

    def test_path_escape_refused(self):
        for name in ("../private", "/private", "native/../../private", "C:/private", "native\\private",
                     "native/./airk", "native//airk", "", "native/airk/"):
            with self.assertRaises(ValueError):
                package.safe_name(name)


if __name__ == "__main__":
    unittest.main()
