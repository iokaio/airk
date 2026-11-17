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


if __name__=='__main__':
    unittest.main()
