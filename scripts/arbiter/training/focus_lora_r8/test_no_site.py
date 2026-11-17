# SPDX-License-Identifier: Apache-2.0
"""Real inert bootstrap trap, no Torch/model imports."""
import json,os,subprocess,sys,tempfile,unittest
from pathlib import Path
from launch_owned import child_command


class NoSiteControls(unittest.TestCase):
    def test_actual_child_command_has_no_site_and_original_base_worker(self):
        p={'base_python':{'path':'C:/exact/python.exe'},'worker':{'path':'D:/exact/worker.py'}}
        self.assertEqual(child_command(p,'D:/exact/plan.json','literalSHA'),['C:/exact/python.exe','-B','-S','D:/exact/worker.py','--plan','D:/exact/plan.json','--plan-sha256','literalSHA'])
    def test_no_site_executes_neither_owned_pth_nor_sitecustomize(self):
        parent=Path(os.environ['AIRK_RESEARCH_TEST_OUTPUT']).resolve(strict=True)
        with tempfile.TemporaryDirectory(prefix='F1-no-site-control-',dir=parent)as name:
            base=Path(name);trap=base/'trap';trap.mkdir();usersite=base/'userbase'/f'Python{sys.version_info.major}{sys.version_info.minor}'/'site-packages';usersite.mkdir(parents=True)
            pthmarker=base/'pth-executed';custommarker=base/'sitecustomize-executed'
            (usersite/'owned-trap.pth').write_text('import pathlib; pathlib.Path('+repr(str(pthmarker))+').write_text("owned trap")\n',encoding='utf-8')
            (trap/'sitecustomize.py').write_text('from pathlib import Path\nPath('+repr(str(custommarker))+').write_text("owned trap")\n',encoding='utf-8')
            env=dict(os.environ);env.update({'PYTHONPATH':str(trap),'PYTHONUSERBASE':str(base/'userbase'),'PYTHONDONTWRITEBYTECODE':'1','PYTHONPYCACHEPREFIX':str(base/'absent-pycache')})
            code='import sys,json; assert sys.flags.no_site==1; assert "sitecustomize"not in sys.modules; print(json.dumps({"no_site":sys.flags.no_site}))'
            result=subprocess.run([sys.executable,'-B','-S','-c',code],env=env,capture_output=True,text=True,timeout=10)
            self.assertEqual(result.returncode,0,result.stderr);self.assertFalse(pthmarker.exists());self.assertFalse(custommarker.exists());self.assertFalse((base/'absent-pycache').exists())
            # The exact owned .pth trap is live; only explicit test execution runs it.
            result=subprocess.run([sys.executable,'-B','-S','-c','import site; site.addsitedir('+repr(str(usersite))+')'],env=env,capture_output=True,text=True,timeout=10)
            self.assertEqual(result.returncode,0,result.stderr);self.assertTrue(pthmarker.exists());self.assertFalse(custommarker.exists())

if __name__=='__main__':unittest.main(verbosity=2)
