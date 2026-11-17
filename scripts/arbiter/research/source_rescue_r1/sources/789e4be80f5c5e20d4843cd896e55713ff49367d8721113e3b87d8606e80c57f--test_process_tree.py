# SPDX-License-Identifier: Apache-2.0
import importlib.util,tempfile,unittest
from pathlib import Path
HERE=Path(__file__).resolve().parent
p=HERE/'process_tree.py';s=importlib.util.spec_from_file_location('tree',p);m=importlib.util.module_from_spec(s);s.loader.exec_module(m)
class Controls(unittest.TestCase):
 def test_exact_argument_list_with_spaces(self):
  r=m.command_request(['C:/Windows/System32/cmd.exe','/d','/c','C:/private/path with space/action.cmd'],'C:/private',Path('C:/private'),'one')
  self.assertEqual(r['arguments'],'/d /c "C:/private/path with space/action.cmd"');self.assertEqual(r['executable'],'C:/Windows/System32/cmd.exe')
 def test_relative_executable_refuses(self):
  with self.assertRaises(ValueError):m.command_request(['cmd.exe'],'C:/private',Path('C:/private'),'one')
 def test_original_environment_and_timeout_forwarded_without_cleanup_override(self):
  class Fake:
   def run_owned(self,argv,cwd,log,timeout,environment):self.args=(argv,cwd,log,timeout,environment);return {'exit_code':1,'owned_job_forced_termination':True}
   def successful(self,r):return False
  with tempfile.TemporaryDirectory(dir=HERE) as t:
   o=Fake();env={'MARKER':'exact'};r=m.run_tree(o,['C:/Windows/System32/cmd.exe','/d','/c','x.cmd'],Path(t),Path(t),'one',12.25,env)
   self.assertEqual(o.args[3],12.25);self.assertIs(o.args[4],env);self.assertEqual(o.args[0][0],str(m.POWERSHELL));self.assertEqual(r['owned_job_forced_termination'],True)
if __name__=='__main__':unittest.main()
