# SPDX-License-Identifier: Apache-2.0
"""Run inside verified x86-64 Ubuntu with python3 -B -S -m unittest."""
import hashlib
import json
from pathlib import Path
import unittest
from unittest.mock import patch

import inventory
import redistributions
import unpack


class Preparation(unittest.TestCase):
    def test_observes_needed_and_search_paths_without_loading(self):
        raw = " 0x1 (NEEDED) Shared library: [libx.so.1]\n 0x1d (RUNPATH) Library runpath: [$ORIGIN]"
        self.assertEqual(inventory.dynamic_tags(raw), {"NEEDED": ["libx.so.1"], "RUNPATH": ["$ORIGIN"]})

    def test_inspects_system_elf(self):
        row = inventory.source_record(Path("/usr/lib/x86_64-linux-gnu/libc.so.6"))
        self.assertEqual(row["dynamic_tags"]["SONAME"], ["libc.so.6"])
        self.assertIn("ld-linux-x86-64.so.2", row["dynamic_tags"]["NEEDED"])

    def test_archive_paths_are_contained_and_notices_separated(self):
        root = Path("/example")
        for name in ("/home/escape", "expected/../../escape", "different/bin/nvcc"):
            with self.assertRaises(ValueError):
                unpack.member_destination(root, "cuda_nvcc", "expected", name)
        self.assertEqual(unpack.member_destination(root, "cudnn", "p", "p/lib/libcudnn.so"),
                         root / "cudnn/lib/libcudnn.so")
        self.assertEqual(unpack.member_destination(root, "cuda_nvcc", "p", "p/LICENSE"),
                         root / "notices/cuda_nvcc/LICENSE")

    def test_requires_nonroot_registered_uid(self):
        with self.assertRaises(ValueError):
            inventory.inventory(0, "root")

    def test_rejects_changed_publisher_metadata(self):
        with self.assertRaisesRegex(ValueError, "digest"):
            redistributions.selected("cuda", b"{}")

    def test_selects_only_linux_cuda12_cudnn(self):
        row = {"relative_path": "cudnn/linux-x86_64/cudnn-linux-x86_64-9_cuda12-archive.tar.xz",
               "size": "5", "sha256": "0" * 64}
        body = json.dumps({"release_label": "9", "cudnn": {"linux-x86_64": {"cuda12": row}}}).encode()
        with patch.dict(redistributions.INDEXES, cudnn=("9", hashlib.sha256(body).hexdigest(), ("cudnn",))):
            result = redistributions.selected("cudnn", body)
        self.assertEqual(result[0]["name"], "cudnn-linux-x86_64-9_cuda12-archive.tar.xz")
        self.assertEqual(result[0]["bytes"], 5)


if __name__ == "__main__":
    unittest.main()
