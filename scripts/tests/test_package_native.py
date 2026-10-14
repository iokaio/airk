# SPDX-License-Identifier: Apache-2.0
import importlib.util
from pathlib import Path
import tempfile
import unittest
import zipfile

spec = importlib.util.spec_from_file_location("package_native", Path(__file__).parents[1] / "package-native.py")
package = importlib.util.module_from_spec(spec)
spec.loader.exec_module(package)


class ArchiveContracts(unittest.TestCase):
    def test_allowlist_permissions_and_tamper_detection(self):
        with tempfile.TemporaryDirectory() as directory:
            root = Path(directory)
            stage = root / "stage"
            stage.mkdir()
            (stage / "airk").write_bytes(b"fictional executable fixture")
            (stage / "LICENSE").write_text("fixture license", encoding="utf-8")
            (root / "private.env").write_text("excluded fixture", encoding="utf-8")
            expected = package.inventory(stage)
            archive = root / "candidate.zip"
            package.write_archive(stage, archive, expected, "airk")
            with zipfile.ZipFile(archive) as zipped:
                self.assertEqual(set(zipped.namelist()), {"airk", "LICENSE"})
                self.assertEqual((zipped.getinfo("airk").external_attr >> 16) & 0o777, 0o755)
            changed = dict(expected, airk="0" * 64)
            with self.assertRaisesRegex(ValueError, "digest mismatch"):
                package.verify_archive(archive, changed, "airk")
            with zipfile.ZipFile(archive, "a") as zipped:
                zipped.writestr("unexpected.txt", "must fail")
            with self.assertRaisesRegex(ValueError, "entries differ"):
                package.verify_archive(archive, expected, "airk")

    def test_missing_executable_bit_rejected(self):
        with tempfile.TemporaryDirectory() as directory:
            root = Path(directory)
            (root / "airk").write_bytes(b"fixture")
            archive = root / "bad.zip"
            with zipfile.ZipFile(archive, "w") as zipped:
                info = zipfile.ZipInfo("airk")
                info.external_attr = 0o100644 << 16
                zipped.writestr(info, b"fixture")
            with self.assertRaisesRegex(ValueError, "executable permission"):
                package.verify_archive(archive, {"airk": package.sha256(root / "airk")}, "airk")


if __name__ == "__main__":
    unittest.main()
