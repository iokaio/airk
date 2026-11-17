# SPDX-License-Identifier: Apache-2.0
"""Only compiler-proof bookkeeping; no native process/model/source corpus reads."""
import importlib.util
import json
from pathlib import Path
import tempfile
import unittest

path=Path(__file__).with_name('run_reviewed_r2.py')
spec=importlib.util.spec_from_file_location('_ann_r2_test',path)
runner=importlib.util.module_from_spec(spec);spec.loader.exec_module(runner)

class CompilerOwnership(unittest.TestCase):
    def test_separate_crate_depfiles_and_same_command_artifacts_are_required(self):
        with tempfile.TemporaryDirectory() as name:
            root=Path(name);original=runner.RETAINED;runner.RETAINED=root
            dependency={'reason':'compiler-artifact','target':{'name':'airk_retrieval'},'features':['local-inference','test-support'],'filenames':['libairk_retrieval-d4713b2dc80ddac7.rlib','libairk_retrieval-d4713b2dc80ddac7.rmeta']}
            kernel={'reason':'compiler-artifact','target':{'name':'airk_kernel'},'executable':'airk_kernel-57b62dce9e600d19.exe','profile':{'test':True}}
            def log(rows): (root/'ann-probe-compile.log').write_text(''.join(json.dumps(row)+'\n' for row in rows))
            try:
                (root/'airk-kernel.d').write_text('crates/airk-kernel/src/inference_owner/evaluation_tests/ann_self.rs')
                (root/'airk-retrieval-r40.d').write_text('crates/airk-retrieval/src/vector/mutable/self_probe.rs')
                log([dependency,kernel]);runner.verify_compile_owners()
                (root/'airk-retrieval-r40.d').write_text('unrelated.rs')
                with self.assertRaises(ValueError): runner.verify_compile_owners()
                (root/'airk-retrieval-r40.d').write_text('crates/airk-retrieval/src/vector/mutable/self_probe.rs')
                log([dict(dependency,features=['local-inference']),kernel])
                with self.assertRaises(ValueError): runner.verify_compile_owners()
                log([dependency,dependency,kernel])
                with self.assertRaises(ValueError): runner.verify_compile_owners()
                log([dependency,dict(kernel,executable='foreign.exe')])
                with self.assertRaises(ValueError): runner.verify_compile_owners()
            finally: runner.RETAINED=original

if __name__=='__main__': unittest.main()
