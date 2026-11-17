# SPDX-License-Identifier: Apache-2.0
"""Cheap metadata/refusal controls. No native processes or models are launched."""
import json
from pathlib import Path
import unittest
from run_pure_emitter import run

HERE=Path(__file__).resolve().parent

class CurrentBindingsControls(unittest.TestCase):
    def test_actual_raw_roster_and_explicit_unknown_final_identities(self):
        packet=json.loads((HERE/'current-bindings.actual.json').read_text(encoding='utf-8'))
        expected={'focus':('arbiter-focus-qwen3-reranker-06b',4),
                  'compact':('arbiter-context-qwen3-17b',4),'large':('arbiter-context-qwen3-4b',16)}
        self.assertEqual(set(packet['current_raw_profiles']),set(expected))
        for name,(model,cores) in expected.items():
            row=packet['current_raw_profiles'][name]
            self.assertEqual(row['model'],model);self.assertEqual(row['cpu_fit']['intra_threads'],cores)
            self.assertTrue(row['numeric_raw_only']);self.assertIsNone(row['qualification'])
        self.assertEqual(packet['future_fitted_profiles'],{'compact':None,'large':None})
        self.assertIsNone(packet['selected_minilm']);self.assertIsNone(packet['matched_runtime_pair'])

    def test_r97_missing_synthetic_selector_refuses_before_native_launch(self):
        for mode in ('included','excluded','fault'):
            with self.subTest(mode=mode),self.assertRaisesRegex(ValueError,'R97 selector is absent'):
                run(HERE/'current-bindings.actual.json',mode)

    def test_unselected_minilm_refuses_before_native_launch(self):
        with self.assertRaisesRegex(ValueError,'selected MiniLM gate/identity missing'):
            run(HERE/'current-bindings.actual.json','vector')

if __name__=='__main__':unittest.main()
