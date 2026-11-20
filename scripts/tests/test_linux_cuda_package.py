# SPDX-License-Identifier: Apache-2.0
"""GNU file and linker verification controls for the CUDA product packaging consumer."""
import hashlib
import importlib.util
import os
from pathlib import Path
import sys
import tempfile
import time
import unittest

SPEC = importlib.util.spec_from_file_location("airk_linux_package", Path(__file__).parents[1] / "cuda-linux-package.py")
package = importlib.util.module_from_spec(SPEC)
SPEC.loader.exec_module(package)


@unittest.skipUnless(sys.platform == "linux", "requires genuine Linux file and ELF metadata")
class NativeFileControls(unittest.TestCase):
    def test_authenticates_exact_regular_bytes_and_rejects_digest_and_size_changes(self):
        with tempfile.TemporaryDirectory() as raw:
            root = Path(raw).resolve()
            path = root / "held.elf"
            data = b"standalone Linux package file test"
            path.write_bytes(data)
            expected = hashlib.sha256(data).hexdigest()
            deadline = time.monotonic() + 10
            self.assertEqual(package.read_file(path, expected, len(data), deadline, expected_bytes=len(data)), len(data))
            with self.assertRaisesRegex(ValueError, "size differs"):
                package.read_file(path, expected, len(data), deadline, expected_bytes=len(data) - 1)
            with self.assertRaisesRegex(ValueError, "changed"):
                package.read_file(path, "1" * 64, len(data), deadline)
            alias = root / "alias"
            alias.symlink_to(path)
            with self.assertRaisesRegex(ValueError, "noncanonical"):
                package.read_file(alias, expected, len(data), deadline)
            os.link(path, root / "hardlink")
            with self.assertRaisesRegex(ValueError, "ownership, type or size"):
                package.read_file(path, expected, len(data), deadline)

    def test_actual_linux_image_without_required_runtime_exports_is_refused(self):
        with self.assertRaisesRegex(ValueError, "four actual same-image"):
            package.verify_linked_image(Path("/usr/bin/true").resolve(strict=True))

    def test_projection_cannot_name_a_file_outside_runtime(self):
        for value in ("../lib.so", "/usr/lib/lib.so", "name/../lib.so", r"dir\lib.so", "./file"):
            with self.subTest(value=value), self.assertRaises(ValueError):
                package.relative(value)
