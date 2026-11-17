# SPDX-License-Identifier: Apache-2.0
"""Cheap relocation controls: no model imports, writes, processes or private data reads."""
import importlib.util
import inspect
from pathlib import Path
import sys
import unittest
from unittest.mock import patch

ROOT = Path(__file__).resolve().parents[2]
BASE = ROOT / 'scripts/arbiter/export/qwen'

class QwenExportPaths(unittest.TestCase):
    def load(self, folder, name):
        path = BASE / folder / (name + '.py')
        with patch.object(sys, 'path', [str(path.parent), *sys.path]):
            spec = importlib.util.spec_from_file_location(name, path)
            module = importlib.util.module_from_spec(spec)
            spec.loader.exec_module(module)
            return module

    def test_focus_publisher_assets_stay_private_and_repo_relative(self):
        module = self.load('qwen3-reranker06-experiment-r1', 'run_focus_export')
        self.assertEqual(module.ROOT, ROOT)
        self.assertEqual(module.ASSETS_ROOT, ROOT / 'artifacts/advanced-split/models/qwen3-reranker06-assets-r1')

    def test_static_requires_explicit_source_and_cases(self):
        module = self.load('qwen3-reranker06-static-export-r1', 'run_static_export')
        for name in ('source', 'case_path'):
            self.assertIs(inspect.signature(module.run).parameters[name].default, inspect.Parameter.empty)
        with self.assertRaisesRegex(ValueError, 'Fresh local private'):
            module.run(BASE / 'forbidden-output', source=ROOT/'artifacts/input', case_path=ROOT/'artifacts/cases.json')

    def test_static_rejects_tracked_inputs_before_reading(self):
        module = self.load('qwen3-reranker06-static-export-r1', 'run_static_export')
        with patch.object(Path, 'read_bytes', side_effect=AssertionError('input read before admission')):
            with self.assertRaisesRegex(ValueError, 'Private explicit'):
                module.run(ROOT/'artifacts/nonexistent-qwen-test-output', source=BASE, case_path=BASE/'cases.json')

    def test_weight8_requires_private_explicit_input_and_output(self):
        module = self.load('qwen3-reranker06-weight8-r1', 'run_weight8')
        self.assertIs(inspect.signature(module.run).parameters['source'].default, inspect.Parameter.empty)
        with patch.object(Path, 'read_bytes', side_effect=AssertionError('input read before admission')):
            with self.assertRaisesRegex(ValueError, 'Private explicit'):
                module.run(ROOT/'artifacts/nonexistent-qwen-test-output', source=BASE)
            with self.assertRaisesRegex(ValueError, 'Fresh private'):
                module.run(BASE/'forbidden-output', source=ROOT/'artifacts/input')

if __name__ == '__main__':
    unittest.main(verbosity=2)
