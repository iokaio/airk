# SPDX-License-Identifier: Apache-2.0
import copy
import json
import unittest

from inputs import CAMPAIGN, CONTROL, CONTROL_SHA, checked, ref, selected
from panel import numeric_identity, reference_identity
from run import verify_result


class PanelIdentity(unittest.TestCase):
    def setUp(self):
        execution = ref(CAMPAIGN / "cal100-compact-protected-matching-r4-t1408-r1/execution.json",
                        "c47c23956e612005e1c49af85c9db707098b0ef904843952eff10bb9fba3ee70")
        self.result = json.loads(checked(execution))["result"]

    def test_real_cpu_receipt_preserves_publisher_and_image_identities(self):
        reference_identity(self.result)
        identity = numeric_identity()
        self.assertEqual(identity["native_build"]["image"]["sha256"],
                         "b3d20acfea363da2afd1780078e07f46f4c3bcd52da5515aa4ed29c0cc192351")

    def test_original_seven_card_cpu_result_cannot_enter_r5(self):
        rows, mode = selected(ref(CONTROL,CONTROL_SHA),0,1)
        verify_result(self.result,rows,mode,"protected_matching")
        with self.assertRaises(ValueError):
            verify_result(self.result,rows,"r5_original","protected_matching")

    def test_rejects_substituted_publisher_file(self):
        altered = copy.deepcopy(self.result)
        altered["publisher_files"]["config.json"]["sha256"] = "0" * 64
        with self.assertRaises(ValueError):
            reference_identity(altered)
