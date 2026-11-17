# SPDX-License-Identifier: Apache-2.0
import copy,importlib.util,json,unittest
from pathlib import Path
HERE=Path(__file__).resolve().parent
p=HERE/'continue_cuda_runtime.py';s=importlib.util.spec_from_file_location('continuation_diag',p);m=importlib.util.module_from_spec(s);s.loader.exec_module(m)
class Owner:
 def __init__(self):
  self.bundle=json.loads((HERE/'diagnostic-evidence.json').read_bytes());self.rows={r['path']:json.loads(Path(r['path']).read_bytes()) for r in self.bundle['syntax_runs']+[self.bundle['tree_witness']]}
 def plain(self,p,regular=False):return Path(p)
 def read_json(self,p):return self.rows[str(p)]
 def verify_file(self,p,row):pass
 def successful(self,c):return c.get('exit_code')==0 and c.get('owned_descendants_settled') is True and c.get('owned_job_forced_termination') is False and not c.get('timed_out')
 def result(self):return m.verify_diagnostics(self.bundle,self,self.bundle['source_after_sha256'],self.bundle['request_sha256'])
class DiagnosticControls(unittest.TestCase):
 def test_exact_retained_facts_stay_nonqualified(self):
  o=Owner();v=o.result();self.assertFalse(v['syntax_qualified']);self.assertEqual(len(v['runs']),3);self.assertTrue(all(r['syntax_qualified'] is False for r in v['runs']));self.assertIn('CL exit0',v['runs'][0]['kind'].replace('compiler-exit0','CL exit0'));self.assertIn('no CL exit inferred',v['runs'][2]['kind'])
 def test_relabelled_syntax_success_refuses(self):
  o=Owner();o.rows[o.bundle['syntax_runs'][0]['path']]['status']='PASS_SYNTAX_ONLY'
  with self.assertRaises(ValueError):o.result()
 def test_nonzero_compiler_diagnostic_or_missing_timeout_refuses(self):
  for index,key,value in [(0,'exit_code',1),(2,'timed_out',False),(0,'owned_descendants_settled',False)]:
   o=Owner();o.rows[o.bundle['syntax_runs'][index]['path']]['command'][key]=value
   with self.assertRaises(ValueError):o.result()
 def test_failed_tree_witness_remains_refusal(self):
  o=Owner();o.rows[o.bundle['tree_witness']['path']]['cases'][1]['owner']['owned_job_forced_termination']=True
  with self.assertRaises(ValueError):o.result()
 def test_missing_run_or_changed_source_cannot_be_covered_by_other_diagnostics(self):
  o=Owner();o.bundle['syntax_runs'].pop()
  with self.assertRaises(ValueError):o.result()
  o=Owner();o.rows[o.bundle['syntax_runs'][0]['path']]['source_sha256']='0'*64
  with self.assertRaises(ValueError):o.result()
if __name__=='__main__':unittest.main()
