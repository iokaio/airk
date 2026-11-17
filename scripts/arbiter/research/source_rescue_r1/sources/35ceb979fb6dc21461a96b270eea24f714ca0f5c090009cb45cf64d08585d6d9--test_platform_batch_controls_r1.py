# SPDX-License-Identifier: Apache-2.0
"""Inert injected controls for cleanup decisions; never native execution evidence."""
from pathlib import Path
import contextlib,hashlib,importlib.util,json,os,sys,types,unittest
from unittest.mock import patch
HERE=Path(__file__).resolve().parent
HELPER=HERE/'neo-integrated-platform-batch-r3.py'
class Controls(unittest.TestCase):
 def exercise(self,name,mode,copy_error=False):
  home=HERE/'controls-r1/fixtures'/name;home.mkdir(parents=True);source=home/'source';source.mkdir();(source/'scripts/dev').mkdir(parents=True)
  for rel in ('scripts/provision-static-onnx-runtime.py','scripts/dev/receipt_runner.py'):(source/rel).write_text('inert source fixture; never executed')
  target=source/'target';out=home/'output';calls=[];soft=[256,-1]
  resource=types.SimpleNamespace(RLIMIT_NOFILE=7,RLIM_INFINITY=-1,getrlimit=lambda key:tuple(soft),setrlimit=lambda key,value:soft.__setitem__(slice(None),value))
  spec=importlib.util.spec_from_file_location('inert_'+name,HELPER);m=importlib.util.module_from_spec(spec)
  with patch.dict(sys.modules,{'resource':resource}):spec.loader.exec_module(m)
  m.C={'source':str(source),'base':str(home),'out':str(out),'target':str(target),'python':sys.executable,'node':str(home/'node'),'commands':[{'name':'one','argv':['INERT-NO-PROCESS','one']},{'name':'two','argv':['INERT-NO-PROCESS','two']}],'cleanup':['INERT-NO-PROCESS-CLEAN'],'raw_c7':{'sha256':'inert-control','files':0}}
  original_fresh=m.fresh
  def fresh(q,value):original_fresh(q,{**value,'inert_unit_control':True,'scope':'INERT decision test with injected ownership and observation; no Cargo/child/native/Mac execution.'})
  m.fresh=fresh;m.actor=lambda:{'scope':'injected unit read only; actual writer remains verified Windows human'};m.preflight=lambda:(source,home);m.git=lambda *args:b''
  def result(code=0,signaled=False):return {'exit_code':code,'timed_out':False,'log_oversized':False,'owned_descendants_settled':True,'owned_group_signaled':signaled}
  def owned(argv,cwd,log,timeout,env):
   calls.append(argv);Path(log).write_text('INERT callback, no process launched')
   if argv==m.C['cleanup']:return result()
   if not target.exists():
    (target/'cargo-timings').mkdir(parents=True);(target/'cargo-timings/inert.html').write_text('INERT fixture, not Cargo timing evidence')
   if mode=='unknown':raise OSError('injected missing owner witness')
   return result(17 if mode=='failure' else 0,mode=='signaled')
  owner=types.SimpleNamespace(run_owned=owned,successful=lambda r:r['exit_code']==0 and not r['timed_out'] and not r['log_oversized'] and r['owned_descendants_settled'])
  m.load=lambda name,path:owner if name=='actual_darwin_owner' else types.SimpleNamespace(source_identity=lambda:m.C['raw_c7'])
  real_lstat=Path.lstat
  def lstat(q,*args,**kwargs):
   original=real_lstat(q,*args,**kwargs)
   if q==target:
    values=list(original);values[4]=501;return os.stat_result(values) # Injected Darwin metadata only; changes no ACL/account.
   return original
  with contextlib.ExitStack() as stack:
   stack.enter_context(patch.object(m.subprocess,'check_output',return_value=b'inert original dirty status'))
   stack.enter_context(patch.object(Path,'lstat',lstat))
   if copy_error:stack.enter_context(patch.object(m.shutil,'copytree',side_effect=OSError('injected timing retention failure')))
   try:m.main()
   except SystemExit as error:self.assertEqual(error.code,1)
  final=json.loads((out/'batch-result.json').read_text());self.assertTrue(final['inert_unit_control'])
  return final,calls,m.C['cleanup']
 def test_unknown_owner_witness_withholds_cleanup_and_preserves_primary(self):
  final,calls,clean=self.exercise('unknown','unknown');self.assertFalse(final['passed']);self.assertNotIn(clean,calls);self.assertEqual(final['error']['message'],'injected missing owner witness');self.assertIsNotNone(final['cleanup_blocker']);self.assertIsNone(final['cleanup'])
 def test_copy_error_does_not_erase_failed_command_or_skip_settled_cleanup(self):
  final,calls,clean=self.exercise('failed-and-copy','failure',True);self.assertFalse(final['passed']);self.assertEqual(final['phases'][0]['result']['exit_code'],17);self.assertIn('failed one',final['error']['message']);self.assertEqual(final['evidence_errors'][0]['stage'],'timing_retention');self.assertEqual(calls.count(clean),1);self.assertEqual(final['cleanup']['exit_code'],0)
 def test_copy_error_on_success_retains_nonpass_and_still_cleans_settled_target(self):
  final,calls,clean=self.exercise('success-and-copy','success',True);self.assertFalse(final['passed']);self.assertIsNone(final['error']);self.assertEqual(final['evidence_errors'][0]['stage'],'timing_retention');self.assertEqual(calls.count(clean),1)
 def test_unix_group_signal_never_qualifies_even_with_settlement(self):
  final,calls,clean=self.exercise('signaled','signaled');self.assertFalse(final['passed']);self.assertTrue(final['phases'][0]['result']['owned_group_signaled']);self.assertFalse(final['phases'][0]['natural_unix_settlement']);self.assertEqual(len(final['phases']),1);self.assertEqual(calls.count(clean),1)
if __name__=='__main__':unittest.main(verbosity=2)
