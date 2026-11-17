# SPDX-License-Identifier: Apache-2.0
"""Mocked audit of ignored P10 observation adapters; never native qualification."""
import importlib.util
import json
from pathlib import Path
import stat
from types import SimpleNamespace
import unittest
from unittest.mock import patch
import uuid

BASE=Path(__file__).resolve().parent
SPEC=importlib.util.spec_from_file_location('p10_resource_draft',BASE/'p10-resource-campaign.py')
M=importlib.util.module_from_spec(SPEC);SPEC.loader.exec_module(M)

class DraftObservers(unittest.TestCase):
    def setUp(self):
        self.owner=BASE/('resource-adapter-audit-'+uuid.uuid4().hex)
        self.owner.mkdir();self.data=self.owner/'data';self.data.mkdir()

    def test_exact_owned_roots_and_logical_census_without_reading_file_contents(self):
        (self.data/'one').write_bytes(b'1234');(self.data/'two').write_bytes(b'12345678')
        with patch.object(Path,'read_bytes',side_effect=AssertionError('data contents must never be read')):
            value=M.disk_sample(self.owner,{'original_data':self.data})
        self.assertEqual(value['roots']['original_data']['logical_bytes'],12)
        self.assertEqual(value['roots']['original_data']['files'],2)
        with self.assertRaises(AssertionError):M.disk_sample(self.owner,{'original_data':BASE})
        with self.assertRaises(AssertionError):M.disk_sample(self.owner,{'user_root':self.data})
        with patch.object(M,'MAX_CENSUS_ENTRIES',1):
            with self.assertRaisesRegex(AssertionError,'budget'):M.disk_sample(self.owner,{'original_data':self.data})

    def test_redirected_census_entry_refuses_without_following_it(self):
        entry=SimpleNamespace(stat=lambda **kw:SimpleNamespace(st_mode=stat.S_IFREG,st_size=1,st_file_attributes=0x400))
        class Census:
            def __enter__(self):return iter([entry])
            def __exit__(self,*args):pass
        with patch.object(M.os,'scandir',return_value=Census()):
            with self.assertRaisesRegex(AssertionError,'redirected'):M.disk_sample(self.owner,{'original_data':self.data})

    def test_resources_retains_complete_native_future_fields_and_one_read(self):
        calls=[]
        reply={'status':'resources','observation':{'observed_rss_bytes':7,'telemetry_complete':False,'future_counter':9},'future_envelope':'retained'}
        fixture=SimpleNamespace(rpc=lambda method:(calls.append(method),reply)[1])
        value=M.observe_cached_resources(fixture)
        self.assertEqual(calls,['resources']);self.assertIs(value['resources_native'],reply)

    def test_counter_adapter_delegates_only_explicit_owned_handles_to_existing_owner(self):
        child=SimpleNamespace(pid=42)
        calls=[]
        owner=SimpleNamespace(process_metrics=lambda process,exe:(calls.append((process,exe)),{'cpu_us':10})[1])
        with patch.object(M,'load_owner',return_value=owner):value=M.observe_owned_processes(BASE/'p10-worktree',{'service':(child,Path(__file__))})
        self.assertIs(calls[0][0],child);self.assertEqual(value['owned_process_metrics']['service']['cpu_us'],10)
        self.assertEqual(value['owned_process_metrics']['service']['process_id'],42)

    def test_summary_separates_overlap_unknowns_partial_telemetry_and_process_identity(self):
        events=[]
        def add(**fields):events.append({'schema':M.SCHEMA,'sequence':len(events),'phase':'split_active',**fields})
        add(resources_native={'status':'resources','observation':{'observed_rss_bytes':100,'telemetry_complete':False}})
        add(resources_native={'status':'resources','observation':{'observed_rss_bytes':80,'telemetry_complete':True}})
        add(owned_process_metrics={'service':{'process_id':42,'cpu_us':100,'peak_rss_bytes':50}})
        add(owned_process_metrics={'service':{'process_id':42,'cpu_us':500,'peak_rss_bytes':60}})
        for _ in range(4):add(query={'outcome':'success','elapsed_ms':2,'split_observed_running':True})
        summary=M.summarize(BASE/'p10-worktree',events)
        self.assertEqual(summary['contention_coverage'],'observed')
        self.assertFalse(summary['native_qualification']);self.assertIsNone(summary['whole_split_cpu_us']);self.assertIsNone(summary['physical_disk_peak_bytes'])
        self.assertEqual(summary['sampled_rss_max_bytes_complete_telemetry'],80);self.assertEqual(summary['sampled_rss_max_bytes_incomplete_telemetry'],100)
        self.assertEqual(summary['named_owned_process_counter_coverage']['service']['observed_cpu_delta_us'],400)
        add(owned_process_metrics={'service':{'process_id':43,'cpu_us':600}})
        summary=M.summarize(BASE/'p10-worktree',events)
        self.assertIsNone(summary['named_owned_process_counter_coverage']['service']['observed_cpu_delta_us'])
        self.assertEqual(M.summarize(BASE/'p10-worktree',[])['contention_coverage'],'inconclusive_no_required_overlap')
        proof=[{'schema':M.SCHEMA,'sequence':0,'phase':'split_active','same_job_running_at_both_bounds':True,'running_wave_successes':4}]
        self.assertEqual(M.summarize(BASE/'p10-worktree',proof)['queries_success_with_split_observed_running'],4)
        proof[0]['scenario']='cancel'
        self.assertEqual(M.summarize(BASE/'p10-worktree',proof)['contention_coverage'],'not_applicable_metadata_only_cancel')

    def test_scope_refusal_is_failed_sample_and_reconfirmation_cost_is_separate(self):
        reply={'result':{'isError':True,'structuredContent':{'error':{'code':'combined_scope_expired','retryable':False,'query_replay_safe':False}}}}
        events=[{'schema':M.SCHEMA,'sequence':0,'phase':'during','elapsed_us':40,
                 'query':{'outcome':'scope_refusal','elapsed_ms':3,'attempts':1,'split_observed_running':False,'reply':reply}},
                {'schema':M.SCHEMA,'sequence':1,'phase':'during','elapsed_us':60,
                 'query':{'outcome':'indeterminate_read_failure','elapsed_ms':7,'attempts':1,'split_observed_running':False,
                          'reply':{'error':{'code':'combined_read_unavailable','query_submitted':None,'retryable':False,'query_replay_safe':False}}}},
                {'schema':M.SCHEMA,'sequence':2,'phase':'reconfirmation','elapsed_us':90,'outcome':'confirmed','elapsed_us_separate_from_query':17}]
        summary=M.summarize(BASE/'p10-worktree',events)
        self.assertEqual(summary['queries_success_with_split_observed_running'],0)
        self.assertEqual(summary['contention_coverage'],'inconclusive_no_required_overlap')
        self.assertIn('during:scope_refusal',summary['latency'])
        self.assertIn('during:indeterminate_read_failure',summary['latency'])
        self.assertNotIn('during:success',summary['latency'])
        self.assertEqual(summary['reconfirmation_elapsed_us_by_outcome'],{'confirmed':[17]})
        self.assertEqual(summary['case_elapsed_us_through_last_observation'],90)
        self.assertIs(events[0]['query']['reply'],reply)

if __name__=='__main__':unittest.main()
