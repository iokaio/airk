# SPDX-License-Identifier: Apache-2.0
"""No-output controls for maintained independent numeric roots and helper binding."""
import importlib.util
from pathlib import Path
import sys
from types import SimpleNamespace
import unittest
from unittest.mock import patch

ROOT = Path(__file__).resolve().parents[2]
BASE = ROOT / 'scripts/arbiter/evaluation/independent_numeric'

class IndependentNumericPaths(unittest.TestCase):
    def load(self, name):
        with patch.object(sys, 'path', [str(BASE), *sys.path]):
            spec = importlib.util.spec_from_file_location(name, BASE / (name + '.py'))
            module = importlib.util.module_from_spec(spec)
            spec.loader.exec_module(module)
            return module

    def test_helpers_execute_tracked_paths(self):
        helpers = self.load('maintained_helpers')
        self.assertEqual(Path(helpers.worker.__file__), ROOT/'scripts/cuda-native-execution/helpers/reference_worker.py')
        self.assertEqual(Path(helpers.parent.__file__), ROOT/'scripts/cuda-native-execution/helpers/reference_parent_no_site_r2.py')
        self.assertIs(helpers.parent.worker, helpers.worker)

    def test_source_directory_is_never_an_output_root(self):
        helpers = self.load('maintained_helpers')
        with self.assertRaisesRegex(ValueError, 'private artifact'):
            helpers.private_root(BASE)
        with self.assertRaises(FileNotFoundError):
            helpers.private_root(ROOT/'artifacts/nonexistent-numeric-root-for-test')

    def test_inventory_refuses_source_root_before_writing(self):
        prepare = self.load('prepare_reference')
        with patch.object(prepare, 'write_new', side_effect=AssertionError('write before admission')):
            with self.assertRaisesRegex(ValueError, 'private artifact'):
                prepare.inventory(BASE)

    def test_assembly_refuses_source_root_before_reading(self):
        assembly = self.load('assemble_references')
        with patch.object(assembly.worker, 'pinned', side_effect=AssertionError('input read before output admission')):
            with self.assertRaisesRegex(ValueError, 'private artifact'):
                assembly.assemble(SimpleNamespace(output_root=BASE))

if __name__ == '__main__':
    unittest.main(verbosity=2)
