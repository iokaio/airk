# SPDX-License-Identifier: Apache-2.0
"""Pure argv/source-binding controls; no real artifacts, processes or outputs."""
import copy
import importlib.util
from pathlib import Path
import unittest
B=Path(__file__).parent
s=importlib.util.spec_from_file_location('focus_card_preparer',B/'prepare_focus_card.py')
M=importlib.util.module_from_spec(s);s.loader.exec_module(M)

class CardPreparationTests(unittest.TestCase):
    def receipt(self):
        return {'schema':'airk-readiness-command/1','status':'completed','passed':True,'exit_code':0,'runner_exit_code':0,
            'inputs_unchanged':True,'runner_unchanged':True,'runtime_unchanged':True,'command_inputs_unchanged':True,
            'changed_groups':[],'groups':{'native':{'sha256':'b'*64}},
            'command':['python','scripts/build-service-runtime.py','--destination',str(B/'fictional-fresh-pair'),'--profile','debug']}

    def test_actual_builder_receipt_and_destination_bind_without_running(self):
        M.successful_build(self.receipt(),'b'*64,B/'fictional-fresh-pair')

    def test_started_failed_changed_and_foreign_source_builds_refuse(self):
        for key,value in [('status','started'),('passed',False),('exit_code',1),('runner_exit_code',1),
            ('inputs_unchanged',False),('runner_unchanged',False),('runtime_unchanged',False),
            ('command_inputs_unchanged',False),('changed_groups',['native'])]:
            with self.subTest(key=key):
                receipt=self.receipt();receipt[key]=value
                with self.assertRaises(ValueError):M.successful_build(receipt,'b'*64,B/'fictional-fresh-pair')
        with self.assertRaises(ValueError):M.successful_build(self.receipt(),'c'*64,B/'fictional-fresh-pair')

    def test_wrong_destination_profile_and_cross_target_refuse(self):
        with self.assertRaises(ValueError):M.successful_build(self.receipt(),'b'*64,B/'different-pair')
        receipt=self.receipt();receipt['command'][-1]='release'
        with self.assertRaises(ValueError):M.successful_build(receipt,'b'*64,B/'fictional-fresh-pair')
        receipt=self.receipt();receipt['command']+=['--target','x86_64-pc-windows-msvc']
        with self.assertRaises(ValueError):M.successful_build(receipt,'b'*64,B/'fictional-fresh-pair')

    def test_child_argv_uses_full_source_not_native_group_and_exact_typed_producer(self):
        argv=M.child_argv(B,B/'fresh',B/'B2-Service.exe','a'*64,B/'fixture',B/'focus',B/'output')
        self.assertEqual(argv[argv.index('--source-sha256')+1],'a'*64)
        self.assertNotIn('b'*64,argv)
        self.assertEqual(argv[argv.index('--service-test-sha256')+1],M.SERVICE_SHA)
        self.assertEqual(argv[argv.index('--whole-phase-seconds')+1],'1800')
        self.assertEqual(argv[argv.index('--preparation-sha256')+1],M.RECIPE_SHA)
        self.assertIn('--consent-focus-native-journey',argv)

if __name__=='__main__':unittest.main()