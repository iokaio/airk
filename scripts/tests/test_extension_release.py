# SPDX-License-Identifier: Apache-2.0
import hashlib
import importlib.util
import json
from pathlib import Path
import tempfile
import unittest
import zipfile

spec = importlib.util.spec_from_file_location("release", Path(__file__).parents[1] / "verify-extension-release.py")
release = importlib.util.module_from_spec(spec)
spec.loader.exec_module(release)
COMMIT = "a" * 40


def digest(data):
    return hashlib.sha256(data).hexdigest()


class ReleaseContracts(unittest.TestCase):
    def fixture(self, directory, channel="preview"):
        for target, platform in release.package.TARGETS.items():
            output = directory / platform
            output.mkdir()
            executable = "airk.exe" if platform == "win32-x64" else "airk"
            native_files = {executable: b"original fictional executable", "LICENSE": b"fixture notice"}
            native = {"schema": "airk-native-candidate/1", "commit": COMMIT, "version": "0.1.0", "target": target, "vsix_target": platform,
                      "dirty_checkout": False, "archive_sha256": "b" * 64,
                      "toolchain_file_sha256": digest((release.ROOT / "rust-toolchain.toml").read_bytes()),
                      "cargo_lock_sha256": digest((release.ROOT / "Cargo.lock").read_bytes()),
                      "payload": {name: digest(data) for name, data in native_files.items()}}
            native_bytes = json.dumps(native).encode()
            files = {"package.json": b"{}", "README.md": b"fixture", "LICENSE": b"fixture",
                     "native/manifest.json": native_bytes,
                     **{"native/" + name: data for name, data in native_files.items()}}
            archive = output / f"airk-0.1.0-{platform}.vsix"
            with zipfile.ZipFile(archive, "w") as zipped:
                for name, data in files.items():
                    output_name = "readme.md" if name == "README.md" else "LICENSE.txt" if name == "LICENSE" else name
                    info = zipfile.ZipInfo("extension/" + output_name)
                    info.external_attr = (0o100755 if name == "native/" + executable else 0o100644) << 16
                    zipped.writestr(info, data)
                zipped.writestr("[Content_Types].xml", "<Types/>")
                flag = '<Property Id="Microsoft.VisualStudio.Code.PreRelease" Value="true"/>' if channel == "preview" else ""
                zipped.writestr("extension.vsixmanifest", f'<PackageManifest><Identity Id="airk" Publisher="iokaio" Version="0.1.0" TargetPlatform="{platform}"/><DisplayName>AIRK</DisplayName>{flag}</PackageManifest>')
            receipt = {"schema": "airk-extension-candidate/1", "commit": COMMIT, "version": "0.1.0",
                       "extension_id": "iokaio.airk", "display_name": "AIRK", "platform": platform,
                       "pre_release": channel == "preview", "archive": archive.name,
                       "archive_sha256": digest(archive.read_bytes()), "native_archive_sha256": "b" * 64,
                       "native_manifest_sha256": digest(native_bytes),
                       "npm_lock_sha256": digest((release.ROOT / "packages/vscode/package-lock.json").read_bytes()),
                       "payload": {name: digest(data) for name, data in files.items()}}
            (output / "manifest.json").write_text(json.dumps(receipt), encoding="utf-8")

    def test_complete_platform_set_identity_and_channel(self):
        for channel in ("preview", "stable"):
            with tempfile.TemporaryDirectory() as temporary:
                directory = Path(temporary)
                self.fixture(directory, channel)
                self.assertEqual(len(release.verify(directory, COMMIT, "0.1.0", channel)), 4)
                with self.assertRaisesRegex(ValueError, "identity/channel/lock"):
                    release.verify(directory, "c" * 40, "0.1.0", channel)
                with self.assertRaisesRegex(ValueError, "identity/channel/lock"):
                    release.verify(directory, COMMIT, "0.1.0", "stable" if channel == "preview" else "preview")
                (directory / "linux-x64/manifest.json").unlink()
                with self.assertRaisesRegex(ValueError, "four platform"):
                    release.verify(directory, COMMIT, "0.1.0", channel)

    def test_tampered_archive_fails_closed(self):
        with tempfile.TemporaryDirectory() as temporary:
            directory = Path(temporary)
            self.fixture(directory)
            archive = directory / "linux-x64/airk-0.1.0-linux-x64.vsix"
            archive.write_bytes(archive.read_bytes() + b"tampered")
            with self.assertRaisesRegex(ValueError, "digest mismatch"):
                release.verify(directory, COMMIT, "0.1.0", "preview")

    def test_native_schema_target_and_toolchain_must_match(self):
        for field, changed in (("schema", "unknown/1"), ("target", "aarch64-apple-darwin"),
                               ("toolchain_file_sha256", "0" * 64)):
            with self.subTest(field=field), tempfile.TemporaryDirectory() as temporary:
                directory = Path(temporary)
                self.fixture(directory)
                receipt_path = directory / "linux-x64/manifest.json"
                receipt = json.loads(receipt_path.read_text(encoding="utf-8"))
                archive = receipt_path.parent / receipt["archive"]
                with zipfile.ZipFile(archive) as zipped:
                    entries = [(entry, zipped.read(entry)) for entry in zipped.infolist()]
                native_name = "extension/native/manifest.json"
                native = json.loads(next(data for entry, data in entries if entry.filename == native_name))
                native[field] = changed
                replacement = json.dumps(native).encode()
                with zipfile.ZipFile(archive, "w") as zipped:
                    for entry, data in entries:
                        zipped.writestr(entry, replacement if entry.filename == native_name else data)
                receipt["native_manifest_sha256"] = digest(replacement)
                receipt["payload"]["native/manifest.json"] = digest(replacement)
                receipt["archive_sha256"] = digest(archive.read_bytes())
                receipt_path.write_text(json.dumps(receipt), encoding="utf-8")
                with self.assertRaisesRegex(ValueError, "provenance differs"):
                    release.verify(directory, COMMIT, "0.1.0", "preview")

    def test_duplicate_platform_fails_closed(self):
        with tempfile.TemporaryDirectory() as temporary:
            directory = Path(temporary)
            self.fixture(directory)
            receipt = directory / "win32-x64/manifest.json"
            data = json.loads(receipt.read_text(encoding="utf-8"))
            data["platform"] = "linux-x64"
            receipt.write_text(json.dumps(data), encoding="utf-8")
            with self.assertRaisesRegex(ValueError, "duplicate release platform"):
                release.verify(directory, COMMIT, "0.1.0", "preview")


if __name__ == "__main__":
    unittest.main()
