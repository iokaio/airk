# SPDX-License-Identifier: Apache-2.0
import hashlib
import importlib.util
import json
import re
import shutil
import subprocess
from pathlib import Path
import tempfile
import unittest
from unittest import mock
import zipfile

spec = importlib.util.spec_from_file_location("package_extension", Path(__file__).parents[1] / "package-extension.py")
package = importlib.util.module_from_spec(spec)
spec.loader.exec_module(package)

SPLIT = "1.8.24"
COMMIT = "a" * 40
TREE = "c" * 40
MANIFEST = '<PackageManifest><Metadata><Identity Id="airk" Publisher="iokaio" Version="{version}"{platform}/><DisplayName>AIRK</DisplayName><Properties>{flag}</Properties></Metadata></PackageManifest>'


def vsix_manifest(version, platform=None, preview=False):
    return MANIFEST.format(version=version, platform=f' TargetPlatform="{platform}"' if platform else "",
                           flag='<Property Id="Microsoft.VisualStudio.Code.PreRelease" Value="true"/>' if preview else "")


def write_vsix(archive, values, manifest, modes=None):
    with zipfile.ZipFile(archive, "w") as zipped:
        for name, data in values.items():
            stored = "readme.md" if name == "README.md" else "LICENSE.txt" if name == "LICENSE" else name
            info = zipfile.ZipInfo("extension/" + stored)
            info.external_attr = (modes or {}).get(name, 0o100644) << 16
            zipped.writestr(info, data)
        zipped.writestr("[Content_Types].xml", "<Types/>")
        zipped.writestr("extension.vsixmanifest", manifest)


def universal_values(version=SPLIT):
    """A sealed, fictional universal stage: the loader graph, legal files and its inventory."""
    values = {name: b"fictional sealed interface asset: " + name.encode() for name in package.update_inventory_paths()}
    values["package.json"] = json.dumps({"name": "airk", "publisher": "iokaio", "version": version}).encode()
    values["README.md"] = b"Fictional listing without images"
    values["LICENSE"] = b"fixture license"
    values["airk-update.json"] = package.encode_update_inventory(
        {name: package.digest(values[name]) for name in package.update_inventory_paths()},
        version, values["package.json"].decode())
    return values


def universal_candidate(directory, version=SPLIT, preview=False, commit=COMMIT, tree=TREE):
    """A complete universal candidate directory (archive, C7 receipt, SHA256SUMS)."""
    directory.mkdir(parents=True)
    values = universal_values(version)
    archive = directory / f"airk-{version}.vsix"
    write_vsix(archive, values, vsix_manifest(version, preview=preview))
    expected = {name: package.digest(data) for name, data in values.items()}
    receipt = {"schema": "airk-extension-candidate/2", "version": version, "commit": commit, "tree": tree,
               "extension_id": "iokaio.airk", "display_name": "AIRK", "pre_release": preview, "target": "universal",
               "i2_contracts": [1], "required_capabilities": ["session/2", "tools/1"],
               "ui_inventory_sha256": package.digest(values["airk-update.json"]),
               "locks": {"packages/vscode/package-lock.json": package.digest((package.ROOT / package.LOCK).read_bytes())},
               "node": "v24.15.0", "vsce": "4.0.1-3", "archive": archive.name,
               "archive_sha256": package.digest(archive.read_bytes()), "payload": expected,
               "qualification": "verified universal package; installed-editor qualification separate"}
    package.receipts.validate_vsix(receipt)
    (directory / "manifest.json").write_text(json.dumps(receipt, indent=2) + "\n", encoding="utf-8")
    (directory / "SHA256SUMS").write_text(
        f"{package.digest(archive.read_bytes())}  {archive.name}\n"
        f"{package.digest((directory / 'manifest.json').read_bytes())}  manifest.json\n", encoding="utf-8")
    return receipt, archive


class VsixContracts(unittest.TestCase):
    def test_vsix_model_notice_owner_retains_exact_public_companions_and_rejects_extra_files(self):
        expected = {"LICENSE": "minilm-v2-LICENSE.txt", "TRAINING-NOTICES.md": "minilm-v2-TRAINING-NOTICES.md",
                    "training-provenance.json": "minilm-v2-provenance.json", "CC-BY-4.0.txt": "CC-BY-4.0.txt"}
        with tempfile.TemporaryDirectory() as directory:
            stage = Path(directory)
            package.stage_model_notices(stage)
            notice = stage / "notices/all-MiniLM-L6-v2"
            self.assertEqual({p.name for p in notice.iterdir()}, {*expected, "provenance.json"})
            for output, source in expected.items():
                self.assertEqual((notice / output).read_bytes(), (package.ROOT / "third_party/models" / source).read_bytes())
            self.assertEqual((notice / "provenance.json").read_bytes(), (package.ROOT / "crates/airk-retrieval/src/model_catalog.json").read_bytes())
            (notice / "not-reviewed.txt").write_text("must not ship")
            with self.assertRaisesRegex(ValueError, "Unexpected trained-model notice inventory"):
                package.stage_model_notices(stage)

    def test_finished_vsix_refuses_unresolvable_listing_images_even_with_valid_hashes(self):
        for image in ('![Workbench](images/workbench.png)', '<img src="images/explorer.png">',
                      '![Workbench](http://example.com/workbench.png)',
                      '![Workbench](https://example.com/workbench.png)'):
            with self.subTest(image=image), tempfile.TemporaryDirectory() as temporary:
                archive = Path(temporary) / "fixture.vsix"
                files = {"readme.md": image.encode()}
                expected = {name: package.digest(data) for name, data in files.items()}
                with zipfile.ZipFile(archive, "w") as zipped:
                    for name, data in files.items():
                        zipped.writestr("extension/" + name, data)
                    zipped.writestr("[Content_Types].xml", "<Types/>")
                    zipped.writestr("extension.vsixmanifest", '<PackageManifest><Identity Id="airk" Publisher="iokaio"/></PackageManifest>')
                if image.startswith('![Workbench](https:'):
                    package.verify_vsix(archive, expected)
                else:
                    with self.assertRaisesRegex(ValueError, "absolute HTTPS"):
                        package.verify_vsix(archive, expected)

    def test_uninstall_hook_survives_packaging_without_development_scripts(self):
        source = json.loads((package.EXTENSION / "package.json").read_text(encoding="utf-8"))
        staged = package.runtime_package(source)
        self.assertEqual(staged["scripts"], {"vscode:uninstall": "node ./out/uninstallHook.js"})
        self.assertIn("uninstallHook.js", package.MODULES)
        self.assertNotIn("devDependencies", staged)
        self.assertIn("test", source["scripts"])
        self.assertEqual(staged["dependencies"], {"jsonc-parser": "3.3.1"})
        self.assertIn("@sigstore/verify", source["dependencies"], "development pins remain intact")
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
                    "license": "Apache-2.0", "files": [name for name in package.runtime_package(manifest)["files"] if name != "airk-update.json"],
                    "dependencies": package.runtime_package(manifest)["dependencies"]}), encoding="utf-8")
                (root / "README.md").write_text("AIRK packaging fixture", encoding="utf-8")
                (root / "LICENSE").write_text("Fixture legal file", encoding="utf-8")
                for name in ("out/fixture.js", "out/airkctl/agents/fixture.js", "out/fixture.css", "images/fixture.png",
                             "images/fixture.svg", "NOTICE", "THIRD_PARTY_NOTICES.md", "out/help-manifest.json"):
                    file = root / name
                    file.parent.mkdir(parents=True, exist_ok=True)
                    file.write_bytes(b"public packaging fixture")
                package.stage_model_notices(root)
                expected = {file.relative_to(root).as_posix(): package.digest(file.read_bytes()) for file in root.rglob("*") if file.is_file()}
                archive = root / "fixture.vsix"
                subprocess.run(["node", str(package.EXTENSION / "scripts" / "package.cjs"), str(root), str(archive)], check=True, capture_output=True)
                # Universal: VSCE wrote no TargetPlatform and the archive holds only the stage.
                package.verify_vsix(archive, expected, version="1.1.2", pre_release=False)

                refused = subprocess.run(["node", str(package.EXTENSION / "scripts" / "package.cjs"), str(root),
                                          "win32-x64", str(root / "platform.vsix")], capture_output=True)
                self.assertNotEqual(refused.returncode, 0, "the packager never accepts a target platform")
                self.assertFalse((root / "platform.vsix").exists())
            source = stage / "lib" / "umd" / "main.js"
            source.write_text('require("./missing");', encoding="utf-8")
            with self.assertRaisesRegex(ValueError, "Unpackaged jsonc-parser dependency"):
                package.stage_jsonc_parser(stage, root / "rejected")

    def test_runtime_manifest_refuses_unknown_externals_and_parser_drift(self):
        source = json.loads((package.EXTENSION / "package.json").read_text(encoding="utf-8"))
        source["dependencies"] = {**source["dependencies"], "unreviewed-package": "1.0.0"}
        with self.assertRaisesRegex(ValueError, "Unreviewed external"):
            package.runtime_package(source)
        del source["dependencies"]["unreviewed-package"]
        source["dependencies"]["jsonc-parser"] = "0.0.0"
        with self.assertRaisesRegex(ValueError, "shipped pinned jsonc-parser"):
            package.runtime_package(source)

    def test_update_inventory_seals_the_complete_staged_host_ui_and_parser_graph(self):
        with tempfile.TemporaryDirectory() as temporary:
            stage = Path(temporary)
            expected_paths = {"package.json", "images/airk.svg"}
            expected_paths.update("out/" + name for name in package.MODULES + package.BUNDLES)
            expected_paths.update("node_modules/jsonc-parser/" + name for name in package.JSONC_FILES
                                  if name.endswith((".js", ".json")))
            self.assertNotIn("native/manifest.json", package.update_inventory_paths())
            self.assertEqual(set(package.update_inventory_paths(native=True)) - expected_paths, {"native/manifest.json"})
            for name in expected_paths:
                file = stage / name
                file.parent.mkdir(parents=True, exist_ok=True)
                file.write_text("fictional staged bytes: " + name, encoding="utf-8")
            manifest_bytes = b'{"name":"airk","publisher":"iokaio","version":"1.8.12","floating":1e-7,"unicode":"\\u00e9"}\n'
            (stage / "package.json").write_bytes(manifest_bytes)
            package.write_update_inventory(stage, "1.8.12")
            inventory = json.loads((stage / "airk-update.json").read_text(encoding="utf-8"))
            self.assertEqual(inventory["schema"], "airk-extension-update/1")
            self.assertEqual(inventory["version"], "1.8.12")
            self.assertEqual(inventory["package_manifest"].encode("utf-8"), manifest_bytes)
            self.assertEqual(package.digest(inventory["package_manifest"].encode("utf-8")), inventory["files"]["package.json"])
            self.assertEqual(set(inventory["files"]), expected_paths)
            self.assertEqual(inventory["files"], {name: package.digest((stage / name).read_bytes())
                                                 for name in expected_paths})
            before = inventory["files"]["out/explorerWebview.css"]
            (stage / "out/explorerWebview.css").write_text("fictional replacement", encoding="utf-8")
            package.write_update_inventory(stage, "1.8.12")
            self.assertNotEqual(json.loads((stage / "airk-update.json").read_text())["files"]["out/explorerWebview.css"], before)
            (stage / "out/airkctl/index.js").unlink()
            with self.assertRaises(FileNotFoundError):
                package.write_update_inventory(stage, "1.8.12")

    def test_sealed_names_follow_the_update_loader_rule(self):
        # The loader (extensionUpdateLoader.ts) refuses any inventory name outside its
        # pattern; read it from the TypeScript owner rather than trusting the copy here.
        loader = (package.EXTENSION / "src" / "extensionUpdateLoader.ts").read_text(encoding="utf-8")
        pattern = re.search(r"if \(!/\^\(\?:(.*?)\)\$/\.test\(name\)", loader)
        self.assertIsNotNone(pattern, "update loader name rule not found")
        rule = re.compile("(?:" + pattern.group(1).replace("\\/", "/") + ")")
        self.assertEqual(rule.pattern, package.LOADER_NAME.pattern)
        for name in package.update_inventory_paths():
            self.assertRegex(name, package.LOADER_NAME)
        # The loader's required set is part of every sealed inventory.
        required = re.search(r"for \(const required of \[(.*?)\]\)", loader).group(1)
        for name in re.findall(r"'([^']+)'", required):
            self.assertIn(name, package.update_inventory_paths())

    def test_finished_vsix_checks_the_loader_inventory_even_when_outer_hashes_are_resealed(self):
        original = universal_values()
        with tempfile.TemporaryDirectory() as temporary:
            archive = Path(temporary) / "fixture.vsix"
            for mutation in (None, "native_entry", "ui", "missing", "incomplete", "snapshot", "metadata", "outside_rule"):
                with self.subTest(mutation=mutation):
                    values = dict(original)
                    if mutation == "native_entry":
                        inventory = json.loads(values["airk-update.json"])
                        values["native/manifest.json"] = b'{"version": "1.8.24"}'
                        inventory["files"]["native/manifest.json"] = package.digest(values["native/manifest.json"])
                        values["airk-update.json"] = json.dumps(inventory).encode()
                    elif mutation == "ui":
                        values["out/explorerWebview.js"] += b" changed"
                    elif mutation == "missing":
                        del values["airk-update.json"]
                    elif mutation in ("incomplete", "snapshot", "outside_rule"):
                        inventory = json.loads(values["airk-update.json"])
                        if mutation == "incomplete":
                            del inventory["files"]["out/explorerController.js"]
                        elif mutation == "outside_rule":
                            values["out/generated/i2.js"] = b"fictional unreachable module"
                            inventory["files"]["out/generated/i2.js"] = package.digest(values["out/generated/i2.js"])
                        else:
                            inventory["package_manifest"] += " "
                        values["airk-update.json"] = json.dumps(inventory).encode()
                    elif mutation == "metadata":
                        # Archives seal source bytes. Only the installed loader,
                        # after VS Code installation, accepts __metadata rewrites.
                        manifest = json.loads(values["package.json"])
                        values["package.json"] = json.dumps({**manifest, "__metadata": {"installedTimestamp": 1}}).encode()
                    write_vsix(archive, values, vsix_manifest(SPLIT))
                    expected = {name: package.digest(data) for name, data in values.items()}
                    if mutation is None:
                        package.verify_vsix(archive, expected, version=SPLIT)
                    elif mutation == "native_entry":
                        with self.assertRaisesRegex(ValueError, r"forbidden content \(native subtree\)"):
                            package.verify_vsix(archive, expected, version=SPLIT)
                    else:
                        with self.assertRaisesRegex(ValueError, "update inventory|Reviewed extension"):
                            package.verify_vsix(archive, expected, version=SPLIT)

    def test_universal_vsix_refuses_native_and_executable_content(self):
        cases = {
            "native/runtime/airk.exe": (b"fictional", 0o100644, "native subtree"),
            "out/helper.dll": (b"fictional", 0o100644, "suffix"),
            "out/addon.node": (b"fictional", 0o100644, "suffix"),
            "out/libort.so": (b"fictional", 0o100644, "suffix"),
            "out/tool.js": (b"console.log(1)", 0o100755, "executable permission"),
            "images/disguised.png": (b"MZ\x90\x00fictional PE", 0o100644, "executable image header"),
            "images/elf.png": (b"\x7fELF fictional", 0o100644, "executable image header"),
            "images/macho.png": (b"\xcf\xfa\xed\xfe fictional", 0o100644, "executable image header"),
        }
        with tempfile.TemporaryDirectory() as temporary:
            archive = Path(temporary) / "fixture.vsix"
            for name, (data, mode, reason) in cases.items():
                with self.subTest(name=name):
                    values = {**universal_values(), name: data}
                    write_vsix(archive, values, vsix_manifest(SPLIT), {name: mode})
                    expected = {key: package.digest(value) for key, value in values.items()}
                    with self.assertRaisesRegex(ValueError, "forbidden content.*" + reason):
                        package.verify_vsix(archive, expected, version=SPLIT)
            # The same rule runs on the stage before VSCE, so nothing is packaged.
            stage = Path(temporary) / "stage"
            (stage / "out").mkdir(parents=True)
            (stage / "out" / "extension.js").write_text("'use strict';", encoding="utf-8")
            package.check_stage_content(stage)
            (stage / "out" / "helper.exe").write_bytes(b"fictional")
            with self.assertRaisesRegex(ValueError, "stage contains forbidden content"):
                package.check_stage_content(stage)

    def test_universal_identity_channel_and_tampering(self):
        values = universal_values()
        expected = {name: package.digest(data) for name, data in values.items()}
        with tempfile.TemporaryDirectory() as directory:
            archive = Path(directory) / "fixture.vsix"
            write_vsix(archive, values, vsix_manifest(SPLIT, preview=True))
            package.verify_vsix(archive, expected, version=SPLIT, pre_release=True, display_name="AIRK")
            with self.assertRaisesRegex(ValueError, "pre-release flag differs"):
                package.verify_vsix(archive, expected, pre_release=False)
            with self.assertRaisesRegex(ValueError, "display name differs"):
                package.verify_vsix(archive, expected, display_name="Other label")
            # The sealed inventory names the version too; it refuses first.
            with self.assertRaisesRegex(ValueError, "inventory manifest identity differs"):
                package.verify_vsix(archive, expected, version="1.8.25")
            with self.assertRaisesRegex(ValueError, "payload differs"):
                package.verify_vsix(archive, dict(expected, **{"out/extension.js": "0" * 64}))
            with self.assertRaisesRegex(ValueError, "platform VSIX is not accepted"):
                package.verify_vsix(archive, expected, version=SPLIT, platform="win32-x64")
            write_vsix(archive, values, vsix_manifest(SPLIT, platform="win32-x64"))
            with self.assertRaisesRegex(ValueError, "must not declare a TargetPlatform"):
                package.verify_vsix(archive, expected, version=SPLIT)
            with zipfile.ZipFile(archive, "w") as zipped:
                for name, data in values.items():
                    stored = "readme.md" if name == "README.md" else "LICENSE.txt" if name == "LICENSE" else name
                    zipped.writestr("extension/" + stored, data)
                zipped.writestr("extension/private.env", "fixture must fail")
                zipped.writestr("[Content_Types].xml", "<Types/>")
                zipped.writestr("extension.vsixmanifest", vsix_manifest(SPLIT))
            with self.assertRaisesRegex(ValueError, "payload differs"):
                package.verify_vsix(archive, expected)

    def test_universal_receipt_is_the_validated_c7_shape(self):
        with tempfile.TemporaryDirectory() as directory:
            receipt, archive = universal_candidate(Path(directory) / "candidate")
            stage = Path(directory) / "stage"
            stage.mkdir()
            (stage / "airk-update.json").write_bytes(universal_values()["airk-update.json"])
            package_json = {"name": "airk", "publisher": "iokaio", "version": SPLIT, "displayName": "AIRK",
                            "devDependencies": {"@vscode/vsce": "4.0.1-3"}}
            built = package.universal_receipt(package=package_json, commit=COMMIT, tree=TREE, pre_release=False,
                                              stage=stage, vsix=archive, expected=receipt["payload"],
                                              node_version="v24.15.0", requirements=([1], ["session/2", "tools/1"]))
            self.assertEqual(built, receipt)
            for name in ("native/runtime/airk.exe", "out/helper.dll"):
                with self.assertRaises(package.receipts.ReceiptError):
                    package.universal_receipt(package=package_json, commit=COMMIT, tree=TREE, pre_release=False,
                                              stage=stage, vsix=archive, expected={**receipt["payload"], name: "0" * 64},
                                              node_version="v24.15.0", requirements=([1], []))

    def test_universal_packaging_starts_at_the_split_version(self):
        self.assertEqual(package.receipts.SPLIT_VERSION, (1, 8, 24))
        self.assertFalse(package.receipts.split_version("1.8.23"))
        self.assertTrue(package.receipts.split_version("1.8.24"))
        self.assertTrue(package.receipts.split_version("1.9.0"))
        for invalid in ("1.8", "v1.8.24", "1.08.24", ""):
            with self.assertRaises(ValueError):
                package.receipts.split_version(invalid)

    def test_listing_product_screenshots_are_explicit_bounded_public_assets(self):
        listing = (package.EXTENSION / "README.md").read_text(encoding="utf-8")
        for name, component in package.PRODUCT_ILLUSTRATIONS.items():
            source = package.ROOT / "docs" / component / "images" / name
            self.assertFalse(source.is_symlink())
            self.assertTrue(source.is_file())
            self.assertLessEqual(source.stat().st_size, 2_097_152)
            self.assertIn(f"https://raw.githubusercontent.com/iokaio/airk/main/docs/{component}/images/{name}", listing)

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

    @mock.patch.object(package, "verify_bootstrap_runtime")
    @mock.patch.object(package, "verify_bootstrap_release_runtime")
    def test_runtime_import_closure_includes_controls_without_test_extras(self, verify_release, verify_bootstrap):
        with tempfile.TemporaryDirectory() as directory:
            compiled = Path(directory) / "compiled"
            for name in package.MODULES:
                (compiled / name).parent.mkdir(parents=True, exist_ok=True)
                (compiled / name).write_text('"use strict";\n', encoding="utf-8")
            (compiled / "uiPilotRuntime.js").write_bytes((
                "/* SPDX-License-Identifier: Apache-2.0 */\n/* AIRK UI pilot runtime source-sha256: "
                + package.pilot_runtime_source_seal() + " */\nexports.createNativePilot=function(){};\n").encode("utf-8"))
            (compiled / "extension.js").write_text(
                'require("vscode"); require("./serviceExtension"); require("./extensionUpdateLoader.js");', encoding="utf-8")
            (compiled / "serviceExtension.js").write_text(
                'require("./airkctl/index"); require("./airkctl/agents/index.js");', encoding="utf-8")
            (compiled / "airkctl" / "index.js").write_text('require("node:fs"); require("../model"); require("./runner");', encoding="utf-8")
            (compiled / "test").mkdir()
            (compiled / "test" / "private.test.js").write_text("not runtime", encoding="utf-8")
            (compiled / "extension.js.map").write_text("not runtime", encoding="utf-8")
            (compiled / "generated").mkdir()
            (compiled / "generated" / "i2.js").write_text("not runtime", encoding="utf-8")
            stage = Path(directory) / "stage"
            package.stage_runtime_modules(compiled, stage)
            verify_bootstrap.assert_called_once_with((compiled / "airkctl/bootstrap/provenance.js").read_bytes())
            verify_release.assert_called_once_with((compiled / "airkctl/bootstrap/release.js").read_bytes())
            self.assertEqual({p.relative_to(stage).as_posix() for p in stage.rglob("*") if p.is_file()}, set(package.MODULES))
            for name in package.MODULES:
                self.assertEqual((stage / name).read_bytes(), (compiled / name).read_bytes())
            # A directory import is unloadable by the isolated update loader (x, x.js, x.json only).
            for source, missing in (("serviceExtension.js", "./futureControl"), ("serviceExtension.js", "./test/private.test"),
                                    ("serviceExtension.js", "../outside"), ("serviceExtension.js", "./airkctl"),
                                    ("serviceExtension.js", "./generated/i2"), ("airkctl/index.js", "../../escape"),
                                    ("airkctl/index.js", "./agents")):
                with self.subTest(source=source, request=missing):
                    original = (compiled / source).read_bytes()
                    (compiled / source).write_text('require("' + missing + '");', encoding="utf-8")
                    rejected = Path(directory) / "rejected"
                    with self.assertRaisesRegex(ValueError, "Runtime import outside allowlisted stage"):
                        package.stage_runtime_modules(compiled, rejected)
                    self.assertFalse(rejected.exists(), "Validate closure before staging any runtime modules")
                    (compiled / source).write_bytes(original)

    def test_ui_pilot_runtime_requires_current_source_and_no_extra_loader(self):
        with tempfile.TemporaryDirectory() as directory:
            root = Path(directory)
            sources = ("packages/ui/src/nativePilot.ts", "packages/ui/src/pilotContract.ts", "packages/vscode/src/uiPilotContract.ts", "packages/vscode/scripts/build-explorer.cjs")
            for source in sources:
                target = root / source
                target.parent.mkdir(parents=True, exist_ok=True)
                target.write_text("// " + source + "\n", encoding="utf-8")
            seal = package.pilot_runtime_source_seal(root)
            runtime = ("/* SPDX-License-Identifier: Apache-2.0 */\n/* AIRK UI pilot runtime source-sha256: "
                       + seal + " */\nexports.createNativePilot=function(){};\n").encode("ascii")
            package.verify_pilot_runtime(runtime, root)
            with self.assertRaisesRegex(ValueError, "current source seal"):
                package.verify_pilot_runtime(b'"use strict";\n', root)
            with self.assertRaisesRegex(ValueError, "must not load external modules"):
                package.verify_pilot_runtime(runtime + b'require("./extra");', root)
            for source in sources:
                target = root / source
                original = target.read_bytes()
                target.write_bytes(original + b"// changed\n")
                with self.subTest(changed=source), self.assertRaisesRegex(ValueError, "current source seal"):
                    package.verify_pilot_runtime(runtime, root)
                target.write_bytes(original)

    @mock.patch.object(package, "verify_bootstrap_runtime")
    @mock.patch.object(package, "verify_bootstrap_release_runtime")
    def test_ui_pilot_runtime_missing_refuses_before_any_staging(self, _verify_release, _verify_bootstrap):
        with tempfile.TemporaryDirectory() as directory:
            compiled = Path(directory) / "compiled"
            for name in package.MODULES:
                if name == "uiPilotRuntime.js":
                    continue
                target = compiled / name
                target.parent.mkdir(parents=True, exist_ok=True)
                target.write_text('"use strict";\n', encoding="utf-8")
            destination = Path(directory) / "stage"
            with self.assertRaisesRegex(ValueError, "Invalid or oversized runtime module: uiPilotRuntime.js"):
                package.stage_runtime_modules(compiled, destination)
            self.assertFalse(destination.exists())

    def test_bootstrap_runtime_uses_owning_node_verifier_on_exact_captured_bytes(self):
        captured = b"// exact captured bundle, not a reread\n"
        with mock.patch.object(package.subprocess, "run", return_value=subprocess.CompletedProcess([], 0, b"verified\n", b"")) as run:
            package.verify_bootstrap_runtime(captured)
            self.assertEqual(run.call_args.args[0], ["node", str(package.ROOT / "packages/vscode/scripts/build-bootstrap.cjs"), "--verify-stdin"])
            self.assertEqual(run.call_args.kwargs["input"], captured)
            self.assertEqual(run.call_args.kwargs["cwd"], package.ROOT)
            self.assertEqual(run.call_args.kwargs["timeout"], 30)
        for result in [subprocess.CompletedProcess([], 1, b"", b"refused"), subprocess.CompletedProcess([], 0, b"", b""),
                       subprocess.CompletedProcess([], 0, b"verified\nextra\n", b"")]:
            with self.subTest(returncode=result.returncode, stdout=result.stdout), mock.patch.object(package.subprocess, "run", return_value=result):
                with self.assertRaisesRegex(ValueError, "failed owner verification"):
                    package.verify_bootstrap_runtime(captured)
        with mock.patch.object(package.subprocess, "run", side_effect=subprocess.TimeoutExpired("node", 30)):
            with self.assertRaisesRegex(ValueError, "verification unavailable"):
                package.verify_bootstrap_runtime(captured)

    def test_bootstrap_release_uses_native_binding_owner_on_exact_captured_bytes(self):
        captured = b"// captured release bundle\n"
        with mock.patch.object(package.subprocess, "run", return_value=subprocess.CompletedProcess([], 0, b"verified\n", b"")) as run:
            package.verify_bootstrap_release_runtime(captured)
            self.assertEqual(run.call_args.args[0][-1], "--verify-release-stdin")
            self.assertEqual(run.call_args.kwargs["input"], captured)
        for result in (subprocess.CompletedProcess([], 1, b"", b"missing native capture"),
                       subprocess.CompletedProcess([], 0, b"verified\nextra\n", b"")):
            with self.subTest(result=result), mock.patch.object(package.subprocess, "run", return_value=result):
                with self.assertRaisesRegex(ValueError, "release bundle failed owner verification"):
                    package.verify_bootstrap_release_runtime(captured)

    @unittest.skipUnless((package.EXTENSION / "out" / "extension.js").is_file(), "compiled extension unavailable")
    def test_actual_compiled_runtime_imports_are_all_staged(self):
        with tempfile.TemporaryDirectory() as directory:
            stage = Path(directory) / "out"
            package.stage_runtime_modules(package.EXTENSION / "out", stage)
            self.assertEqual({p.relative_to(stage).as_posix() for p in stage.rglob("*") if p.is_file()}, set(package.MODULES))
            contracts, capabilities = package.extension_requirements(Path(directory))
            self.assertEqual(contracts, [1])
            self.assertIn("session/2", capabilities)

    def test_bundles_are_self_contained_and_carry_the_preact_notice(self):
        banner = "/*! AIRK Explorer webview. Includes Preact 10.0.0 (MIT).\nPermission is hereby granted\n*/\n"
        with tempfile.TemporaryDirectory() as directory:
            compiled = Path(directory) / "compiled"
            compiled.mkdir()
            (compiled / "explorerWebview.js").write_bytes((banner + "(()=>{document.body;})();\n").encode("utf-8"))
            (compiled / "explorerWebview.css").write_bytes(b"body{margin:0}\n")
            (compiled / "workbenchWebview.js").write_bytes((banner + "(()=>{document.body;})();\n").encode("utf-8"))
            (compiled / "workbenchWebview.css").write_bytes(b"body{margin:0}\n")
            assistance = "/*! AIRK Explorer Assistance MCP server 1.0.0. No third-party code. */\n"
            (compiled / "explorerAssistance.js").write_bytes((assistance + 'require("node:crypto");\n').encode("utf-8"))
            version = json.loads((package.EXTENSION / "package.json").read_text(encoding="utf-8"))["version"]
            cli = {"example": 'require("./documented-example")'}
            help_bytes = json.dumps({"schema": "airk.help.manifest/1", "productVersion": version, "extensionVersion": version,
                                     "channel": None, "catalogVersion": 1, "topics": [], "cliCommands": cli,
                                     "helpRequestSchema": {}, "tools": [], "native": {"version": version,
                                         "schema": "airk.help.native-content/1", "cliCommands": cli, "helpRequestSchema": {}, "tools": []}}).encode("utf-8")
            help_source = Path(directory) / "source-help.json"
            help_source.write_bytes(help_bytes)
            (compiled / "help-manifest.json").write_bytes(help_bytes)
            stage = Path(directory) / "stage"
            package.stage_bundles(compiled, stage, "10.0.0", help_source)
            self.assertEqual({p.name for p in stage.iterdir()}, set(package.BUNDLES))
            self.assertEqual((stage / "help-manifest.json").read_bytes(), help_bytes)
            (compiled / "help-manifest.json").write_bytes(help_bytes + b"\n")
            with self.assertRaisesRegex(ValueError, "Help manifest is stale"):
                package.stage_bundles(compiled, Path(directory) / "stale-help", "10.0.0", help_source)
            self.assertFalse((Path(directory) / "stale-help").exists())
            changed_help = json.loads(help_bytes)
            changed_help["extensionVersion"] = "0.0.0"
            invalid_help = json.dumps(changed_help).encode("utf-8")
            (compiled / "help-manifest.json").write_bytes(invalid_help)
            help_source.write_bytes(invalid_help)
            with self.assertRaisesRegex(ValueError, "invalid schema or version"):
                package.stage_bundles(compiled, Path(directory) / "wrong-help-version", "10.0.0", help_source)
            self.assertFalse((Path(directory) / "wrong-help-version").exists())
            (compiled / "help-manifest.json").write_bytes(help_bytes)
            help_source.write_bytes(help_bytes)
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
                    package.stage_bundles(compiled, target, "10.0.0", help_source)
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
                    package.stage_bundles(compiled, Path(directory) / "assistance", "10.0.0", help_source)
            (compiled / "explorerAssistance.js").write_bytes(b"require('node:fs');")
            with self.assertRaisesRegex(ValueError, "identifying banner"):
                package.stage_bundles(compiled, Path(directory) / "assistance", "10.0.0", help_source)
            (compiled / "explorerAssistance.js").write_bytes(assistance.encode("utf-8"))
            with self.assertRaisesRegex(ValueError, "pinned Preact notice"):
                package.stage_bundles(compiled, Path(directory) / "wrong-version", "10.0.1", help_source)
            (compiled / "explorerWebview.css").unlink()
            with self.assertRaisesRegex(ValueError, "Invalid or oversized bundle"):
                package.stage_bundles(compiled, Path(directory) / "missing", "10.0.0", help_source)

    @unittest.skipUnless((package.EXTENSION / "out" / "explorerWebview.js").is_file(), "compiled webview unavailable")
    def test_actual_bundles_stage_with_the_pinned_preact_notice(self):
        manifest = json.loads((package.EXTENSION / "package.json").read_text(encoding="utf-8"))
        with tempfile.TemporaryDirectory() as directory:
            stage = Path(directory) / "out"
            package.stage_bundles(package.EXTENSION / "out", stage, manifest["devDependencies"]["preact"])
            self.assertIn("Permission is hereby granted", (stage / "explorerWebview.js").read_text(encoding="utf-8"))

    def test_lock_pins_esbuild_binaries_for_release_targets(self):
        # The universal VSIX is built on any CI host; each host compiles with the locked esbuild.
        lock = json.loads((package.EXTENSION / "package-lock.json").read_text(encoding="utf-8"))["packages"]
        manifest = json.loads((package.EXTENSION / "package.json").read_text(encoding="utf-8"))
        version = manifest["devDependencies"]["esbuild"]
        self.assertEqual(lock["node_modules/esbuild"]["version"], version)
        for platform in ("win32-x64", "linux-x64", "darwin-arm64", "darwin-x64"):
            entry = lock.get("node_modules/@esbuild/" + platform)
            self.assertIsNotNone(entry, "lockfile lacks the esbuild binary for " + platform)
            self.assertEqual(entry["version"], version)
            self.assertTrue(entry.get("optional"))

    def test_historical_platform_identity_and_permission_checks(self):
        # Pre-split platform VSIXs stay verifiable through the explicit platform path.
        with tempfile.TemporaryDirectory() as directory:
            archive = Path(directory) / "fixture.vsix"
            files = {"package.json": b"{}", "README.md": b"fixture readme", "LICENSE": b"fixture license", "native/airk": b"fixture executable"}
            expected = {name: hashlib.sha256(data).hexdigest() for name, data in files.items()}
            write_vsix(archive, files, vsix_manifest("0.1.0", "linux-x64", preview=True), {"native/airk": 0o100755})
            package.verify_vsix(archive, expected, platform="linux-x64", version="0.1.0", pre_release=True, display_name="AIRK")
            with self.assertRaisesRegex(ValueError, "platform differs"):
                package.verify_vsix(archive, expected, platform="darwin-arm64", version="0.1.0")
            with self.assertRaisesRegex(ValueError, "native subtree"):
                package.verify_vsix(archive, expected, version="0.1.0")
            write_vsix(archive, files, vsix_manifest("0.1.0", "linux-x64"), {"native/airk": 0o100644})
            with self.assertRaisesRegex(ValueError, "native executable permission"):
                package.verify_vsix(archive, expected, platform="linux-x64")

    def test_path_escape_refused(self):
        for name in ("../private", "/private", "native/../../private", "C:/private", "native\\private",
                     "native/./airk", "native//airk", "", "native/airk/"):
            with self.assertRaises(ValueError):
                package.safe_name(name)

    def test_merge_ref_candidates_match_only_an_identical_source_tree(self):
        commit, tree = "a" * 40, "c" * 40
        self.assertTrue(package.same_source({"commit": commit}, commit, None))
        self.assertTrue(package.same_source({"commit": "d" * 40, "tree": tree}, commit, tree))
        for receipt, expected_tree in (({"commit": "d" * 40, "tree": tree}, None),
                                       ({"commit": "d" * 40, "tree": "b" * 40}, tree),
                                       ({"commit": "d" * 40}, tree),
                                       ({"commit": "d" * 40, "tree": tree.upper()}, tree.upper()),
                                       ({"commit": "d" * 40, "tree": None}, None)):
            with self.subTest(receipt=receipt, tree=expected_tree):
                self.assertFalse(package.same_source(receipt, commit, expected_tree))
        actual = package.source_tree()
        self.assertRegex(actual, r"^[0-9a-f]{40}$")
        self.assertEqual(actual, subprocess.check_output(["git", "rev-parse", "HEAD^{tree}"], cwd=package.ROOT, text=True).strip())
        self.assertIsNone(package.source_tree("0" * 40), "an unknown revision fails comparisons closed")


if __name__ == "__main__":
    unittest.main()
