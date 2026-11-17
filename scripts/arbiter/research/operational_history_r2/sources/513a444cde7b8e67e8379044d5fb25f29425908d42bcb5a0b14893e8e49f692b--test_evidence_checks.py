# SPDX-License-Identifier: Apache-2.0
import copy
import json
from pathlib import Path
import tempfile
import hashlib
import subprocess
import unittest
from unittest.mock import patch
from evidence_checks import search_preservation, observed_race, finite_resources
from assemble_observations import assemble, close_references
from evidence_checks import REQUIRED_CASES
from run_composed import await_ready

class EvidenceControls(unittest.TestCase):
    def test_complete_search_allows_only_proved_permutation(self):
        before = {'hits':[{'id':'a','source':{'revision':1},'unknown':[1]}, {'id':'b','text':'full'}], 'generation':4,'truncated':False,'future':{'x':1}}
        after = copy.deepcopy(before)
        after['hits'].reverse()
        after['focus']={'kind':'receipt','receipt':{'original_order':['A','B'],'ranked_order':['B','A'],'applied':True,'identity':{'manifest':'p'},'scored_count':2}}
        self.assertEqual(search_preservation(before,after)['hits'],2)
        for mutate in (lambda x:x['hits'][1]['source'].update(revision=2), lambda x:x.update(future={}),lambda x:x['hits'].pop()):
            bad=copy.deepcopy(after);mutate(bad)
            with self.assertRaises(ValueError):search_preservation(before,bad)
        bad=copy.deepcopy(after);bad['focus']['receipt']['applied']=False
        with self.assertRaises(ValueError):search_preservation(before,bad)

    def test_original_dispatch_mutation_return_release(self):
        shared={'profile_manifest':'p','question_utf8_sha256':'q','dispatch_identity':7,'input_digest':'i','original_deadline_unix_ms':100,'charge_identity':'lease:1','charge_bytes':4096,'tokens':512,'scope':{'index_id':'index1'},'task':'airk.focus_rank.v1'}
        events=[dict(shared,kind='dispatch_admitted',unix_ms=1,sequence=1),dict(shared,kind='score_returned',unix_ms=5,sequence=2,outcome='success',finite_raw_scores=True,raw_scores=[-1.,2.],work={'input_digest':'i','input_tokens':512}),dict(shared,kind='job_settled',unix_ms=6,sequence=3),dict(shared,kind='charge_released',unix_ms=7,sequence=4)]
        args={'profile_manifest':'p','question_utf8_sha256':'q','dispatch_identity':7}
        observed_race(events,{'started_unix_ms':2,'completed_unix_ms':3},**args)
        for changed in (events[:2],events+[events[0]], [dict(e,original_deadline_unix_ms=200) if e['kind']=='score_returned' else e for e in events], [dict(e,charge_identity='other') if e['kind']=='charge_released' else e for e in events], [e for e in events if e['kind']!='job_settled']):
            with self.assertRaises(ValueError):observed_race(changed,{'started_unix_ms':2,'completed_unix_ms':3},**args)
        with self.assertRaises(ValueError):observed_race(events,{'started_unix_ms':5,'completed_unix_ms':6},**args)

    def test_actual_finite_resource_telemetry(self):
        good={'observed_rss_bytes':4,'memory_reserved_bytes':8,'memory_limit_bytes':10,'telemetry_complete':True}
        finite_resources(good)
        for update in ({'observed_rss_bytes':float('nan')},{'telemetry_complete':False},{'memory_reserved_bytes':11}):
            with self.assertRaises(ValueError):finite_resources(dict(good,**update))

    def test_assembly_never_converts_supplied_pass_to_qualification(self):
        with tempfile.TemporaryDirectory() as directory:
            root=Path(directory); evidence=root/'raw.txt'; evidence.write_text('synthetic control, no service proof',encoding='utf-8')
            mapping=root/'mapping.json'; output=root/'observed.json'
            mapping.write_text(json.dumps({'schema':'airk.private.actual-composed-service/1','proof_class':'ActualComposed',
                'qualification_granted':True,'cases':{k:{'verdict':'pass','evidence':[{'path':str(evidence)}]} for k in REQUIRED_CASES}}),encoding='utf-8')
            assemble(mapping,output); result=json.loads(output.read_text(encoding='utf-8'))
            self.assertFalse(result['qualification_granted'])
            self.assertTrue(all(x['verdict']=='incomplete' for x in result['cases'].values()))

    def test_reference_closure_refuses_changed_original_artifact(self):
        with tempfile.TemporaryDirectory() as directory:
            source=Path(directory)/'raw.txt'; source.write_bytes(b'original')
            reference=close_references({'path':str(source)})
            source.write_bytes(b'changed')
            with self.assertRaises(ValueError):close_references(reference)

    def test_native_owner_failed_and_draining_status_stop_without_retry(self):
        cases=json.loads((Path(__file__).resolve().parents[3]/'fixtures/split/arbiter/stage_cases.json').read_text(encoding='utf-8'))
        cases={x['id']:x['status'] for x in cases['cases']}
        for terminal in ('failed','draining','constrained','missing_assets'):
            with self.subTest(terminal=terminal), tempfile.TemporaryDirectory() as directory:
                root=Path(directory); rows=[]
                for number,name in enumerate(('both_disabled','both_enabled')):
                    raw=json.dumps(cases[name]).encode();(root/f'{number:03d}-stdout.bin').write_bytes(raw)
                    rows.append({'request':{'kind':'native_command'},'result':{'stdout_sha256':hashlib.sha256(raw).hexdigest()}})
                step={'stage':'focus','argv':['reviewed-image','arbiter','status'],'configuration_observation_step':1,
                      'prior_idle_observation_step':0,'profile_manifest':cases['both_enabled']['focus']['selected_profile'],'deadline_seconds':30}
                response=subprocess.CompletedProcess(step['argv'],0,json.dumps(cases[terminal]).encode(),b'')
                with patch('run_composed.time.monotonic',return_value=1), patch('run_composed.subprocess.run',return_value=response) as execute:
                    with self.assertRaises(ValueError):await_ready(step,rows,{1:0},root,2,['reviewed-image'],lambda:60)
                    self.assertEqual(execute.call_count,1)
                retained=json.loads((root/'002-warm-observations.json').read_text())
                self.assertFalse(retained['ready_observed']);self.assertEqual(len(retained['observations']),1)

    def test_warm_original_deadline_expiry_never_launches_another_status(self):
        cases=json.loads((Path(__file__).resolve().parents[3]/'fixtures/split/arbiter/stage_cases.json').read_text(encoding='utf-8'))
        cases={x['id']:x['status'] for x in cases['cases']}
        with tempfile.TemporaryDirectory() as directory:
            root=Path(directory); rows=[]
            for number,name in enumerate(('both_disabled','both_enabled')):
                raw=json.dumps(cases[name]).encode();(root/f'{number:03d}-stdout.bin').write_bytes(raw)
                rows.append({'request':{'kind':'native_command'},'result':{'stdout_sha256':hashlib.sha256(raw).hexdigest()}})
            step={'stage':'focus','argv':['reviewed-image','arbiter','status'],'configuration_observation_step':1,
                  'prior_idle_observation_step':0,'profile_manifest':cases['both_enabled']['focus']['selected_profile'],'deadline_seconds':30}
            with patch('run_composed.time.monotonic',return_value=31),patch('run_composed.subprocess.run') as execute:
                with self.assertRaises(TimeoutError):await_ready(step,rows,{1:0},root,2,['reviewed-image'],lambda:60)
                execute.assert_not_called()

if __name__=='__main__':unittest.main()
