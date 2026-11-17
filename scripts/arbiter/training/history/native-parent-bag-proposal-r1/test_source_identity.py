# SPDX-License-Identifier: Apache-2.0
"""Actual Windows suspended-image boundary with synthetic Python only."""
from pathlib import Path
import hashlib
import importlib.util
import json
import os
import tempfile
import types
import unittest
HERE=Path(__file__).resolve().parent;ROOT=HERE.parents[3]
def digest(path):return hashlib.sha256(path.read_bytes()).hexdigest()
spec=importlib.util.spec_from_file_location('source_owner',HERE/'run_source.py');source=importlib.util.module_from_spec(spec);spec.loader.exec_module(source)
owner_path=ROOT/'tools/retrieval-train/training_execution.py';raw=owner_path.read_bytes()
if hashlib.sha256(raw).hexdigest()!='d363913b2424321822f9eee189f999eabb807a43c7d0c46fab567531dd1bd975':raise ValueError('qualified owner changed')
owner=types.ModuleType('captured_owner');owner.__file__=str(owner_path);exec(compile(raw,str(owner_path),'exec'),owner.__dict__)
PYTHON=Path('C:/Users/tdotn/AppData/Local/Programs/Python/Python313/python.exe')
@unittest.skipUnless(os.name=='nt','actual Windows ownership boundary')
class IdentityTests(unittest.TestCase):
    def run_owned(self,wrong=False):
        temp=tempfile.TemporaryDirectory(dir=HERE,prefix='identity-test-');self.addCleanup(temp.cleanup);work=Path(temp.name);marker=work/'marker';(work/'stdin').write_bytes(b'')
        expected={'path':str(PYTHON),'sha256':'0'*64 if wrong else digest(PYTHON)}
        if not wrong:self.assertEqual(expected['sha256'],'d932e5e2f324d57f392e8fd063dcf6d0185be8a664c57c6d24e7762ed02c28ca')
        argv=[str(PYTHON),'-I','-S','-B','-c','from pathlib import Path;Path('+repr(str(marker))+').write_text("owned")']
        with (work/'stdout').open('wb') as log,(work/'stdin').open('rb') as stdin:
            result=owner.run_windows_owned_training(argv,ROOT,log,5,dict(os.environ),stdin,268435456,before_resume=lambda api,job,info,deadline:source.suspended_identity(owner,api,job,info,expected,deadline))
        return result,marker
    def test_actual_base_image_proved_while_suspended_then_natural_close(self):
        result,marker=self.run_owned();self.assertTrue(marker.exists());self.assertEqual(result['exit_code'],0);self.assertTrue(result['owned_descendants_settled']);self.assertFalse(result['owned_job_forced_termination']);self.assertEqual(result['owned_total_processes'],1);self.assertEqual(result['suspended_assignment']['active'],1);self.assertEqual(result['suspended_assignment']['pinned_base_image']['sha256'],'d932e5e2f324d57f392e8fd063dcf6d0185be8a664c57c6d24e7762ed02c28ca')
    def test_wrong_pinned_image_refuses_before_first_synthetic_write_and_observes_exit(self):
        before=set(HERE.glob('identity-test-*/marker'))
        with self.assertRaises(ValueError) as error:self.run_owned(wrong=True)
        self.assertEqual(set(HERE.glob('identity-test-*/marker')),before);self.assertTrue(error.exception.owned_cleanup['settled']);self.assertEqual(error.exception.owned_cleanup['final_active'],0);self.assertEqual(error.exception.owned_cleanup['final_member_pids'],[])
if __name__=='__main__':unittest.main()
