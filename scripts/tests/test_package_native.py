# SPDX-License-Identifier: Apache-2.0
import importlib.util
import json
import os
from pathlib import Path
import tempfile
import unittest
import zipfile
from unittest.mock import patch

spec = importlib.util.spec_from_file_location("package_native", Path(__file__).parents[1] / "package-native.py")
package = importlib.util.module_from_spec(spec)
spec.loader.exec_module(package)


class ArchiveContracts(unittest.TestCase):
    def test_payload_budget_precedes_hashing_and_hard_links_are_refused(self):
        with tempfile.TemporaryDirectory() as directory:
            root = Path(directory)
            original = root / "airk"
            original.write_bytes(b"oversize fixture")
            with patch.object(package, "MAX_PAYLOAD_BYTES", 8), patch.object(package, "sha256") as hashed:
                with self.assertRaisesRegex(ValueError, "installer budget"):
                    package.inventory(root)
                hashed.assert_not_called()
            alias = root / "alias"
            os.link(original, alias)
            with self.assertRaisesRegex(ValueError, "Aliased"):
                package.inventory(root)
            self.assertEqual(original.read_bytes(), b"oversize fixture")
            alias.unlink()
            with self.assertRaisesRegex(ValueError, "oversized"):
                package.bounded_json(original, 4)

    def test_pair_requires_exact_source_inventory_and_both_executable_permissions(self):
        with tempfile.TemporaryDirectory() as directory:
            root = Path(directory)
            for name in ("airk", "airk-embed"):
                (root / name).write_bytes(name.encode())
            pair = {"schema": 1, "version": "1.4.0", "os": "linux", "arch": "x86_64",
                    "airk_sha256": package.sha256(root / "airk"), "embed_sha256": package.sha256(root / "airk-embed")}
            source = {"sha256": "1" * 64, "files": 4}
            for name in ("LICENSE", "NOTICE", "THIRD_PARTY_NOTICES.md", "notices/bom.cdx.json", "notices/all-MiniLM-L6-v2/LICENSE", "notices/all-MiniLM-L6-v2/provenance.json"):
                destination = root / name
                destination.parent.mkdir(parents=True, exist_ok=True)
                destination.write_bytes(b"fixture legal/metadata")
            (root / "notices/inventory.json").write_text("[]", encoding="utf-8")
            (root / "airk-runtime.json").write_text(json.dumps(pair), encoding="utf-8")
            (root / "airk-build.json").write_text(json.dumps({"schema": 1, "source": source, "runtime": pair}), encoding="utf-8")
            install = {key: pair[key] for key in ("schema", "version", "os", "arch")}
            install["files"] = package.inventory(root)
            (root / "airk-install.json").write_text(json.dumps(install), encoding="utf-8")
            files, executables = package.validate_runtime(root, "1.4.0", "x86_64-unknown-linux-gnu", source)
            archive = root.parent / (root.name + ".zip")
            try:
                package.write_archive(root, archive, files, executables)
                with zipfile.ZipFile(archive) as zipped:
                    for name in executables:
                        self.assertEqual((zipped.getinfo(name).external_attr >> 16) & 0o777, 0o755)
                with self.assertRaisesRegex(ValueError, "source/pair"):
                    package.validate_runtime(root, "1.4.0", "x86_64-unknown-linux-gnu", {"sha256": "2" * 64})
                (root / "airk-embed").write_bytes(b"modified")
                with self.assertRaisesRegex(ValueError, "inventory differs"):
                    package.validate_runtime(root, "1.4.0", "x86_64-unknown-linux-gnu", source)
            finally:
                archive.unlink(missing_ok=True)

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
