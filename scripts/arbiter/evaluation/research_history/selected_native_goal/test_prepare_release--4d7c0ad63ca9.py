# SPDX-License-Identifier: Apache-2.0
import copy
from pathlib import Path
import unittest
from prepare_release import postready,add
class PostReadyTests(unittest.TestCase):
    def fixture(self):
        control=Path('synthetic-control').resolve()
        ready={'schema':'airk.private-arbiter-fixture/1','evaluation':True,'release_qualified':False,'control_root':str(control),'health':{'instance':'instance'}}
        binding={'schema':'airk.private-arbiter-index-bindings/1','evaluation':True,'release_qualified':False,'instance':'instance','namespace':'namespace','control_root':str(control)}
        status={'schema':'airk.arbiter.status/2','configuration':{'catalog':'namespace'}}
        for stage,profile in [('focus','focus-manifest'),('context','context-manifest')]:
            status['configuration'][stage]={'enabled':True,'selected_profile':profile};status[stage]={'availability':'ready','selected_profile':profile,'active_profile':profile}
        return ready,binding,status,control
    def test_exact_postready_stage_identities(self):
        ready,binding,status,control=self.fixture();self.assertEqual(postready(ready,binding,status,'namespace',control),('focus-manifest','context-manifest'))
    def test_warm_queued_is_not_ready(self):
        ready,binding,status,control=self.fixture();status['context']['availability']='warming'
        with self.assertRaises(ValueError):postready(ready,binding,status,'namespace',control)
    def test_foreign_active_profile_refuses(self):
        ready,binding,status,control=self.fixture();status['focus']['active_profile']='foreign'
        with self.assertRaises(ValueError):postready(ready,binding,status,'namespace',control)
    def test_disabled_consent_refuses(self):
        ready,binding,status,control=self.fixture();status['configuration']['focus']['enabled']=False
        with self.assertRaises(ValueError):postready(ready,binding,status,'namespace',control)
    def test_native_instance_or_namespace_mismatch_refuses(self):
        for key in ('instance','namespace'):
            ready,binding,status,control=self.fixture();binding[key]='foreign'
            with self.assertRaises(ValueError):postready(ready,binding,status,'namespace',control)
    def test_conflicting_frozen_identity_cannot_repin(self):
        pins={};add(pins,'synthetic-source','01'*32)
        with self.assertRaises(ValueError):add(pins,'synthetic-source','02'*32)
if __name__=='__main__':unittest.main()
