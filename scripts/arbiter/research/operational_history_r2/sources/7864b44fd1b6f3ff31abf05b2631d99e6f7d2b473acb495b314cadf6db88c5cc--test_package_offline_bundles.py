# SPDX-License-Identifier: Apache-2.0
"""Tiny synthetic file tests; no model, Profile, final proof or qualification."""
import hashlib
import json
from pathlib import Path
import tempfile
import unittest
from unittest.mock import patch
import zipfile

import package_offline_bundles as package
import test_stage_release_parts as fixtures


class OfflineArchiveTests(unittest.TestCase):
    def inputs(self, root):
        plan, sources, publication, expected = fixtures.ReleasePartTests().inputs(root)
        notice_files = {}
        for number in range(1, 4):
            rows = []
            for name, role, raw in (("LICENSE", "license", b"Complete synthetic license\n"),
                                    ("README.md", "readme", b"Complete publisher card\n")):
                path = root / f"notice-{number}-{name}"
                path.write_bytes(raw)
                rows.append({"name": name, "role": role, "path": str(path),
                             "bytes": len(raw), "sha256": hashlib.sha256(raw).hexdigest()})
            notice_files[f"{number:064x}"] = rows
        notices = root / "notices.private.json"
        self.write(notices, {"schema": "airk.private-offline-notice-mapping/1",
                             "plan_sha256": package.owner.digest(plan), "files": notice_files})
        return plan, sources, publication, notices, expected

    def write(self, path, value):
        path.write_text(json.dumps(value), encoding="utf-8")

    def run_package(self, root, inputs):
        plan, sources, publication, notices, _ = inputs
        return package.package(plan, package.owner.digest(plan), sources, package.owner.digest(sources),
                               publication, package.owner.digest(publication), notices,
                               package.owner.digest(notices), root / "candidate", root)

    def test_three_clean_archives_import_route_checksums_parts_and_unchanged_sources(self):
        with tempfile.TemporaryDirectory() as temporary:
            root = Path(temporary)
            inputs = self.inputs(root)
            before = {p.name: p.read_bytes() for p in root.iterdir()}
            receipt = self.run_package(root, inputs)
            self.assertTrue(receipt["complete"])
            self.assertFalse(receipt["qualified"])
            self.assertFalse(receipt["published"])
            self.assertEqual(receipt["release_parts"], 6)
            self.assertEqual(receipt["logical_bundles"], 3)
            checksums = (root / "candidate/archives/SHA256SUMS").read_text(encoding="ascii")
            for number, row in enumerate(receipt["archives"], 1):
                identity = f"{number:064x}"
                path = root / "candidate/archives" / row["name"]
                self.assertIn(f"{package.owner.digest(path)}  {path.name}\n", checksums)
                self.assertEqual(path.stat().st_nlink, 1)
                with zipfile.ZipFile(path) as archive:
                    self.assertEqual(set(archive.namelist()), {f"models/{identity}/logical.bin", "IMPORT.txt",
                                                              "notices/LICENSE", "notices/README.md"})
                    self.assertEqual(archive.read(f"models/{identity}/logical.bin"), b"ABCDEF" + str(number).encode())
                    self.assertEqual(archive.read("notices/LICENSE"), b"Complete synthetic license\n")
                    instruction = archive.read("IMPORT.txt").decode("utf-8")
                    self.assertIn(f"--profile {identity}", instruction)
                    self.assertIn("--expected-revision N", instruction)
                    self.assertIn(f"models/{identity}", instruction)
                    self.assertNotIn(str(root), instruction)
                    self.assertTrue(all(info.compress_type == zipfile.ZIP_STORED for info in archive.infolist()))
            self.assertNotIn(str(root), (root / "candidate/packaging-receipt.json").read_text(encoding="utf-8"))
            self.assertEqual(before, {root.joinpath(name).name: root.joinpath(name).read_bytes() for name in before})
            self.assertEqual({p.name: p.read_bytes() for p in (root / "candidate/release-parts/parts").iterdir()}, inputs[-1])
            with self.assertRaisesRegex(ValueError, "fresh"):
                self.run_package(root, inputs)

    def test_missing_or_foreign_notice_closure_refuses_before_output(self):
        for fault in ("missing", "foreign", "no_license", "escape", "boolean", "digest", "duplicate"):
            with self.subTest(fault=fault), tempfile.TemporaryDirectory() as temporary:
                root = Path(temporary)
                inputs = self.inputs(root)
                value = json.loads(inputs[3].read_bytes())
                rows = value["files"][f"{1:064x}"]
                if fault == "missing":
                    value["files"].pop(f"{1:064x}")
                elif fault == "foreign":
                    value["plan_sha256"] = "0" * 64
                elif fault == "no_license":
                    rows[0]["role"] = "notice"
                elif fault == "escape":
                    rows[0]["name"] = "../profile.json"
                elif fault == "boolean":
                    rows[0]["bytes"] = True
                elif fault == "digest":
                    rows[0]["sha256"] = "0" * 64
                else:
                    rows[1]["name"] = rows[0]["name"]
                self.write(inputs[3], value)
                with self.assertRaises(ValueError):
                    self.run_package(root, inputs)
                self.assertFalse((root / "candidate").exists())

    def test_notice_hardlink_refused_before_output(self):
        with tempfile.TemporaryDirectory() as temporary:
            root = Path(temporary)
            inputs = self.inputs(root)
            (root / "notice-alias").hardlink_to(root / "notice-1-LICENSE")
            with self.assertRaisesRegex(ValueError, "hardlinked"):
                self.run_package(root, inputs)
            self.assertFalse((root / "candidate").exists())

    def test_archive_readback_refuses_corruption_without_top_completion(self):
        with tempfile.TemporaryDirectory() as temporary:
            root = Path(temporary)
            inputs = self.inputs(root)
            original = package.add_file
            def corrupt(archive, name, source, size, pin, anchor):
                original(archive, name, source, size, pin, anchor)
                if name.endswith("logical.bin"):
                    archive.writestr("private-profile.json", b"unexpected")
            with patch.object(package, "add_file", side_effect=corrupt):
                with self.assertRaisesRegex(ValueError, "census"):
                    self.run_package(root, inputs)
            self.assertFalse((root / "candidate/packaging-receipt.json").exists())
            self.assertFalse((root / "candidate/archives/SHA256SUMS").exists())

    def test_archive_source_drift_and_foreign_target_preserved(self):
        with tempfile.TemporaryDirectory() as temporary:
            root = Path(temporary)
            source = root / "source"
            source.write_bytes(b"current")
            path = root / "candidate.zip"
            files = [{"name": "source", "bytes": 7, "sha256": hashlib.sha256(b"changed").hexdigest()}]
            child = root / "models" / ("1" * 64)
            child.mkdir(parents=True)
            (child / "source").write_bytes(b"current")
            with self.assertRaisesRegex(ValueError, "hash"):
                package.archive_bundle(path, "1" * 64, files, [], root, root)
            self.assertEqual(source.read_bytes(), b"current")
            foreign = root / "foreign.zip"
            foreign.write_bytes(b"preserve")
            with self.assertRaises(FileExistsError):
                package.archive_bundle(foreign, "1" * 64, files, [], root, root)
            self.assertEqual(foreign.read_bytes(), b"preserve")


if __name__ == "__main__":
    unittest.main(verbosity=2)
