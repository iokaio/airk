# SPDX-License-Identifier: Apache-2.0
"""Synthetic zero-NN seam controls, not native lifecycle qualification."""
import copy,json,tempfile,unittest
from pathlib import Path
from wait_warm import WarmWait,SCHEMA
class Tests(unittest.TestCase):
    def setUp(self):
        self.temp=tempfile.TemporaryDirectory();self.addCleanup(self.temp.cleanup);self.work=Path(self.temp.name)
        self.path=self.work/'warm-completions.jsonl';self.path.write_bytes(b'');self.calls=0;self.now=0
        self.configuration={'revision':4,'focus':{'enabled':False,'selected_profile':None},'context':{'enabled':True,'selected_profile':'model'}}
        self.row={'schema':SCHEMA,'observer_id':'instance','sequence':1,'stage':'context','profile_manifest':'model','expected_configuration':self.configuration,'actual_configuration':self.configuration,'lifecycle_serial':2,'current_lifecycle_serial':2,'accepted_configure_revision':4,'captured_configure_revision':4,'outcome':'ready','operation_succeeded':True,'canceled':False,'phase':'ready','active_profile':'model','native_configuration_revision':4}
        self.status={'schema':'airk.arbiter.status/2','initialization':None,'configuration':self.configuration,'context':{'availability':'ready','selected_profile':'model','active_profile':'model'}}
    def write(self,row):self.path.write_text(json.dumps(row)+'\n',encoding='utf-8')
    def waiter(self,callback=None):
        parent=self
        class Events:
            def wait(self,deadline):
                parent.calls+=1
                if callback:callback()
                else:raise TimeoutError('synthetic event deadline')
        value=WarmWait(self.work,{'warm_observer_id':'instance','evaluation':True,'release_qualified':False},Events(),lambda:self.now);value.arm();return value
    def run_wait(self,waiter,status=None):
        return waiter.wait_ready(self.configuration,'context','model',30_000_000_000,lambda:30,lambda budget:copy.deepcopy(status or self.status))
    def test_actual_event_wakes_one_status(self):
        waiter=self.waiter(lambda:self.write(self.row));self.assertEqual(self.run_wait(waiter),self.status);self.assertEqual(self.calls,1);self.assertEqual(waiter.trace[0]['status_calls'],1)
    def test_completion_before_wait_is_scanned_without_lost_wakeup(self):
        waiter=self.waiter();self.write(self.row);self.run_wait(waiter);self.assertEqual(self.calls,0)
    def test_stale_preconfigure_event_is_not_reused(self):
        self.write(self.row);waiter=self.waiter()
        with self.assertRaises(TimeoutError):self.run_wait(waiter)
    def test_failed_and_superseded_stop_without_status(self):
        for outcome in ('failed','superseded'):
            self.path.write_bytes(b'');waiter=self.waiter();row=copy.deepcopy(self.row);row['outcome']=outcome;self.write(row)
            with self.assertRaises(ValueError):self.run_wait(waiter)
    def test_stale_native_fields_are_rejected(self):
        for key,value in [('active_profile',None),('native_configuration_revision',3),('canceled',True),('operation_succeeded',False),('current_lifecycle_serial',9),('accepted_configure_revision',9)]:
            self.path.write_bytes(b'');waiter=self.waiter();row=copy.deepcopy(self.row);row[key]=value;self.write(row)
            with self.assertRaises(ValueError):self.run_wait(waiter)
    def test_changed_final_status_is_rejected_without_retry(self):
        waiter=self.waiter();self.write(self.row);status=copy.deepcopy(self.status);status['context']['active_profile']=None
        with self.assertRaises(ValueError):self.run_wait(waiter,status)
        self.assertEqual(self.calls,0)
    def test_original_deadline_stops_before_call(self):
        waiter=self.waiter();self.write(self.row);self.now=30_000_000_000
        with self.assertRaises(TimeoutError):self.run_wait(waiter)
    def test_foreign_observer_and_missing_sequence_rejected(self):
        for key,value in [('observer_id','foreign'),('sequence',2)]:
            self.path.write_bytes(b'');waiter=self.waiter();row=copy.deepcopy(self.row);row[key]=value;self.write(row)
            with self.assertRaises(ValueError):self.run_wait(waiter)
    def test_unrelated_directory_event_is_only_wakeup(self):
        def event():
            if self.calls==2:self.write(self.row)
        waiter=self.waiter(event);self.run_wait(waiter);self.assertEqual(self.calls,2)
    def test_closed_journal_without_completion_fails(self):
        waiter=self.waiter();(self.work/'warm-completions.closed.json').write_text('{}')
        with self.assertRaises(ValueError):self.run_wait(waiter)
if __name__=='__main__':unittest.main()
