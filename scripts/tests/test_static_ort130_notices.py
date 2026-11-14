# SPDX-License-Identifier: Apache-2.0
import io, tarfile, tempfile, unittest
from pathlib import Path
from unittest.mock import patch
import test_static_onnx_provisioning as baseline

MODULE = baseline.MODULE
WINDOWS = baseline.WINDOWS

class ProtobufTarNoticeTests(unittest.TestCase):
    def fixture(self, root, kind=tarfile.REGTYPE):
        lock, dependencies = baseline.StaticLicenseRetentionTests().fixture(root)
        path = root / "downloads/protobuf.zip"
        with tarfile.open(path, "w:gz") as archive:
            entry = tarfile.TarInfo("protobuf-33.6/LICENSE")
            entry.type = kind
            content = b"Complete protobuf 33.6 license fixture\n"
            if kind == tarfile.REGTYPE:
                entry.size = len(content)
                archive.addfile(entry, io.BytesIO(content))
            else:
                entry.linkname = "../../outside/LICENSE"
                archive.addfile(entry)
        row = next(row for row in dependencies if row["name"] == "protobuf")
        row.update(bytes=path.stat().st_size, sha256=MODULE.digest(path))
        return lock, dependencies

    def test_pinned_tar_source_retains_complete_license_without_extraction(self):
        with tempfile.TemporaryDirectory() as directory:
            root = Path(directory)
            lock, dependencies = self.fixture(root)
            with patch.object(MODULE, "LOCK", lock), patch.object(MODULE, "selected_dependencies", return_value=dependencies):
                rows = MODULE.collect_notices(root, WINDOWS)
            relative = "notices/protobuf/protobuf-33.6/LICENSE"
            self.assertEqual((root / relative).read_bytes(), b"Complete protobuf 33.6 license fixture\n")
            self.assertIn(relative, [row["path"] for row in rows])
            self.assertFalse((root / "protobuf-33.6").exists())

    def test_tar_legal_links_refuse_before_any_notice_is_published(self):
        for kind in [tarfile.SYMTYPE, tarfile.LNKTYPE]:
            with self.subTest(kind=kind), tempfile.TemporaryDirectory() as directory:
                root = Path(directory)
                lock, dependencies = self.fixture(root, kind)
                with patch.object(MODULE, "LOCK", lock), patch.object(MODULE, "selected_dependencies", return_value=dependencies):
                    with self.assertRaises(ValueError): MODULE.collect_notices(root, WINDOWS)
                self.assertFalse((root / "notices").exists())
