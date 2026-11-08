# SPDX-License-Identifier: Apache-2.0
"""Deep upstream legal paths must fit the unchanged native install contract."""
import copy
import importlib.util
import json
from pathlib import Path
import tempfile
import unittest
from unittest.mock import patch

SPEC = importlib.util.spec_from_file_location("static_notice_builder", Path(__file__).parents[1] / "build-service-runtime.py")
BUILDER = importlib.util.module_from_spec(SPEC)
SPEC.loader.exec_module(BUILDER)


class StaticNoticePackagingTests(unittest.TestCase):
    def test_deep_notices_keep_exact_bytes_and_complete_original_provenance(self):
        with tempfile.TemporaryDirectory() as temporary:
            root = Path(temporary)
            producer, staged = root / "producer", root / "runtime"
            producer.mkdir()
            staged.mkdir()
            lock = json.loads((BUILDER.ROOT / "scripts/static-onnx-runtime-lock.json").read_text())
            paths = [f'notices/{name}/LICENSE' for name in {"onnxruntime", *[row["name"] for row in lock["dependencies"]]}]
            paths += ["notices/onnxruntime/ThirdPartyNotices.txt",
                      "notices/json/json-3.11.3/docs/mkdocs/docs/home/license.md"]
            rows = []
            for name in sorted(paths):
                source = producer / name
                source.parent.mkdir(parents=True, exist_ok=True)
                # Identical legal bytes at different upstream paths must all be
                # retained with distinct, unambiguous source/destination entries.
                source.write_bytes(b"original upstream legal bytes\n")
                rows.append({"path":name,"bytes":source.stat().st_size,"sha256":BUILDER.digest(source)})
            receipt = producer / "build-receipt.json"
            receipt.write_text(json.dumps({"notices":rows}))
            inputs = {"target":"x86_64-pc-windows-msvc","build_receipt_sha256":BUILDER.digest(receipt)}
            with patch.dict(BUILDER.os.environ, {"ORT_LIB_LOCATION":str(producer / "b")}):
                BUILDER.stage_static_notices(staged, inputs)
            metadata = json.loads((staged / "notices/static-onnx-runtime/inventory.json").read_text())
            self.assertEqual(metadata["schema"], 2)
            self.assertEqual(metadata["build_receipt_sha256"], inputs["build_receipt_sha256"])
            self.assertEqual({row["path"] for row in metadata["sources"].values()}, set(paths))
            self.assertEqual(len(metadata["files"]), len(paths))
            for name, digest in metadata["files"].items():
                self.assertEqual(len(name.split("/")), 4)
                self.assertLessEqual(max(map(len, name.split("/"))), 128)
                self.assertEqual((staged / name).read_bytes(), (producer / metadata["sources"][name]["path"]).read_bytes())
                self.assertEqual(BUILDER.digest(staged / name), digest)
            expected = BUILDER.static_notice_inventory(metadata, inputs["target"], metadata["files"])
            self.assertEqual(expected, {"notices/static-onnx-runtime/inventory.json", *metadata["files"]})
            crossed = copy.deepcopy(metadata)
            first = next(iter(crossed["sources"]))
            crossed["sources"][first]["path"] = "notices/json/other/LICENSE"
            with self.assertRaisesRegex(ValueError, "mapping"):
                BUILDER.static_notice_inventory(crossed, inputs["target"], metadata["files"])
            omitted = copy.deepcopy(metadata)
            for name in list(omitted["files"]):
                if omitted["sources"][name]["path"] == "notices/onnxruntime/ThirdPartyNotices.txt":
                    del omitted["files"][name]
                    del omitted["sources"][name]
            with self.assertRaisesRegex(ValueError, "incomplete"):
                BUILDER.static_notice_inventory(omitted, inputs["target"], metadata["files"])

    def test_invalid_source_paths_cannot_be_hidden_by_flattening(self):
        for source in ["notices/json/../LICENSE", "notices/json//LICENSE", "notices/json/C:/LICENSE",
                       "notices/json/dir\\LICENSE", "/notices/json/LICENSE", "notices/json/source.rs"]:
            with self.subTest(source=source), self.assertRaises(ValueError):
                BUILDER.packaged_static_notice_path(source)


if __name__ == "__main__":
    unittest.main()
