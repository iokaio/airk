# SPDX-License-Identifier: Apache-2.0
import hashlib
import importlib.util
import json
import shutil
import subprocess
from pathlib import Path
import tempfile
import unittest
import zipfile

spec = importlib.util.spec_from_file_location("package_extension", Path(__file__).parents[1] / "package-extension.py")
package = importlib.util.module_from_spec(spec)
spec.loader.exec_module(package)


class VsixContracts(unittest.TestCase):
    def test_uninstall_hook_survives_packaging_without_development_scripts(self):
        source = json.loads((package.EXTENSION / "package.json").read_text(encoding="utf-8"))
        staged = package.runtime_package(source)
        self.assertEqual(staged["scripts"], {"vscode:uninstall": "node ./out/uninstallHook.js"})
        self.assertIn("extensionUninstall.js", package.MODULES)
        self.assertIn("uninstallHook.js", package.MODULES)
        self.assertNotIn("devDependencies", staged)
        self.assertIn("test", source["scripts"])
        source["scripts"]["vscode:uninstall"] = "node ./missing.js"
        with self.assertRaisesRegex(ValueError, "uninstall hook"):
            package.runtime_package(source)

    @unittest.skipUnless((package.EXTENSION / "node_modules" / "jsonc-parser").is_dir(), "npm dependencies unavailable")
    def test_packaged_parser_closure_and_license(self):
        with tempfile.TemporaryDirectory() as directory:
            # VSCE resolves collected file paths; use the same canonical root
            # when macOS gives tempfile the /var -> /private/var alias.
            root = Path(directory).resolve(strict=True)
            stage = root / "node_modules" / "jsonc-parser"
            package.stage_jsonc_parser(package.EXTENSION / "node_modules" / "jsonc-parser", stage)
            self.assertEqual({p.relative_to(stage).as_posix() for p in stage.rglob("*") if p.is_file()}, set(package.JSONC_FILES))
            self.assertIn("Permission is hereby granted", (stage / "LICENSE.md").read_text(encoding="utf-8"))
            if shutil.which("node"):
                subprocess.run(["node", "-e", 'const p=require(process.argv[1]); if(p.parse("{/*comment*/ \\\"value\\\": 2}").value!==2)process.exit(1);', str(stage)], check=True)
                manifest = json.loads((package.EXTENSION / "package.json").read_text(encoding="utf-8"))
                (root / "package.json").write_text(json.dumps({"name": "airk", "publisher": "iokaio", "version": "1.1.2",
                    "engines": {"vscode": "^1.99.0"}, "repository": {"type": "git", "url": "https://github.com/iokaio/airk"},
                    "license": "Apache-2.0", "files": manifest["files"], "dependencies": manifest["dependencies"]}), encoding="utf-8")
                (root / "README.md").write_text("AIRK packaging fixture", encoding="utf-8")
                (root / "LICENSE").write_text("Fixture legal file", encoding="utf-8")
                for name in ("out/fixture.js", "out/fixture.css", "native/airk.exe", "images/fixture.png", "images/fixture.svg", "NOTICE", "THIRD_PARTY_NOTICES.md"):
                    file = root / name
                    file.parent.mkdir(parents=True, exist_ok=True)
                    file.write_bytes(b"public packaging fixture")
                expected = {file.relative_to(root).as_posix(): package.digest(file.read_bytes()) for file in root.rglob("*") if file.is_file()}
                archive = root / "fixture.vsix"
                subprocess.run(["node", str(package.EXTENSION / "scripts" / "package.cjs"), str(root), "win32-x64", str(archive), "stable"], check=True, capture_output=True)
                package.verify_vsix(archive, expected, "airk.exe", "win32-x64", "1.1.2", False)
            source = stage / "lib" / "umd" / "main.js"
            source.write_text('require("./missing");', encoding="utf-8")
            with self.assertRaisesRegex(ValueError, "Unpackaged jsonc-parser dependency"):
                package.stage_jsonc_parser(stage, root / "rejected")

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
                'require("vscode"); require("./serviceExtension"); require("./serviceConsole");', encoding="utf-8")
            (compiled / "serviceExtension.js").write_text('require("./native.js");', encoding="utf-8")
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
                (compiled / "serviceExtension.js").write_text('require("' + missing + '");', encoding="utf-8")
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

    def test_bundles_are_self_contained_and_carry_the_preact_notice(self):
        banner = "/*! AIRK Explorer webview. Includes Preact 10.0.0 (MIT).\nPermission is hereby granted\n*/\n"
        with tempfile.TemporaryDirectory() as directory:
            compiled = Path(directory) / "compiled"
            compiled.mkdir()
            (compiled / "explorerWebview.js").write_bytes((banner + "(()=>{document.body;})();\n").encode("utf-8"))
            (compiled / "explorerWebview.css").write_bytes(b"body{margin:0}\n")
            assistance = "/*! AIRK Explorer Assistance MCP server 1.0.0. No third-party code. */\n"
            (compiled / "explorerAssistance.js").write_bytes((assistance + 'require("node:crypto");\n').encode("utf-8"))
            stage = Path(directory) / "stage"
            package.stage_bundles(compiled, stage, "10.0.0")
            self.assertEqual({p.name for p in stage.iterdir()}, set(package.BUNDLES))
            rejected = {
                'require("./serviceClient");': "local module",
                'import("./serviceClient");': "local module",
                'require("vscode");': "VS Code API",
                "class ServiceClient {}": "host implementation",
                "class IntentJournal {}": "host implementation",
                'require("node:fs");': "must not require",
            }
            for body, message in rejected.items():
                (compiled / "explorerWebview.js").write_bytes((banner + body).encode("utf-8"))
                target = Path(directory) / "rejected"
                with self.assertRaisesRegex(ValueError, message):
                    package.stage_bundles(compiled, target, "10.0.0")
                self.assertFalse(target.exists(), "Validate every bundle before staging any")
            (compiled / "explorerWebview.js").write_bytes(banner.encode("utf-8"))
            for body, message in {
                'require("fs");': "Node built-ins only",
                'require("vscode");': "VS Code API",
                "require(name);": "Node built-ins only",
                'import("node:fs").then(()=>require("child"));': "Node built-ins only",
                "class RuntimeHost {}": "host implementation",
            }.items():
                (compiled / "explorerAssistance.js").write_bytes((assistance + body).encode("utf-8"))
                with self.assertRaisesRegex(ValueError, message):
                    package.stage_bundles(compiled, Path(directory) / "assistance", "10.0.0")
            (compiled / "explorerAssistance.js").write_bytes(b"require('node:fs');")
            with self.assertRaisesRegex(ValueError, "identifying banner"):
                package.stage_bundles(compiled, Path(directory) / "assistance", "10.0.0")
            (compiled / "explorerAssistance.js").write_bytes(assistance.encode("utf-8"))
            with self.assertRaisesRegex(ValueError, "pinned Preact notice"):
                package.stage_bundles(compiled, Path(directory) / "wrong-version", "10.0.1")
            (compiled / "explorerWebview.css").unlink()
            with self.assertRaisesRegex(ValueError, "Invalid or oversized bundle"):
                package.stage_bundles(compiled, Path(directory) / "missing", "10.0.0")

    @unittest.skipUnless((package.EXTENSION / "out" / "explorerWebview.js").is_file(), "compiled webview unavailable")
    def test_actual_bundles_stage_with_the_pinned_preact_notice(self):
        manifest = json.loads((package.EXTENSION / "package.json").read_text(encoding="utf-8"))
        with tempfile.TemporaryDirectory() as directory:
            stage = Path(directory) / "out"
            package.stage_bundles(package.EXTENSION / "out", stage, manifest["devDependencies"]["preact"])
            self.assertIn("Permission is hereby granted", (stage / "explorerWebview.js").read_text(encoding="utf-8"))

    def test_lock_pins_esbuild_binaries_for_release_targets(self):
        lock = json.loads((package.EXTENSION / "package-lock.json").read_text(encoding="utf-8"))["packages"]
        manifest = json.loads((package.EXTENSION / "package.json").read_text(encoding="utf-8"))
        version = manifest["devDependencies"]["esbuild"]
        self.assertEqual(lock["node_modules/esbuild"]["version"], version)
        for platform in package.TARGETS.values():
            entry = lock.get("node_modules/@esbuild/" + platform)
            self.assertIsNotNone(entry, "lockfile lacks the esbuild binary for " + platform)
            self.assertEqual(entry["version"], version)
            self.assertTrue(entry.get("optional"))

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
