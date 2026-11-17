# SPDX-License-Identifier: Apache-2.0
import ast
import unittest
import admission as a

class LiteralAdmissionControls(unittest.TestCase):
    def test_native_yes_no_targets_literal(self):
        self.assertEqual(a.supervised_target(0),2152)
        self.assertEqual(a.supervised_target(1),9693)
        with self.assertRaises(ValueError):a.supervised_target(-1)
    def test_response_loss_masks_all_prompt_tokens(self):
        y=a.decision_labels(1)
        self.assertEqual(len(y),513)
        self.assertEqual(y[:-1],[-100]*512)
        self.assertEqual(y[-1],9693)
    def test_alpha_rename_copies_match(self):
        one=ast.parse('def foo(x):\n    y=x+7\n    return y*external(x)\n').body[0]
        two=ast.parse('def renamed(arg):\n    result=arg+7\n    return result*external(arg)\n').body[0]
        self.assertEqual(a.node_hash(a.normalized_node(one)),a.node_hash(a.normalized_node(two)))
    def test_global_call_and_constants_are_not_erased(self):
        base=ast.parse('def f(x):\n    y=x+7\n    return external(y)\n').body[0]
        for changed in ['def f(x):\n    y=x+8\n    return external(y)\n','def f(x):\n    y=x+7\n    return unrelated(y)\n']:
            self.assertNotEqual(a.node_hash(a.normalized_node(base)),a.node_hash(a.normalized_node(ast.parse(changed).body[0])))
    def test_substantive_partial_copies_co_locate(self):
        body='    y=[external(v) for v in x if accept(v) and ready(v)]\n    z=[other(v) for v in y if allowed(v)]\n'
        one='def first(x):\n'+body+'    return z\n'
        two='def second(x):\n    unrelated()\n'+body+'    return None\n'
        shared=a.fingerprints(one)&a.fingerprints(two)
        self.assertTrue(any(k=='alpha_partial_2to5_statements_min40nodes'for k,_ in shared))
    def test_parse_error_is_not_independence(self):
        with self.assertRaises(SyntaxError):a.fingerprints('def broken(:')
    def test_family_union_is_order_independent(self):
        one=a.DSU(['a','b','c','d']);two=a.DSU(['a','b','c','d'])
        one.union('b','c');one.union('a','b');two.union('a','b');two.union('c','b')
        self.assertEqual(one.components(),two.components())
        self.assertEqual(one.components()['a'],'a|b|c')
        self.assertEqual(one.components()['d'],'d')
    def test_roles_are_component_only_and_stable_under_input_permutation(self):
        components=[str(i)for i in range(20)]
        self.assertEqual(a.roles_for(components,{'3'},{'4'}),a.roles_for(list(reversed(components)),{'3'},{'4'}))
    def test_eval_only_and_parse_unknown_cannot_train_or_tune(self):
        roles=a.roles_for(['a','b','c','d','e','f'],{'a'},{'b'})
        self.assertEqual(roles['a'],'RESEARCH_TEST_RESERVED')
        self.assertEqual(roles['b'],'QUARANTINE_SOURCE_PARSE_OR_EXPOSURE')
        self.assertNotIn(roles['a'],['TRAIN','DEV'])
    def test_original_scope_unknown_refusals_not_negative(self):
        good={'rights_review':'PERMITTED_PRIVATE_RESEARCH_SCOPE','native_visibility':'accepted','label':1,'upstream_role':'publisher_TRAIN'}
        a.assert_pair_scope(good)
        for k,v in [('rights_review','UNKNOWN'),('native_visibility','refused'),('label',-1),('upstream_role','FINAL'),('unjudged_cross_pair',True)]:
            with self.assertRaises(ValueError):a.assert_pair_scope(dict(good,**{k:v}))
    def compiled(self):
        return {'candidate_positions':[2152,9693],'option_mask':[1,1],'decision_position':511,'input_ids':[151643]*510+[12,13],'attention_mask':[0]*510+[1,1],'position_ids':[0]*510+[0,1],'input_digest':'ab'*32}
    def test_exact_512_emitted_tensor_inspection(self):
        self.assertEqual(a.assert_compiled(self.compiled()),2)
    def test_no_implicit_crop_or_wrong_response_token(self):
        for k,v in [('input_ids',[1]*511),('candidate_positions',[9693,2152]),('decision_position',510),('attention_mask',[1,0]+[1]*510)]:
            c=self.compiled();c[k]=v
            with self.assertRaises(ValueError):a.assert_compiled(c)
    def test_four_signs_cannot_claim_alpha_point05(self):
        p=a.sign_bound(4)
        self.assertIsNone(p['positive_sign_threshold'])
        self.assertEqual(p['conditional_power_Pwin_0_8'],0)
    def test_six_signs_all_positive_has_exact_two_sided_bound(self):
        p=a.sign_bound(6)
        self.assertEqual(p['positive_sign_threshold'],6)
        self.assertEqual(p['two_sided_alpha'],.03125)
    def test_diagnostic_metrics_literal_ties_not_ranking(self):
        m=a.metrics([1,0,1,0],[3.,1.,1.,-1.])
        self.assertEqual(m['pointwise_auroc'],.875)
        self.assertEqual(m['balanced_accuracy_at_fixed_zero'],.75)
        self.assertFalse(m['ranking_or_terminal_quality_claim'])
    def test_nonfinite_unknown_and_missing_scores_refuse(self):
        for labels,scores in [([1,0],[float('nan'),0]),([1,-1],[1,0]),([1,0],[1])]:
            with self.assertRaises(ValueError):a.metrics(labels,scores)
    def test_card_missing_execution_native_replay_or_probe_refuses(self):
        c={'root_review_accepted':True,'instruction_selection':{'native_F1_compilation':'native','DEV_choice_receipt':'dev'},'general_document_replay':{'fraction':.1,'accepted_native_roster':'native-replay'},'training':{'measured_probe_receipt':'probe'},'promotion':False}
        a.require_execution_bindings(c)
        for section,key,value in [(None,'root_review_accepted',False),('instruction_selection','native_F1_compilation',None),('instruction_selection','DEV_choice_receipt',None),('general_document_replay','fraction',None),('general_document_replay','accepted_native_roster',None),('training','measured_probe_receipt',None),(None,'promotion',True)]:
            import copy
            bad=copy.deepcopy(c)
            if section:bad[section][key]=value
            else:bad[key]=value
            with self.assertRaises(ValueError):a.require_execution_bindings(bad)

if __name__=='__main__':unittest.main(verbosity=2)
