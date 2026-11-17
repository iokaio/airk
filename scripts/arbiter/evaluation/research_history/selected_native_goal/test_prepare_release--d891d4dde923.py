# SPDX-License-Identifier: Apache-2.0
import copy
import hashlib
import json
from pathlib import Path
import unittest
import tempfile
from prepare_release import postready,add,verify_build_input
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
    def test_actual_retained_cli_is_raw_status_and_not_ready(self):
        path=Path(__file__).resolve().parents[3]/'stage-service-preparation-r1'/'cal-execution-r102'/'context-first-preflight-status.log'
        raw=path.read_bytes()
        self.assertEqual(hashlib.sha256(raw).hexdigest(),'72138764b07447d86f8c6d7d04d7b252f5d3bb5291b9274267167863383e8a52')
        status=json.loads(raw)
        self.assertEqual(status['schema'],'airk.arbiter.status/2')
        self.assertNotIn('observation',status)
        ready,binding,_,control=self.fixture()
        binding['namespace']=status['configuration']['catalog']
        with self.assertRaises(ValueError):postready(ready,binding,status,binding['namespace'],control)
    def test_conflicting_frozen_identity_cannot_repin(self):
        pins={};add(pins,'synthetic-source','01'*32)
        with self.assertRaises(ValueError):add(pins,'synthetic-source','02'*32)
    def test_recorded_missing_absent_passes(self):
        with tempfile.TemporaryDirectory() as directory:
            path=Path(directory)/'absent';pins={};absent={}
            verify_build_input(path,'missing',pins,absent)
            self.assertEqual(pins,{})
            self.assertEqual(absent,{str(path.resolve()):'missing'})
    def test_recorded_missing_present_refuses(self):
        with tempfile.TemporaryDirectory() as directory:
            path=Path(directory)/'present';path.write_bytes(b'actual')
            with self.assertRaises(ValueError):verify_build_input(path,'missing',{}, {})
    def test_real_digest_missing_refuses(self):
        with tempfile.TemporaryDirectory() as directory:
            with self.assertRaises(FileNotFoundError):verify_build_input(Path(directory)/'absent','00'*32,{}, {})
if __name__=='__main__':unittest.main()
