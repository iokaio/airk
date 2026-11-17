# SPDX-License-Identifier: Apache-2.0
"""Small private custody controls; no declared model asset is opened or copied."""
import hashlib, importlib.util, json, pathlib, unittest
HERE=pathlib.Path(__file__).parent
spec=importlib.util.spec_from_file_location('r154_flat_stage',HERE/'stage_assets.py')
stage=importlib.util.module_from_spec(spec); spec.loader.exec_module(stage)

class StagingCustodyControls(unittest.TestCase):
    @classmethod
    def setUpClass(cls):
        cls.root=HERE/'cheap-custody-fixture-r1'; cls.policy=stage.protected_child(cls.root)
        cls.source=cls.root/'source.bin'; cls.source.write_bytes(b'only-four-byte-custody-fixture')
        cls.pin={'name':'source.bin','source':str(cls.source),'sha256':hashlib.sha256(cls.source.read_bytes()).hexdigest(),
                 **{key:stage.file_pin(cls.source)[key] for key in ('file_id','bytes','mtime_ns')}}
    def test_01_initial_creation_is_protected_and_independent_copy_is_exact(self):
        self.assertTrue(self.policy['sddl'].endswith(f'D:P(A;OICI;FA;;;{stage.SID})'))
        result=stage.copy_asset(self.pin,self.root/'copied.bin')
        self.assertEqual(result['sha256'],self.pin['sha256'])
        self.assertNotEqual(result['destination_observation']['file_id'],self.pin['file_id'])
        self.assertEqual(result['destination_observation']['owner_sid'],stage.SID)
        self.assertEqual(result['source_before_and_after'],stage.file_pin(self.source))
    def test_02_existing_destination_refused(self):
        destination=self.root/'existing.bin'; destination.write_bytes(b'existing-preserved')
        with self.assertRaisesRegex(RuntimeError,'destination exists'): stage.copy_asset(self.pin,destination)
        self.assertEqual(destination.read_bytes(),b'existing-preserved')
    def test_03_wrong_source_preimage_refused_before_creation(self):
        bad={**self.pin,'file_id':self.pin['file_id']+1}; destination=self.root/'not-created.bin'
        with self.assertRaisesRegex(RuntimeError,'source preimage'): stage.copy_asset(bad,destination)
        self.assertFalse(destination.exists())
    def test_04_wrong_hash_retains_failed_copy_and_original(self):
        before=stage.file_pin(self.source); destination=self.root/'failed-hash-retained.bin'
        with self.assertRaisesRegex(RuntimeError,'source content differs'):
            stage.copy_asset({**self.pin,'sha256':'0'*64},destination)
        self.assertTrue(destination.exists()); self.assertEqual(stage.file_pin(self.source),before)
    def test_05_existing_private_root_refused_without_policy_change(self):
        before=stage.security(self.root)
        with self.assertRaisesRegex(RuntimeError,'create-new protected child failed'): stage.protected_child(self.root)
        self.assertEqual(stage.security(self.root),before)

if __name__=='__main__': unittest.main(verbosity=2)