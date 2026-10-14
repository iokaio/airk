# SPDX-License-Identifier: Apache-2.0
import hashlib
import importlib.util
from pathlib import Path
import tempfile
import unittest
import zipfile

spec = importlib.util.spec_from_file_location("package_extension", Path(__file__).parents[1] / "package-extension.py")
package = importlib.util.module_from_spec(spec)
spec.loader.exec_module(package)


class VsixContracts(unittest.TestCase):
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
        for name in ("../private", "/private", "native/../../private", "C:/private", "native\\private"):
            with self.assertRaises(ValueError):
                package.safe_name(name)


if __name__ == "__main__":
    unittest.main()
