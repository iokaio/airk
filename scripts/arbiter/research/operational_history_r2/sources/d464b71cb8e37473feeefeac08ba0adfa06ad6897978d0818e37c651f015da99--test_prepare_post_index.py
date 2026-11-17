# SPDX-License-Identifier: Apache-2.0
"""Refusal/identity controls only; no synthesized native authority or execution."""
import json
from pathlib import Path
import tempfile
import unittest
import os
from prepare_post_index import json_lines,setup_index,semantic_ready,pin,verify,ROOT,same_physical_path,catalog_singleton
from observe_semantic_ready import remaining,phase,canonical

class ContinuationControls(unittest.TestCase):
    def test_source_bound_workspace_root(self):
        self.assertTrue((ROOT/'crates/airk-service/src/service_runtime/evaluation_fixture_tests.rs').is_file())

    def test_nonreceipt_diagnostic_cannot_supply_an_index_id(self):
        with tempfile.TemporaryDirectory() as folder:
            path=Path(folder)/'original.log'
            path.write_text('diagnostic\n'+json.dumps({'guessed_index_id':'idx_not_authority'})+'\n',encoding='utf-8')
            with self.assertRaisesRegex(ValueError,'one original native setup'):
                setup_index(path,source_root=folder,collection='actual')

    def test_opaque_pass_flag_cannot_supply_semantic_readiness(self):
        with tempfile.TemporaryDirectory() as folder:
            path=Path(folder)/'original.log'
            path.write_text(json.dumps({'passed':True,'semantic_ready':True}),encoding='utf-8')
            with self.assertRaisesRegex(ValueError,'original full canonical'):
                semantic_ready(path,'actual-index','actual-pipeline','actual-collection')

    def test_identity_refuses_equal_length_artifact_change(self):
        with tempfile.TemporaryDirectory() as folder:
            path=Path(folder)/'original';path.write_bytes(b'first');before=pin(path);path.write_bytes(b'other')
            with self.assertRaisesRegex(ValueError,'original pinned input changed'):verify(before)

    def test_json_log_preserves_native_objects_and_ignores_only_nonjson_lines(self):
        with tempfile.TemporaryDirectory() as folder:
            path=Path(folder)/'original.log';value={'unknown_future_field':{'keep':['all',1]}}
            path.write_text('ordinary stderr\n'+json.dumps(value)+'\n',encoding='utf-8')
            self.assertEqual(json_lines(path),[value])

    def test_semantic_observer_cannot_refresh_expired_original_setup_deadline(self):
        self.assertEqual(remaining(30_000_000_000,clock=lambda:25_000_000_000),5)
        with self.assertRaises(TimeoutError):remaining(30_000_000_000,clock=lambda:30_000_000_000)

    def test_status_phase_stops_on_unknown_failure_and_does_not_score_probe(self):
        partial={'index_state':'available','semantic_readiness':{'state':'partial'},'discovery_readiness':{'state':'partial'}}
        self.assertFalse(phase(partial))
        for state in ('failed','disabled','unavailable','unknown'):
            changed=dict(partial,semantic_readiness={'state':state})
            with self.assertRaises(ValueError):phase(changed)

    def test_actual_discovery_states_partial_accepts_running_paused_failed_refuse(self):
        document={'index_state':'available','semantic_readiness':{'state':'ready'}}
        self.assertFalse(phase(dict(document,discovery_readiness={'state':'partial'})))
        self.assertTrue(phase(dict(document,discovery_readiness={'state':'ready'})))
        for state in ('running','paused','failed','unknown'):
            with self.assertRaises(ValueError):phase(dict(document,discovery_readiness={'state':state}))

    def test_samefile_accepts_actual_extended_windows_path_without_guessing(self):
        with tempfile.TemporaryDirectory() as folder:
            path=Path(folder)/'ready.json';path.write_bytes(b'actual file')
            self.assertTrue(same_physical_path(path,path))
            self.assertFalse(same_physical_path(path,Path(folder)/'missing'))
            if os.name=='nt':self.assertTrue(same_physical_path('\\\\?\\'+str(path.resolve()),path))

    def test_retained_actual_pretty_status_and_compact_setup_parse_without_new_calls(self):
        base=ROOT/'artifacts/advanced-split/stage-service-preparation-r1/cal-execution-r102'
        log=base/'semantic-ready-owned-actual/000-status.log'
        envelope=json.loads(log.read_text(encoding='utf-8'))
        self.assertEqual(json_lines(log),[envelope])
        value=canonical(log)
        self.assertEqual(value['semantic_readiness']['completed_artifacts'],576)
        self.assertEqual(value['semantic_readiness']['selected_artifacts'],753)
        self.assertFalse(phase(value))
        setup=base/'setup-index-owned-actual/00-actual-index-setup.log'
        actual=json_lines(setup)
        self.assertEqual([x['status'] for x in actual],['catalog_index_setup_draft','catalog_index_setup_applied'])
        source=ROOT/'artifacts/advanced-split/models/context-selected-native-data-anchor-r1/CAL/source-fixture/sources/hotpot'
        self.assertEqual(setup_index(setup,source_root=source,collection='selected-context-cal-r1'),
                         'idx_2af3a1b378f33bc080f534853c81a713')

    def test_actual_duplicate_catalog_refuses_and_clean_setup_singleton_is_reused(self):
        base=ROOT/'artifacts/advanced-split/stage-service-preparation-r1/cal-execution-r102'
        setup=[x['receipt'] for x in json_lines(base/'setup-index-owned-actual/00-actual-index-setup.log') if x.get('status')=='catalog_index_setup_applied'][0]
        duplicate=[base/'membership-diagnostic-actual'/(kind+'.log') for kind in ('projects','memberships','indexes')]
        with self.assertRaisesRegex(ValueError,'exactly one current membership'):catalog_singleton(*duplicate,setup)
        clean=[base/'post-index-corrected-actual'/(kind+'-after-cleanup.log') for kind in ('projects','memberships','indexes')]
        actual=catalog_singleton(*clean,setup)
        self.assertEqual(actual['project']['id'],'prj_2aaf8fd2373a635c29629a0e310d95c4')
        self.assertEqual(actual['membership']['index_id'],'idx_2af3a1b378f33bc080f534853c81a713')

if __name__=='__main__':unittest.main()
