# SPDX-License-Identifier: Apache-2.0
"""Exact-artifact campaign publication, with actual API observations injected."""
from copy import deepcopy
from contextlib import ExitStack
import json
import os
from pathlib import Path
import tempfile
import unittest
from unittest.mock import patch
from types import SimpleNamespace
from scripts.tests import test_campaign_release as fixtures

release=fixtures.release
COMMIT=fixtures.COMMIT
VERSION='1.8.35'
TREE='b'*40

class RetainedPublication(unittest.TestCase):
    def test_full_bundle_handoff_verifies_both_copies_and_live_origin(self):
        candidates = release.producers
        origin = fixtures.Origin()
        with tempfile.TemporaryDirectory() as temporary, patch.object(candidates, 'api', side_effect=origin.api):
            producer = candidates.attest(123, COMMIT, allow_manual=True, version=VERSION, branch='feat/advanced-split')
            signed = candidates.attest_signed_release(456, COMMIT, branch='feat/advanced-split')
            expected = {'release.json': b'original release document', 'native.zip': b'original signed native bytes',
                        'portable.zip': b'original portable bytes', 'release-notes.md': b'original notes'}
            observations = []
            def downloaded(artifact, directory):
                self.assertEqual(artifact, signed['artifact'])
                directory.mkdir()
                for name, data in expected.items(): (directory / name).write_bytes(data)
            def verified(directory, commit, version, tag, run_id, attempt):
                self.assertEqual((commit, version, run_id, attempt), (COMMIT, VERSION, signed["run_id"], signed["attempt"]))
                observations.append({p.name: p.read_bytes() for p in directory.iterdir()})
                return {'channel': 'stable', 'tree': TREE, 'producer': producer}
            fake = SimpleNamespace(identity=lambda version: 'v'+version, verify_bundle=verified)
            real_spec = candidates.importlib.util.spec_from_file_location
            real_module = candidates.importlib.util.module_from_spec
            sentinel = SimpleNamespace(loader=SimpleNamespace(exec_module=lambda module: None))
            def specification(name, path):
                return sentinel if name == 'signed_release' else real_spec(name, path)
            def module(spec):
                return fake if spec is sentinel else real_module(spec)
            destination = Path(temporary) / 'complete'
            with patch.object(candidates, 'download', side_effect=downloaded), patch.object(candidates.importlib.util, 'spec_from_file_location', side_effect=specification), patch.object(candidates.importlib.util, 'module_from_spec', side_effect=module):
                candidates.download_signed_release(signed, COMMIT, destination, VERSION, 'stable', tree=TREE, full_bundle=True)
            self.assertEqual(observations, [expected, expected])
            self.assertEqual({p.name: p.read_bytes() for p in destination.iterdir()}, expected)
            with patch.object(candidates, 'download') as refused:
                with self.assertRaisesRegex(ValueError, 'distinct'):
                    candidates.download_signed_release(signed, COMMIT, Path(temporary)/'invalid', VERSION, 'stable', full_bundle=True, published_only=True)
                refused.assert_not_called()

    def controls(self):
        origin=fixtures.Origin()
        current=deepcopy(fixtures.CASES['publisher']['run'])
        def api(path,paginate=False):
            if path=='actions/runs/789':return deepcopy(current)
            return origin.api(path,paginate)
        environment={**fixtures.CASES['context'],'GITHUB_RUN_ID':'789','GITHUB_RUN_ATTEMPT':'1'}
        return origin,current,api,environment
    def test_campaign_cannot_publish_fresh_or_caller_supplied_directory(self):
        with patch.object(release,'context',return_value=(COMMIT,VERSION,'v'+VERSION)),patch.dict(os.environ,{'GITHUB_REF':'refs/heads/feat/advanced-split'}),patch.object(release,'verify_bundle') as verify,patch.object(release,'_publish_assets') as effects:
            with self.assertRaisesRegex(ValueError,'previously qualified'):
                release.publish(Path('unqualified'))
            with self.assertRaisesRegex(ValueError,'only from'):
                release.publish(Path('caller-supplied'),signed_producer={},expected_producer={},qualified_channel='stable')
            verify.assert_not_called();effects.assert_not_called()
    def exercise(self,change=None):
        origin,current,api,environment=self.controls();captured={}
        with ExitStack() as stack:
            stack.enter_context(patch.dict(os.environ,environment))
            stack.enter_context(patch.object(release,'context',return_value=(COMMIT,VERSION,'v'+VERSION)))
            stack.enter_context(patch.object(release.producers,'api',side_effect=api))
            stack.enter_context(patch.object(release.extension.package,'source_tree',return_value=TREE))
            producer=release.producers.attest(123,COMMIT,allow_manual=True,version=VERSION,branch='feat/advanced-split')
            signed=release.producers.attest_signed_release(456,COMMIT,branch='feat/advanced-split')
            record={'producer':producer,'channel':'stable','tree':TREE}
            original={'release.json':json.dumps(record).encode(),'native.zip':b'qualified bytes, not rebuilt','release-notes.md':b'original notes'}
            def downloaded(observed,commit,destination,version,channel,**kwargs):
                self.assertEqual(observed,signed);self.assertEqual((commit,version,channel),(COMMIT,VERSION,'stable'))
                self.assertEqual(kwargs,{'tree':TREE,'full_bundle':True})
                destination.mkdir()
                for name,data in original.items():(destination/name).write_bytes(data)
                if change=='cancel':current['status']='completed';current['conclusion']='cancelled'
                if change=='artifact_changed':origin.bundle['digest']='sha256:'+'f'*64
            download=stack.enter_context(patch.object(release.producers,'download_signed_release',side_effect=downloaded))
            verify=stack.enter_context(patch.object(release,'verify_bundle',return_value=record))
            def published(directory,actual,tag,commit,resume):
                captured.update(files={path.name:path.read_bytes() for path in directory.iterdir()},record=actual,
                                tag=tag,commit=commit,resume=resume,path=str(directory))
            effects=stack.enter_context(patch.object(release,'_publish_assets',side_effect=published))
            if change=='nomination':signed['artifact']['digest']='sha256:'+'0'*64
            if change=='producer':producer=deepcopy(producer);producer['workflow_id']=4.0
            if change is not None:
                with self.assertRaises(ValueError):
                    release.publish(None,resume=True,signed_producer=signed,expected_producer=producer,qualified_channel='stable')
                effects.assert_not_called()
                if change in ('nomination','producer'):download.assert_not_called()
            else:
                release.publish(None,resume=True,signed_producer=signed,expected_producer=producer,qualified_channel='stable')
                self.assertEqual(captured['files'],original)
                self.assertEqual((captured['tag'],captured['commit'],captured['resume']),('v'+VERSION,COMMIT,True))
                self.assertFalse(Path(captured['path']).exists())
                self.assertEqual(verify.call_args.args[4:],('456','3'))
            return captured
    def test_publishes_original_owned_bundle_using_old_assembly_identity(self):
        self.exercise()
    def test_mutated_signed_nomination_and_malformed_ci_source_refuse_before_download(self):
        for fault in ('nomination','producer'):
            with self.subTest(fault=fault):self.exercise(fault)
    def test_cancelled_actual_publisher_or_changed_signed_artifact_refuses_after_download(self):
        for fault in ('cancel','artifact_changed'):
            with self.subTest(fault=fault):self.exercise(fault)
    def test_workflow_reuses_signed_artifact_and_keeps_main_fresh_route(self):
        source=(fixtures.ROOT/'.github/workflows/github-release.yml').read_text(encoding='utf-8')
        self.assertIn('signed_run_id:',source)
        for job in ('sign','package-linux','assemble'):
            block=source.split('\n  '+job+':\n',1)[1].split('runs-on:',1)[0]
            self.assertIn("inputs.signed_run_id == ''",block)
        qualified=source.split('\n  publish_qualified:\n',1)[1].split('\n  downloads:',1)[0]
        self.assertIn('--signed-producer artifacts/signed-producer.json',qualified)
        self.assertNotIn('package-native.py',qualified);self.assertNotIn('sign-native.py',qualified)
        self.assertNotIn('--directory',qualified)
        self.assertIn('needs: [publish, publish_qualified]',source)

if __name__=='__main__':unittest.main()
