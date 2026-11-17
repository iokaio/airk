# SPDX-License-Identifier: Apache-2.0
"""Synthetic launcher-boundary controls only, not native restart proof."""
import importlib.machinery,importlib.util,json,sys,tempfile,unittest
from pathlib import Path
HERE=Path(__file__).resolve().parent
sys.path.insert(0,str(HERE.parent/'cal-execution-r102'))
loader=importlib.machinery.SourceFileLoader('private_restart_launcher',str(HERE/'run_fixture.py.proposed'))
spec=importlib.util.spec_from_loader(loader.name,loader);module=importlib.util.module_from_spec(spec);loader.exec_module(module)
class Controls(unittest.TestCase):
    def setUp(self):
        self.temp=tempfile.TemporaryDirectory();self.addCleanup(self.temp.cleanup);self.root=Path(self.temp.name)
        self.old=self.root/'old';self.old.mkdir();self.control=self.old/'control';self.control.mkdir();self.work=self.root/'fresh-metadata'
        self.ready={'schema':'airk.private-arbiter-fixture/1','evaluation':True,'release_qualified':False,'inputs_sha256':'original-input','control_root':str(self.control),'health':{'instance':'old-instance','executable_sha256':'old-image','catalog_clock':{'namespace':'original-catalog'}}}
        self.closed={'schema':'airk.private-arbiter-fixture-closed/1','evaluation':True,'release_qualified':False,'control_root':str(self.control),'instance':'old-instance','shutdown_completed':True,'service_lease_reacquired':True}
        self.status={'schema':'airk.arbiter.status/2','initialization':None,'configuration':{'catalog':'original-catalog'},'focus':{'queued':0,'running':0},'context':{'queued':0,'running':0}}
        refs={}
        for name,value in [('ready',self.ready),('closed',self.closed),('status',self.status)]:
            path=self.old/(name+'.json');path.write_text(json.dumps(value),encoding='utf-8');reference=module.pin(path);refs[name]={k:reference[k] for k in ('path','sha256')}
        self.descriptor=self.root/'restart.json';self.descriptor.write_text(json.dumps(refs),encoding='utf-8')
        self.packet={'work':str(self.work),'inputs':{'sha256':'original-input'},'restart':module.pin(self.descriptor),'existing_control_root':str(self.control)}
    def test_ordinary_root_behavior_and_explicit_restart_requirement(self):
        self.assertIsNone(module.restart_inputs({'work':str(self.work)}))
        with self.assertRaises(ValueError):module.restart_inputs({'work':str(self.work),'existing_control_root':str(self.control)})
    def test_original_closed_owner_is_preserved_without_fresh_root_relabel(self):
        result=module.restart_inputs(self.packet)
        self.assertEqual(result['control_root'],str(self.control));self.assertEqual(result['prior_instance'],'old-instance')
        new_ready=dict(self.ready,health=dict(self.ready['health'],instance='new-instance',executable_sha256='new-image'))
        new_closed=dict(self.closed,instance='new-instance')
        self.assertTrue(module.closure_matches(new_ready,new_closed,self.work,'original-input','new-image',control_root=str(self.control),prior_instance='old-instance'))
        self.assertFalse(module.closure_matches(new_ready,new_closed,self.work,'original-input','new-image'))
    def test_foreign_control_and_missing_descriptor_are_refused(self):
        foreign=self.root/'foreign';foreign.mkdir()
        for change in ({'existing_control_root':str(foreign)},{'existing_control_root':None}):
            with self.assertRaises(ValueError):module.restart_inputs(dict(self.packet,**change))
    def test_changed_prior_evidence_refuses_before_launch(self):
        (self.old/'status.json').write_text('{}',encoding='utf-8')
        with self.assertRaises(ValueError):module.restart_inputs(self.packet)
    def test_new_instance_and_original_closure_are_required(self):
        self.assertFalse(module.closure_matches(self.ready,self.closed,self.work,'original-input','old-image',control_root=str(self.control),prior_instance='old-instance'))
        self.assertFalse(module.closure_matches(self.ready,dict(self.closed,shutdown_completed=False),self.work,'original-input','old-image',control_root=str(self.control)))
if __name__=='__main__':unittest.main()
