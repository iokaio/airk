# SPDX-License-Identifier: Apache-2.0
"""Pure argument/selection/provenance/teardown contracts; no native execution."""
import hashlib
import importlib.util
import json
from pathlib import Path
import shutil
import tempfile
import unittest

SCRIPT=Path(__file__).resolve().parents[1]/'check-runtime-legacy-retirement.py'
spec=importlib.util.spec_from_file_location('legacy_retirement_under_test',SCRIPT)
h=importlib.util.module_from_spec(spec);spec.loader.exec_module(h)


class LegacyRetirementHarnessTests(unittest.TestCase):
    def test_modern_all_role_proof_requires_native_false_and_preserves_exact_retained_state(self):
        selected={'active':'a'*64}
        before={'airk-launcher.exe':{'bytes':23,'sha256':'b'*64,'device':1,'inode':2},'launcher.json':None}
        proof={'launcher_repaired':False,'runtime':selected['active']}
        self.assertIs(h.qualify_launcher_repair({'schema':2},proof,selected,before,before),proof)
        with self.assertRaises(AssertionError):
            h.qualify_launcher_repair({'schema':2},{**proof,'launcher_repaired':True},selected,before,before)
        with self.assertRaises(AssertionError):
            h.qualify_launcher_repair({'schema':2},proof,selected,before,{**before,'launcher.json':{'sha256':'c'*64}})
        with self.assertRaises(AssertionError):
            h.qualify_launcher_repair({'schema':2},proof,selected,{'airk-launcher.exe':None},before)
        with self.assertRaises(AssertionError):
            h.qualify_launcher_repair({'schema':2},{**proof,'runtime':'c'*64},selected,before,before)

    def test_historical_repair_remains_actual_true_and_unknown_schema_is_refused(self):
        selected={'active':'a'*64};proof={'launcher_repaired':True,'runtime':selected['active']}
        h.qualify_launcher_repair({'schema':1},proof,selected,{}, {'changed':'owned historical repair'})
        for schema in (None,True,0,3):
            with self.assertRaises(AssertionError):h.qualify_launcher_repair({'schema':schema},proof,selected,{}, {})

    def test_retained_launcher_snapshot_records_absence_and_refuses_aliases(self):
        with tempfile.TemporaryDirectory() as temporary:
            root=Path(temporary).resolve();install=root/'installation';install.mkdir()
            before=h.retained_launcher_snapshot(root,install)
            self.assertEqual(before,{'airk-launcher.exe':None,'launcher.json':None})
            launcher=install/'airk-launcher.exe';launcher.write_bytes(b'inert predecessor bytes')
            present=h.retained_launcher_snapshot(root,install)
            self.assertEqual(present['airk-launcher.exe']['sha256'],h.sha(launcher))
            import os
            os.link(launcher,install/'unrelated-alias')
            with self.assertRaises(AssertionError):h.retained_launcher_snapshot(root,install)

    def test_old_new_search_arguments_preserve_shared_work_without_unsupported_field(self):
        old,new='1'*64,'2'*64
        before=h.search_arguments(old,old,new);after=h.search_arguments(new,old,new)
        self.assertNotIn('verification',before)
        self.assertEqual(after.pop('verification'),'off')
        self.assertEqual(before,after)
        self.assertEqual(before,{'query':'legacyhealthyneedle','mode':'lexical','limit':4,'response_mode':'full'})

    def test_unknown_query_payload_never_guesses_old_or_new_contract(self):
        with self.assertRaises(AssertionError):h.search_arguments('3'*64,'1'*64,'2'*64)

    def payloads(self,root):
        install=root/'install';(install/'runtimes').mkdir(parents=True)
        seals=[]
        for label,version in [('old','1.8.7'),('new','1.8.8')]:
            directory=root/label;directory.mkdir()
            raw=(label+' inert unit-test bytes; never executed').encode()
            (directory/'airk.exe').write_bytes(raw)
            manifest={'schema':1,'os':'windows','arch':'x86_64','version':version,
                'files':{'airk.exe':hashlib.sha256(raw).hexdigest()}}
            (directory/'airk-install.json').write_text(json.dumps(manifest))
            seal=h.seal(directory);shutil.copytree(directory,install/'runtimes'/seal['manifest_sha256'])
            seals.append(seal)
        return install,seals

    def test_selected_owner_resolves_exact_installed_old_or_new_bytes(self):
        with tempfile.TemporaryDirectory() as temporary:
            root=Path(temporary).resolve();install,seals=self.payloads(root)
            for seal in seals:
                selected={'active':seal['manifest_sha256'],'rollback':None}
                (install/'selected.json').write_text(json.dumps(selected))
                binary,observed=h.selected_payload(root,install,*seals)
                self.assertEqual(observed,selected)
                self.assertEqual(binary,install/'runtimes'/seal['manifest_sha256']/'airk.exe')
                self.assertEqual(h.sha(binary),seal['files']['airk.exe'])

    def test_selected_owner_refuses_unrelated_identity_or_tampered_installed_main(self):
        with tempfile.TemporaryDirectory() as temporary:
            root=Path(temporary).resolve();install,seals=self.payloads(root)
            (install/'selected.json').write_text(json.dumps({'active':'3'*64}))
            with self.assertRaises(AssertionError):h.selected_payload(root,install,*seals)
            old=seals[0];(install/'selected.json').write_text(json.dumps({'active':old['manifest_sha256']}))
            (install/'runtimes'/old['manifest_sha256']/'airk.exe').write_bytes(b'tampered inert bytes')
            with self.assertRaises(AssertionError):h.selected_payload(root,install,*seals)

    def test_finally_uses_selected_owner_without_candidate_argument_and_preserves_primary(self):
        class FailedOwner:
            host_parity_receipts=[]
            def drain(self):raise RuntimeError('selected old quiescence failed')
        receipt={'status':'failed','primary_error':'original old-parser refusal'}
        h.finish_owner(FailedOwner(),receipt)
        self.assertEqual(receipt['primary_error'],'original old-parser refusal')
        self.assertIn('selected old quiescence failed',receipt['stop_error'])
        self.assertEqual(receipt['status'],'failed')
        class StoppedOwner(FailedOwner):
            def drain(self):return {'selected_manifest_sha256':'old exact identity'}
        passed={'status':'passed'};h.finish_owner(StoppedOwner(),passed)
        self.assertEqual(passed['status'],'passed')
        self.assertEqual(passed['final_quiescence']['selected_manifest_sha256'],'old exact identity')


    def test_structural_diagnostic_retains_real_freshness_fields_and_type_differences(self):
        diff=h.bounded_differences({'freshness':{'state':'pending'},'flag':True},
            {'freshness':{'state':'idle'},'flag':1})
        self.assertEqual([entry['pointer'] for entry in diff['entries']],['/flag','/freshness/state'])
        self.assertFalse(diff['truncated'])

    def test_structural_diagnostic_bounds_output_without_changing_values(self):
        left={str(i):'a'*1000 for i in range(100)};right={str(i):'b'*1000 for i in range(100)}
        diff=h.bounded_differences(left,right,limit=3)
        self.assertEqual(diff['differences_shown'],3);self.assertTrue(diff['truncated'])
        self.assertTrue(all(entry['cli']['truncated'] for entry in diff['entries']))
        self.assertEqual(left['0'],'a'*1000);self.assertEqual(right['0'],'b'*1000)

    def test_complete_replies_survive_failed_parity_before_any_failure_summary(self):
        from types import SimpleNamespace
        from unittest.mock import patch
        with tempfile.TemporaryDirectory() as temporary:
            root=Path(temporary).resolve();source=root/'source';source.mkdir()
            (source/'README.md').write_bytes(b'legacyhealthyneedle original fictional bytes\n')
            binary=root/'old.exe';binary.write_bytes(b'inert unit bytes, never executed')
            project={'root':str(source),'data_root':str(root/'data'),'collection':'legacy-b','binding':'a'*64}
            body={'collection':'legacy-b','generation':1,'truncated':False,
                'hits':[{'text':'legacyhealthyneedle','truncated':False,'citation':{
                    'path':'README.md','content_hash':h.sha(source/'README.md')}}],
                'publication':{'token':'literal-fixture-publication'},'freshness':{'state':'pending'}}
            other=json.loads(json.dumps(body));other['freshness']['state']='idle'
            envelope=lambda value:{'content':[{'type':'text','text':json.dumps(value)}],'isError':False}
            class Clients:
                @staticmethod
                def remaining(_until,ceiling):return ceiling
                @staticmethod
                def read_envelope(read,_deadline):return read(None),[]
                @staticmethod
                def tool_value(value):return json.loads(value['content'][0]['text'])
                @staticmethod
                def read_retry_delay(_value):return None
            class Mcp:
                def __init__(self,*_args):pass
                def call(self,*_args):return envelope(other)
                def close(self):pass
            fixture=object.__new__(h.Fixture);fixture.root=root;fixture.control=root/'control'
            fixture.old_seal={'files':{'airk.exe':h.sha(binary)},'manifest_sha256':'1'*64}
            fixture.new_seal={'files':{'airk.exe':'2'*64},'manifest_sha256':'3'*64}
            fixture.clients=Clients()
            fixture.source_jobs={project['binding']: {}}
            fixture.wait_old_read_quiescence=lambda *_args:None  # This unit isolates failure-evidence retention.
            fixture.command=lambda *_args,**_kwargs:SimpleNamespace(returncode=0,stdout=json.dumps(envelope(body)))
            with patch.object(h,'Mcp',Mcp), patch.object(h,'validate_old_search_freshness'):
                with self.assertRaisesRegex(AssertionError,'Complete CLI/MCP legacy-body mismatch'):
                    fixture.query(binary,project)
            cli=list(root.glob('search-*-cli.json'));mcp=list(root.glob('search-*-mcp.json'))
            self.assertEqual(len(cli),1);self.assertEqual(len(mcp),1)
            self.assertEqual(Clients.tool_value(json.loads(cli[0].read_text())),body)
            self.assertEqual(Clients.tool_value(json.loads(mcp[0].read_text())),other)
            report=json.loads(next(root.glob('search-*-comparison.json')).read_text())
            self.assertEqual(report['existing_parity_projection_differences']['entries'][0]['pointer'],'/freshness/state')

    def test_old_status_barrier_uses_actual_compact_idle_and_exact_publication(self):
        project={'collection':'legacy-b'}
        job={'id':'actual-index','state':'succeeded','kind':'index',
            'outcome':{'state':'succeeded','revision':'exact-publication'}}
        value={'collection':'legacy-b','publication':{'token':'exact-publication'},
            'freshness':{'state':'pending','live_source_verified':False}}
        self.assertFalse(h.old_status_ready(value,project,job))
        value['freshness']['state']='idle'
        self.assertTrue(h.old_status_ready(value,project,job))
        value['publication']['token']='another-publication'
        with self.assertRaises(AssertionError):h.old_status_ready(value,project,job)

    def test_old_status_barrier_refuses_missing_or_failed_compact_metadata(self):
        project={'collection':'legacy-b'}
        job={'id':'index','state':'succeeded','kind':'index','outcome':{'state':'succeeded','revision':'publication'}}
        with self.assertRaises(AssertionError):h.old_status_ready({'collection':'legacy-b','freshness':{'state':'idle'}},project,job)
        value={'collection':'legacy-b','freshness':{'state':'failed','live_source_verified':False}}
        with self.assertRaises(AssertionError):h.old_status_ready(value,project,job)

    def test_full_old_search_requires_exact_job_idle_watermarks_timestamp_and_publication(self):
        project={'collection':'legacy-b'}
        job={'id':'index','outcome':{'revision':'publication'}}
        value={'collection':'legacy-b','publication':{'token':'publication'},'freshness':{
            'state':'idle','source_job':'index','requested_watermark':0,'completed_watermark':0,
            'running_watermark':None,'last_success_unix_ms':12,'error':None}}
        h.validate_old_search_freshness(value,project,job)
        for key,bad in [('source_job','wrong'),('state','pending'),('completed_watermark',True),
                ('last_success_unix_ms',None),('running_watermark',1),('error','failure')]:
            original=value['freshness'][key];value['freshness'][key]=bad
            with self.subTest(key=key),self.assertRaises(AssertionError):h.validate_old_search_freshness(value,project,job)
            value['freshness'][key]=original

    def test_old_status_wait_performs_only_metadata_reads_with_original_deadline(self):
        from types import SimpleNamespace
        from unittest.mock import patch
        project={'binding':'b','root':'source','data_root':'data','collection':'legacy-b'}
        job={'id':'index','state':'succeeded','kind':'index','outcome':{'state':'succeeded','revision':'publication'}}
        pending={'collection':'legacy-b','publication':{'token':'publication'},'freshness':{
            'state':'pending','live_source_verified':False}}
        idle=json.loads(json.dumps(pending));idle['freshness'].update(state='idle')
        with tempfile.TemporaryDirectory() as temporary:
            root=Path(temporary).resolve();binary=root/'old.exe';binary.write_bytes(b'inert unit bytes')
            fixture=object.__new__(h.Fixture);fixture.old_seal={'files':{'airk.exe':h.sha(binary)}}
            fixture.control=root/'control';fixture.source_jobs={'b':job};fixture.old_read_quiescence=[];fixture.commands=[]
            values=iter([pending,idle]);calls=[];deadlines=[]
            def command(_binary,*arguments,**options):
                calls.append(arguments);self.assertEqual(arguments[-4:-2],('call','airk_status'))
                fixture.commands.append(arguments)
                return SimpleNamespace(returncode=0,stdout=json.dumps({'structuredContent':next(values)}))
            fixture.command=command
            class Clients:
                @staticmethod
                def remaining(until,ceiling):deadlines.append(until);return ceiling
                @staticmethod
                def read_envelope(read,until):return read(until),[]
                @staticmethod
                def tool_value(envelope):return envelope['structuredContent']
                @staticmethod
                def read_retry_delay(_value):return None
            fixture.clients=Clients()
            with patch.object(h.time,'sleep') as sleep:
                fixture.wait_old_read_quiescence(binary,project,999.0)
            self.assertEqual(len(calls),2);self.assertEqual(set(deadlines),{999.0})
            self.assertEqual(fixture.old_read_quiescence[0]['metadata_polls'],2)
            sleep.assert_called_once_with(.1)

if __name__=='__main__':
    unittest.main()
