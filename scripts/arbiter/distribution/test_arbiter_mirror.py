# SPDX-License-Identifier: Apache-2.0
"""Model-free tests of the release inventory, naming and readback projection."""
import hashlib
import json
from pathlib import Path
import sys
import tempfile
import unittest

sys.path.insert(0, str(Path(__file__).resolve().parent))
import arbiter_mirror as owner


def pin(path, data):
    return {"path": path, "bytes": len(data), "sha256": hashlib.sha256(data).hexdigest()}


class Naming(unittest.TestCase):
    def test_qualified_names_are_stable_across_hosts(self):
        args = "context-compact", "qwen3-1.7b", "windows-cuda-fp16", 7
        self.assertEqual(owner.public_name(*args, "model.onnx.data.part016"),
                         "airk-arbiter-context-compact-qwen3-1.7b-windows-cuda-fp16-r007.model.onnx.data.part016")
        self.assertEqual(owner.public_name(*args, "native-runtime1024-fixture.json"),
                         "airk-arbiter-context-compact-qwen3-1.7b-windows-cuda-fp16-r007.native-runtime1024-fixture.json")
        for name in ("../model.onnx", "model.onnx.data.part000", "model.onnx.data.part017", "model.onnx.data.part01"):
            with self.subTest(name=name), self.assertRaisesRegex(ValueError, "role or part ordinal"):
                owner.public_name(*args, name)
        for revision in (0, 1000, True):
            with self.subTest(revision=revision), self.assertRaisesRegex(ValueError, "naming input"):
                owner.public_name(*args[:3], revision, "model.onnx")


class Inventory(unittest.TestCase):
    def setUp(self):
        holder = tempfile.TemporaryDirectory()
        self.addCleanup(holder.cleanup)
        root = Path(holder.name).resolve(strict=True)
        if not root.is_relative_to(Path(tempfile.gettempdir()).resolve(strict=True)):
            raise ValueError("temporary release test fixture escaped its checked parent")
        self.directory = root / "stage"
        self.directory.mkdir()
        self.path = root / "inventory.json"
        self.asset = pin("airk-arbiter-focus-qwen3-reranker-0.6b-cpu-int8-r001.model.onnx", b"model")
        self.card = pin("README.md", b"# Card")
        (self.directory / self.asset["path"]).write_bytes(b"model")
        (self.directory / self.card["path"]).write_bytes(b"# Card")
        self.request = {"schema": owner.SCHEMA, "repo": owner.REPO, "files": [self.asset], "card": self.card}

    def inspect(self, value=None):
        raw = json.dumps(value if value is not None else self.request).encode()
        self.path.write_bytes(raw)
        return owner.inventory(self.path, hashlib.sha256(raw).hexdigest(), self.directory)

    def test_checks_exact_bytes_and_separates_hub_card(self):
        assets, card = self.inspect()
        self.assertEqual([owner.remote_only(row) for row in assets], [self.asset])
        self.assertEqual(owner.remote_only(card), self.card)
        self.assertEqual(assets[0]["local_path"], self.directory / self.asset["path"])
        response = {"repo": owner.REPO, "commit": "a" * 40, "files": [self.asset, self.card], "changed": True}
        output = owner.release_report(response, [self.asset], self.card)
        self.assertEqual(output["files"], [self.asset])
        with self.assertRaisesRegex(ValueError, "readback does not cover"):
            owner.release_report(response | {"files": [self.asset]}, [self.asset], self.card)

    def test_changed_bytes_and_extra_files_are_refused(self):
        p = self.directory / self.asset["path"]
        p.write_bytes(b"modes")
        with self.assertRaisesRegex(ValueError, "SHA-256 differs"):
            self.inspect()
        p.write_bytes(b"model")
        (self.directory / "unexpected.txt").write_bytes(b"private")
        with self.assertRaisesRegex(ValueError, "exactly the reviewed"):
            self.inspect()

    def test_wrong_repo_duplicate_and_traversal_are_refused(self):
        cases = ((self.request | {"repo": "someone/else"}, "inventory"),
                 (self.request | {"files": [self.asset, self.asset]}, "duplicate"),
                 (self.request | {"files": [self.asset | {"path": "../escape"}]}, "flat safe"))
        for case, reason in cases:
            with self.subTest(reason=reason), self.assertRaisesRegex(ValueError, reason):
                self.inspect(case)

    def test_altered_or_ambiguous_inventory_is_refused(self):
        raw = b'{"schema":"first","schema":"second"}'
        self.path.write_bytes(raw)
        with self.assertRaisesRegex(ValueError, "inventory SHA-256"):
            owner.inventory(self.path, "0" * 64, self.directory)
        with self.assertRaisesRegex(ValueError, "duplicate inventory field"):
            owner.inventory(self.path, hashlib.sha256(raw).hexdigest(), self.directory)


if __name__ == "__main__":
    unittest.main()
