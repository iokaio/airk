# SPDX-License-Identifier: Apache-2.0
"""Pure path/source admission controls; no ONNX or model imports or output files."""
import importlib.util
from pathlib import Path
import sys
import unittest
from unittest.mock import patch

ROOT = Path(__file__).resolve().parents[2]
BASE = ROOT/'scripts/arbiter/export/onnx_storage'

class StoragePaths(unittest.TestCase):
    def load(self, path):
        with patch.object(sys, 'path', [str(path.parent), *sys.path]):
            spec = importlib.util.spec_from_file_location(path.stem, path)
            module = importlib.util.module_from_spec(spec)
            spec.loader.exec_module(module)
            return module

    def test_focus_requires_explicit_private_source(self):
        m=self.load(BASE/'normalize_focus_artifact.py')
        with patch.object(sys,'argv',['worker','--source',str(BASE),'--output',str(ROOT/'artifacts/nonexistent-storage-output')]):
            with self.assertRaisesRegex(ValueError,'Explicit private source'):m.main()

    def test_focus_refuses_tracked_output_before_input_reads(self):
        m=self.load(BASE/'normalize_focus_artifact.py')
        with patch.object(sys,'argv',['worker','--source',str(ROOT/'artifacts/input'),'--output',str(BASE/'forbidden')]), patch.object(Path,'read_bytes',side_effect=AssertionError('read before admission')):
            with self.assertRaisesRegex(ValueError,'Fresh private output'):m.main()

    def test_context_binds_only_maintained_source(self):
        m=self.load(ROOT/'scripts/arbiter/export/qwen/qwen3-challengers-r1/context-weight8-r3/quantize_context.py')
        expected=BASE/'storage_normalizer.py'
        pin={'path':str(expected),'sha256':'7d6b024315dc68f6286cc1563ea5fbe5e69efb727de70d1c820b89127b8e77f4'}
        path,raw=m.verified_normalizer(pin)
        self.assertEqual(path,expected)
        self.assertEqual(raw,expected.read_bytes())
        with patch.object(Path,'read_bytes',side_effect=AssertionError('read before admission')):
            with self.assertRaisesRegex(ValueError,'maintained storage'):
                m.verified_normalizer(dict(pin,path=str(ROOT/'artifacts/old-normalizer.py')))
            with self.assertRaisesRegex(ValueError,'maintained storage'):
                m.verified_normalizer(dict(pin,sha256='0'*64))

    def test_context_rejects_same_path_changed_bytes(self):
        m=self.load(ROOT/'scripts/arbiter/export/qwen/qwen3-challengers-r1/context-weight8-r3/quantize_context.py')
        pin={'path':str(BASE/'storage_normalizer.py'),'sha256':'7d6b024315dc68f6286cc1563ea5fbe5e69efb727de70d1c820b89127b8e77f4'}
        with patch.object(Path,'read_bytes',return_value=b'changed'):
            with self.assertRaisesRegex(ValueError,'source differs'):m.verified_normalizer(pin)

if __name__=='__main__':unittest.main(verbosity=2)
