# SPDX-License-Identifier: Apache-2.0
"""Literal external initializer value controls; no learned values or neural imports."""
import unittest
from types import SimpleNamespace
from gate_protocol import install_external_value_guard,infer_complete

class ExternalValues(unittest.TestCase):
    def engine(self):
        e=SimpleNamespace(initializers_={'scorer.text.layers.0.input_layernorm.weight':object(),'shape_literal':object()},
            known_vi_={'scorer.text.layers.0.input_layernorm.weight':(1,[2048]),'shape_literal':(7,[1])},reads=[])
        def get(node,i):
            name=node.input[i];e.reads.append(name)
            if name=='scorer.text.layers.0.input_layernorm.weight':raise FileNotFoundError('model.onnx.data absent in fresh shape CWD')
            return [17]
        def optional(node,i):
            if i>=len(node.input):return None
            if node.input[i] in e.initializers_:return e._get_value(node,i)
            return None
        e._get_value=get;e._try_get_value=optional
        return e
    def test_original_optional_lookup_attempts_missing_external_file(self):
        e=self.engine();node=SimpleNamespace(name='/scorer/layers.0/input_layernorm/Mul',input=['activation','scorer.text.layers.0.input_layernorm.weight'])
        with self.assertRaises(FileNotFoundError):e._try_get_value(node,1)
    def test_optional_unknown_preserves_true_shapes_and_skips_value_compute(self):
        e=self.engine();original=e.known_vi_;policy=install_external_value_guard(e,{'scorer.text.layers.0.input_layernorm.weight'})
        node=SimpleNamespace(name='/scorer/layers.0/input_layernorm/Mul',input=['activation','scorer.text.layers.0.input_layernorm.weight'])
        values=[e._try_get_value(node,i) for i in range(2)]
        self.assertEqual(values,[None,None]);self.assertFalse(all(v is not None for v in values))
        self.assertEqual(e.reads,[]);self.assertIs(e.known_vi_,original)
        self.assertEqual(e.known_vi_['scorer.text.layers.0.input_layernorm.weight'],(1,[2048]))
        self.assertEqual(policy,{'external_optional_values_unknown':1,'external_required_values_refused':0})
    def test_required_external_expand_value_refuses_before_read(self):
        e=self.engine();policy=install_external_value_guard(e,{'scorer.text.layers.0.input_layernorm.weight'})
        node=SimpleNamespace(name='Expand',input=['data','scorer.text.layers.0.input_layernorm.weight'])
        with self.assertRaisesRegex(ValueError,'requires unavailable external weight VALUE'):e._get_value(node,1)
        self.assertEqual(e.reads,[]);self.assertEqual(policy['external_required_values_refused'],1)
    def test_inline_shape_values_and_missing_optional_inputs_are_unchanged(self):
        e=self.engine();install_external_value_guard(e,{'scorer.text.layers.0.input_layernorm.weight'})
        node=SimpleNamespace(name='Reshape',input=['data','shape_literal'])
        self.assertEqual(e._try_get_value(node,1),[17]);self.assertEqual(e._get_value(node,1),[17])
        self.assertIsNone(e._try_get_value(node,2));self.assertEqual(e.reads,['shape_literal','shape_literal'])
    def test_external_roster_requires_retained_shape_metadata(self):
        e=self.engine();del e.known_vi_['scorer.text.layers.0.input_layernorm.weight']
        with self.assertRaises(ValueError):install_external_value_guard(e,{'scorer.text.layers.0.input_layernorm.weight'})
    def test_unresolved_global_shape_still_refuses(self):
        e=self.engine();install_external_value_guard(e,{'scorer.text.layers.0.input_layernorm.weight'})
        e.run_=True;e.out_mp_='inference copy'
        def step():e.run_=False;return False
        e._infer_impl=step;e._update_output_from_vi=lambda:None
        with self.assertRaisesRegex(ValueError,'Incomplete'):infer_complete(e)
if __name__=='__main__':unittest.main()