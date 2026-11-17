# SPDX-License-Identifier: Apache-2.0
"""Pure positive-build admission controls; no native/process/artifact payload read."""
from pathlib import Path
from copy import deepcopy
import importlib.util,json,unittest
R=Path(r'D:/code/Github/airk');O=R/'artifacts/advanced-split/models/actual-composed-service-r2/final-plan-current-policy-preparation-20261116T085632Z/current-orchestration';E=R/'artifacts/v190/final-plan/exec-20261116T052942Z'
s=importlib.util.spec_from_file_location('existing_focus_evidence',O/'public-focus-recipe-r6/public_focus_evidence_r6.py');A=importlib.util.module_from_spec(s);s.loader.exec_module(A)
SID='S-1-5-21-3678370870-3858642417-826968105-1002'
class HealthBuildControls(unittest.TestCase):
    def test_genuine_signed_r3_ui_unavailable_build_cannot_qualify_focus_health_source(self):
        health=json.loads((E/'ui-native-r3-cards/workbench-r3/managed-start.json').read_text())['health'];before=deepcopy(health)
        self.assertIsNone(health['build']);self.assertEqual(health['build_error'],'build_receipt_missing_or_unverified')
        with self.assertRaises(ValueError):A.require_health_source(health,SID,'a8d7983b0f8bfdb701c7659f3b7a424505359a0dbe52446eb10c31de0b1a3dfe')
        self.assertEqual(health,before)
    def test_genuine_original_payload_focus_health_build_remains_historical_positive(self):
        health=json.loads((O.parent/'public-focus-native-r5/001-actual-start.json').read_text());before=deepcopy(health)
        A.require_health_source(health,SID,'f4353a13aef8d5d908944fb643ff833f59af0f215cbd73bad79723b063ea8b1f')
        self.assertEqual(health,before)
        with self.assertRaises(ValueError):A.require_health_source(health,SID,'a8d7983b0f8bfdb701c7659f3b7a424505359a0dbe52446eb10c31de0b1a3dfe')
    def test_missing_null_malformed_wrong_source_and_native_refusal_remain_unavailable(self):
        literal={'user':SID,'build':{'source_sha256':'12'*32,'source_files':3,'profile':'debug','rustc':'literal fixture'},'build_error':None}
        for name in ('missing','null','scalar','empty','wrong_source','native_refusal','foreign_user'):
            value=deepcopy(literal)
            if name=='missing':del value['build']
            if name=='null':value['build']=None
            if name=='scalar':value['build']='unverified'
            if name=='empty':value['build']={}
            if name=='wrong_source':value['build']['source_sha256']='34'*32
            if name=='native_refusal':value['build_error']='build_receipt_missing_or_unverified'
            if name=='foreign_user':value['user']='foreign'
            before=deepcopy(value)
            with self.subTest(name=name),self.assertRaises(ValueError):A.require_health_source(value,SID,'12'*32)
            self.assertEqual(value,before)
    def test_original_payload_preservation_and_positive_gate_precede_any_optional_model_work(self):
        text=(O/'public-focus-recipe-r7/run_public_focus_r7.py').read_text()
        self.assertIn('preserve_pair_metadata=True',text)
        self.assertLess(text.index('assertions.require_health_source(first_health'),text.index("control('import'"))
        self.assertLess(text.index('assertions.require_health_source(first_health'),text.index("control('enable'"))
        self.assertLess(text.index('assertions.require_health_source(fixture.managed_health'),text.index("record('restarted-state'"))
