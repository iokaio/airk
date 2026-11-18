# SPDX-License-Identifier: Apache-2.0
"""Fictional payloads prove packaging boundaries, never native qualification."""
import importlib.util
import json
from pathlib import Path
import tempfile
import unittest
import zipfile
from scripts.tests import test_package_standalone_linux as fixtures

ROOT=Path(__file__).resolve().parents[2]
spec=importlib.util.spec_from_file_location("package_portable_tests",ROOT / "scripts/package-portable.py")
package=importlib.util.module_from_spec(spec);spec.loader.exec_module(package)


def candidate(base, version="1.8.35"):
    target="x86_64-unknown-linux-gnu"
    runtime=base / "runtime";runtime.mkdir()
    source={"sha256":"1"*64,"files":4}
    pair,build=fixtures.native_tests.split_runtime(runtime,target,source,version=version)
    if package.shortcuts.enabled(version):
        notices=runtime / "notices/all-MiniLM-L6-v2"
        (notices / "LICENSE").write_bytes((ROOT / "third_party/models/minilm-LICENSE.txt").read_bytes())
        (notices / "provenance.json").write_bytes((ROOT / "crates/airk-retrieval/src/model_catalog.json").read_bytes())
        package.native.runtime_builder().load_script("license-notices").stage_research_notices(ROOT,runtime / "notices")
        fixtures.native_tests.seal_split(runtime,pair,build)
    files,images=package.native.validate_runtime(runtime,version,target,source)
    native=base / "native";native.mkdir();archive=native / f"airk-{version}-{target}.zip"
    package.native.write_archive(runtime,archive,files,images)
    common={"version":version,"target":target,"commit":"c"*40,"tree":"d"*40,"dirty_checkout":False,"source":source,
        "cargo_lock_sha256":package.native.sha256(ROOT / "Cargo.lock"),"toolchain_file_sha256":package.native.sha256(ROOT / "rust-toolchain.toml"),
        "rustc":"rustc fixture","cargo":"cargo fixture","archive":archive.name,"archive_sha256":package.native.sha256(archive),
        "install_manifest_sha256":files["airk-install.json"],"payload":files,"bundled_model":{"manifest":"fixture.json"},"embedded_models":{},"signed_distribution":False,"signing":None}
    receipt=package.native.split_candidate(common,files,images,runtime,{"schema":1,"profile":"fixture"})
    fixtures.seal_receipt(native,receipt)
    return native,receipt


class PortablePackage(unittest.TestCase):
    def test_complete_runtime_bytes_roster_inventory_manuals_and_determinism(self):
        with tempfile.TemporaryDirectory(dir=ROOT / "artifacts") as directory:
            base=Path(directory);native,original=candidate(base)
            first=package.package(native,base / "first");second=package.package(native,base / "second")
            self.assertEqual(first["archive_sha256"],second["archive_sha256"])
            self.assertEqual(set(first["payload"]),{"README.txt","manual.html","payload-inventory.json","portable-manifest.json"}|{"runtime/"+name for name in original["payload"]})
            with zipfile.ZipFile(base / "first/publish" / first["archive"]) as archive:
                for name,digest in original["payload"].items():
                    self.assertEqual(package.native.hashlib.sha256(archive.read("runtime/"+name)).hexdigest(),digest)
                inventory=json.loads(archive.read("payload-inventory.json"))
                self.assertEqual(set(inventory["files"]),set(original["payload"]))
                self.assertEqual(inventory["executables"],{"airk":"airk","airkctl":"airkctl"})
                embedded=json.loads(archive.read("portable-manifest.json"))
                self.assertEqual(set(embedded["files"]),set(first["payload"])-{"portable-manifest.json"})
                for name,digest in embedded["files"].items():
                    self.assertEqual(package.native.hashlib.sha256(archive.read(name)).hexdigest(),digest)
                self.assertIn(b"service setup --portable --program",archive.read("README.txt"))
            self.assertEqual({path.name for path in (base / "first/publish").iterdir()},{first["archive"],package.contract.portable_manifest("1.8.35",original["target"]),"SHA256SUMS"})

    def test_new_portable_shortcut_is_outer_manifest_only(self):
        with tempfile.TemporaryDirectory(dir=ROOT / "artifacts") as directory:
            base=Path(directory);native,original=candidate(base, "1.9.0")
            result=package.package(native,base / "pack")
            with zipfile.ZipFile(base / "pack/publish" / result["archive"]) as archive:
                self.assertIn("rk", result["payload"])
                self.assertEqual(archive.read("rk"), b'#!/bin/sh\n# SPDX-License-Identifier: Apache-2.0\ncase "$0" in\n  */*) rk_dir=${0%/*} ;;\n  *) rk_dir=. ;;\nesac\nexec "$rk_dir/runtime/airkctl" "$@"\n')
                self.assertEqual((archive.getinfo("rk").external_attr >> 16) & 0o777, 0o755)
                self.assertNotIn("rk", json.loads(archive.read("payload-inventory.json"))["files"])
                self.assertEqual(set(original["payload"]),set(json.loads(archive.read("payload-inventory.json"))["files"]))

    def test_shortcut_target_literals_and_historical_absence(self):
        self.assertIsNone(package.shortcuts.portable("1.8.999", "x86_64-pc-windows-msvc"))
        name,body,executable=package.shortcuts.portable("1.9.0", "x86_64-pc-windows-msvc")
        self.assertEqual(name,"rk.cmd");self.assertFalse(executable)
        self.assertEqual(body,b'@REM SPDX-License-Identifier: Apache-2.0\r\n@echo off\r\nsetlocal EnableExtensions DisableDelayedExpansion\r\nset "ERRORLEVEL="\r\n"%~dp0runtime\\airkctl.exe" %*\r\nexit /b %errorlevel%\r\n')
        for target in ("aarch64-apple-darwin","x86_64-apple-darwin"):
            self.assertIn(b'exec "$rk_dir/runtime/AIRK.app/Contents/MacOS/airkctl" "$@"', package.shortcuts.portable("1.9.0",target)[1])
        with self.assertRaises(ValueError): package.shortcuts.portable("1.9.0","unknown")

    def test_changed_archive_and_unexpected_roster_refuse_before_output(self):
        with tempfile.TemporaryDirectory(dir=ROOT / "artifacts") as directory:
            base=Path(directory);native,receipt=candidate(base)
            (native / receipt["archive"]).write_bytes(b"changed")
            with self.assertRaisesRegex(ValueError,"digest differs"):
                package.package(native,base / "output")
            self.assertFalse((base / "output").exists())

    def test_four_target_manuals_and_cutoff(self):
        for target in ["x86_64-pc-windows-msvc","x86_64-unknown-linux-gnu","aarch64-apple-darwin","x86_64-apple-darwin"]:
            with self.subTest(target=target):
                self.assertIn("service setup --portable --program",package.instructions("1.8.35",target))
                self.assertIn('service setup --resume "<absolute retained operation-request.json>" --portable',package.instructions("1.9.0",target))
                self.assertNotIn("service setup --resume",package.instructions("1.8.35",target))
                self.assertIn("replace it.\nLinux distributions",package.instructions("1.8.35",target))
                self.assertTrue(package.contract.portable_artifact("1.8.35",target).endswith("-portable.zip"))
        self.assertFalse(package.contract.portable_required("1.8.34","stable"))
        self.assertTrue(package.contract.portable_required("1.8.35","preview"))

if __name__ == "__main__":unittest.main()
