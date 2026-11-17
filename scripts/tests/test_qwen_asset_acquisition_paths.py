# SPDX-License-Identifier: Apache-2.0
"""No-network/output admission tests for both maintained acquisition entry points."""
import importlib.util
import hashlib
from pathlib import Path
import sys
import unittest
from unittest.mock import patch

ROOT=Path(__file__).resolve().parents[2]
BASE=ROOT/'scripts/arbiter/export/qwen_assets'
FOLDERS=('qwen3-challenger-assets-r1','qwen3-reranker06-assets-r1')

def load(folder):
    spec=importlib.util.spec_from_file_location('acquisition_'+folder,BASE/folder/'acquire_candidates.py')
    m=importlib.util.module_from_spec(spec);spec.loader.exec_module(m);return m

class AcquisitionPaths(unittest.TestCase):
    def test_both_imports_are_inert(self):
        with patch.object(Path,'read_bytes',side_effect=AssertionError('read at import')), patch.object(Path,'mkdir',side_effect=AssertionError('write at import')):
            for folder in FOLDERS:
                with self.subTest(folder=folder):self.assertTrue(callable(load(folder).main))

    def test_both_refuse_tracked_output_roots_before_inputs(self):
        for folder in FOLDERS:
            m=load(folder)
            with self.subTest(folder=folder), patch.object(sys,'argv',['acquire','--manifest','absent','--manifest-sha256','0'*64,'--output-root',str(ROOT/'scripts')]), patch.object(Path,'read_bytes',side_effect=AssertionError('read before output admission')):
                with self.assertRaisesRegex(ValueError,'private output root'):m.main()

    def test_both_refuse_wrong_manifest_before_creating_outputs(self):
        for folder in FOLDERS:
            m=load(folder)
            with self.subTest(folder=folder), patch.object(sys,'argv',['acquire','--manifest','unused','--manifest-sha256','0'*64,'--output-root',str(ROOT/'artifacts')]), patch.object(Path,'read_bytes',return_value=b'{}'), patch.object(Path,'mkdir',side_effect=AssertionError('write before manifest admission')):
                with self.assertRaisesRegex(ValueError,'manifest digest'):m.main()

    def test_verified_manifest_bytes_are_not_reopened_before_output(self):
        for folder in FOLDERS:
            m=load(folder)
            with self.subTest(folder=folder), patch.object(sys,'argv',['acquire','--manifest',str(ROOT/'Cargo.toml'),'--manifest-sha256',hashlib.sha256(b'{}').hexdigest(),'--output-root',str(ROOT/'artifacts')]), patch.object(Path,'read_bytes',return_value=b'{}') as reads, patch.object(Path,'mkdir',side_effect=RuntimeError('stop before first write')):
                with self.assertRaisesRegex(RuntimeError,'stop before first write'):m.main()
                self.assertEqual(reads.call_count,1)

if __name__=='__main__':unittest.main(verbosity=2)
