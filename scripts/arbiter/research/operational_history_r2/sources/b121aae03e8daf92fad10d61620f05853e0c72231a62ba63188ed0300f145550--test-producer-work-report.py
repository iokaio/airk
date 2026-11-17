# SPDX-License-Identifier: Apache-2.0
"""Draft parser fixtures only; no native service or source corpus is touched."""
import importlib.util
import json
from pathlib import Path
import tempfile
import types
import unittest
from unittest.mock import Mock, patch

spec = importlib.util.spec_from_file_location('producer_report', Path(__file__).with_name('producer-work-report.py'))
report = importlib.util.module_from_spec(spec)
spec.loader.exec_module(report)

class ParserCases(unittest.TestCase):
    def fixture(self):
        owner = tempfile.TemporaryDirectory()
        self.addCleanup(owner.cleanup)
        fixture = types.SimpleNamespace(owner=Path(owner.name), projects={'airk':{'binding':'d'*64}})
        state = {'nonce':'a'*32, 'runtime':{'install_sha256':'b'*64,'version':'1.8.0'},
                 'parents':{10}, 'jobs':{'c'*64:{'project':'d'*64,'terminal_state':'failed'}}, 'job_limit':False}
        owned = {'schema':1,'run':'a'*32,'runtime_install_sha256':'b'*64,
                 'parent_process_id':10,'worker_process_id':11,'project':'d'*64,'worker':'f'*32,'service':'0'*32}
        return fixture, state, owned
    def event(self, stage, sequence, outcome, calls=0):
        return {'schema':1,'run':'a'*32,'runtime_install_sha256':'b'*64,'native_version':'1.8.0',
                'job':'c'*64,'project':'d'*64,'process_id':11,'worker':'f'*32,'service':'0'*32,'fencing_generation':1,'sequence':sequence,'stage':stage,
                'outcome':outcome,'elapsed_ns':sequence,'suppressed_samples':0,
                'boundary':'SemanticUpdate' if stage=='producer_boundary' else None,
                'metrics':{} if stage=='started' else {name:{'calls':calls,'failures':0,'elapsed_ns':calls,
                    'items':calls,'bytes':calls,'saturated':False} for name in report.METRICS}}
    def write(self, fixture, owner, events):
        lines = [b'AIRK_PRODUCER_OWNER_V1 '+json.dumps(owner).encode()+b'\n']
        lines += [b'AIRK_PRODUCER_V1 '+json.dumps(event).encode()+b'\n' for event in events]
        (fixture.owner/'service.stderr').write_bytes(b''.join(lines))
    def test_failure_survives_later_success_and_missing_finish_is_partial(self):
        fixture,state,owner=self.fixture()
        self.write(fixture,owner,[self.event('started',0,'in_progress'),
            self.event('producer_boundary',1,'failed',1),self.event('producer_boundary',2,'completed_interval',2)])
        observed=report.collect(fixture,state)
        row=observed['jobs'][0]
        self.assertEqual(row['first_failed_interval']['sequence'],1)
        self.assertTrue(row['partial'])
        self.assertEqual(row['job_terminal_observed'],'failed')
    def test_unowned_runtime_and_private_extra_fields_are_rejected_without_copy(self):
        fixture,state,owner=self.fixture()
        private=self.event('producer_boundary',1,'failed',1)
        private['private_path']='SECRET_SOURCE_PATH'
        wrong=self.event('execute_finished',2,'execute_error',2)
        wrong['runtime_install_sha256']='e'*64
        self.write(fixture,owner,[private,wrong])
        observed=report.collect(fixture,state)
        self.assertEqual(observed['rejected_records'],2)
        self.assertNotIn('SECRET_SOURCE_PATH',json.dumps(observed))
        self.assertEqual(observed['coverage'],'partial')
    def test_finished_execute_is_not_fabricated_terminal_job_and_counter_reset_is_partial(self):
        fixture,state,owner=self.fixture()
        state['jobs']['c'*64]['terminal_state']=None
        self.write(fixture,owner,[self.event('started',0,'in_progress'),
            self.event('producer_boundary',1,'completed_interval',9),self.event('execute_finished',2,'execute_succeeded',1)])
        observed=report.collect(fixture,state)
        row=observed['jobs'][0]
        self.assertTrue(row['counter_regression'])
        self.assertTrue(row['partial'])
        self.assertIsNone(row['job_terminal_observed'])

    def test_start_requires_zero_sequence_and_complete_initial_frame(self):
        for field, value in [('sequence',5),('elapsed_ns',1),('suppressed_samples',1)]:
            with self.subTest(field=field):
                fixture,state,owner=self.fixture()
                start=self.event('started',0,'in_progress'); start[field]=value
                self.write(fixture,owner,[start,self.event('execute_finished',6,'execute_error',1)])
                observed=report.collect(fixture,state)
                self.assertEqual(observed['coverage'],'partial')
                self.assertEqual(observed['jobs'],[])
                self.assertEqual(observed['missing_job_intervals'],1)
                self.assertEqual(observed['rejected_records'],2)

    def test_identity_phase_and_terminal_violations_stay_partial_after_valid_finish(self):
        for violation in ('fence','process','duplicate_start','duplicate_finish','after_finish','sequence_gap'):
            with self.subTest(violation=violation):
                fixture,state,owner=self.fixture()
                start=self.event('started',0,'in_progress')
                finish=self.event('execute_finished',1,'execute_error',1)
                if violation == 'fence':
                    bad=self.event('producer_boundary',1,'failed',1); bad['fencing_generation']=2
                    events=[start,bad,finish]
                elif violation == 'process':
                    bad=self.event('producer_boundary',1,'failed',1); bad['process_id']=12
                    events=[start,bad,finish]
                elif violation == 'duplicate_start': events=[start,dict(start),finish]
                elif violation == 'duplicate_finish': events=[start,finish,self.event('execute_finished',2,'execute_error',2)]
                elif violation == 'after_finish': events=[start,finish,self.event('producer_boundary',2,'failed',2)]
                else: events=[start,self.event('producer_boundary',2,'failed',1),finish]
                self.write(fixture,owner,events)
                observed=report.collect(fixture,state)
                self.assertEqual(observed['coverage'],'partial')
                self.assertGreater(observed['rejected_records'],0)
                self.assertEqual(observed['jobs'][0]['last']['sequence'],1)

    def test_valid_start_boundary_finish_is_complete_correlated_interval(self):
        fixture,state,owner=self.fixture()
        self.write(fixture,owner,[self.event('started',0,'in_progress'),
            self.event('producer_boundary',1,'failed',1),self.event('execute_finished',2,'execute_error',2)])
        observed=report.collect(fixture,state)
        self.assertEqual(observed['coverage'],'observed')
        self.assertFalse(observed['jobs'][0]['partial'])
        self.assertEqual(observed['jobs'][0]['first_failed_interval']['sequence'],1)

    def test_probe_import_install_failures_still_close_owned_fixture_and_preserve_primary(self):
        spec=importlib.util.spec_from_file_location('producer_benchmark_fixture',Path(__file__).with_name('benchmark-real-corpora.py'))
        bench=importlib.util.module_from_spec(spec); spec.loader.exec_module(bench)
        for stage in ('import','install'):
            for close_fails in (False,True):
                with self.subTest(stage=stage,close_fails=close_fails), tempfile.TemporaryDirectory() as temporary:
                    owner=Path(temporary).resolve(); opened=[]
                    class Fixture:
                        def __init__(self,runtime,directory,**kwargs):
                            self.owner=directory; self.environment={}; self.child=None
                            stream=(directory/'service.stderr').open('wb'); opened.append(stream)
                            def close():
                                stream.close()
                                if close_fails: raise OSError('injected close failure')
                            self.log=types.SimpleNamespace(close=close)
                    args=types.SimpleNamespace(airk=owner/'a',bookbuilder=owner/'b',runtime_dir=owner/'runtime',
                        report=owner/'report',state_parent=owner,runtime_sha256='b'*64,samples=1,semantic=False,
                        excerpt_path=None,command_timeout=1,job_timeout=1,producer_work=True)
                    primary=RuntimeError('injected telemetry preparation failure')
                    fake_module=types.SimpleNamespace(install=Mock(side_effect=primary))
                    fake_spec=types.SimpleNamespace(loader=types.SimpleNamespace(exec_module=lambda module: None))
                    result={}
                    with patch.object(bench,'isolate',return_value=[owner/'a',owner/'b']), \
                         patch.object(bench,'source_identity',return_value={}), \
                         patch.object(bench,'verify_runtime',return_value={'install_sha256':'b'*64,'version':'1.8.0'}), \
                         patch.object(bench,'environment_facts',return_value={}), \
                         patch.object(bench,'clients',return_value=types.SimpleNamespace(Fixture=Fixture)), \
                         patch.object(bench.importlib.util,'spec_from_file_location',side_effect=primary if stage=='import' else None,return_value=fake_spec), \
                         patch.object(bench.importlib.util,'module_from_spec',return_value=fake_module):
                        with self.assertRaises(RuntimeError) as error: bench.run(args,result)
                    self.assertIs(error.exception,primary)
                    self.assertTrue(opened[0].closed)
                    self.assertEqual(result['cleanup_failed'],close_fails)
                    self.assertEqual(result['cleanup_detail']['failed_stages'],['log_close'] if close_fails else [])

if __name__ == '__main__': unittest.main()
