# SPDX-License-Identifier: Apache-2.0
"""Pure candidate aggregate and exact compiler/code-pin binding controls."""

import importlib.util
from copy import deepcopy
from pathlib import Path
import unittest

source = Path(__file__).with_name("fresh_architecture_archive.py")
spec = importlib.util.spec_from_file_location("cuda_architecture_archive_controls", source)
archive = importlib.util.module_from_spec(spec)
spec.loader.exec_module(archive)


class Tests(unittest.TestCase):
    def test_adaptation_uses_frozen_compiler_projection_and_coff_owner(self):
        adapted = archive.aggregate_owner()
        self.assertEqual(adapted.RECIPE_SCHEMA, archive.RECIPE_SCHEMA)
        self.assertEqual(adapted.RECEIPT_SCHEMA, archive.AGGREGATE_SCHEMA)
        self.assertEqual(Path(adapted.__file__), archive.SELF)
        self.assertTrue(callable(adapted.select_plan))
        self.assertTrue(callable(adapted.inspect_output))
        compiler = adapted.load(archive.HERE / "fresh_compile.py", "compile")
        self.assertTrue(callable(compiler.verify_receipt))
        projection = adapted.load(archive.HERE / "fresh_projection.py", "project")
        self.assertEqual(projection.BUILD_SCHEMA, archive.BUILD_SCHEMA)

    def test_code_receipt_must_bind_current_compile_graph_and_provider(self):
        arch, arch_pin = archive.frozen("fresh_architectures.py", archive.ARCH_SHA)
        compile_pin = {"path": "fresh-compile.json", "bytes": 5, "sha256": "compile"}
        provider = {"path": "onnxruntime_providers_cuda.dll", "bytes": 6, "sha256": "cuda"}
        inspector = {"path": "cuobjdump.exe", "bytes": 7, "sha256": "inspector"}
        graph = {"requested": "86;89;120", "proof": "exact generated graph"}
        compiled = {"configured_graph": {"cuda_architecture_evidence": graph}, "providers": [provider]}
        recipe = {"cuda_code_inspector": inspector}
        good = {"schema": archive.CODE_SCHEMA, "qualification": False, "compile": compile_pin,
                "architecture_owner": arch_pin, "inspector": inspector, "provider": provider,
                "configured_architectures": graph, "stage_receipts": ["real", "virtual"],
                "cubin_objects": {"86": 1, "89": 1, "120a": 1}, "ptx_objects": {"120": 1}}
        archive.check_codes(good, arch, arch_pin, recipe, compile_pin, compiled)
        for key in ("provider", "configured_architectures", "compile", "architecture_owner"):
            altered = deepcopy(good)
            altered[key] = {"sha256": "another-build"}
            with self.subTest(key=key), self.assertRaisesRegex(ValueError, "exact compiled provider"):
                archive.check_codes(altered, arch, arch_pin, recipe, compile_pin, compiled)


if __name__ == "__main__":
    unittest.main()
