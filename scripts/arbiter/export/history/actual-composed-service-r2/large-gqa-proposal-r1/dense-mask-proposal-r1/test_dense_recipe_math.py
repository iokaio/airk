# SPDX-License-Identifier: Apache-2.0
"""Independent scalar math evidence only; never invokes graph/runtime/model math."""
import math,unittest
from fractions import Fraction

class DenseRecipeMath(unittest.TestCase):
    def test_explicit_causal_and_padding_enumerated_means_at17_and128(self):
        for tokens in (17,128):
            for query in range(tokens):
                causal=[key for key in range(tokens) if key<=query]
                padded=[key for key in causal if key!=1]
                self.assertTrue(padded)
                self.assertEqual(Fraction(sum(causal),len(causal)),Fraction(query,2))
                exact=Fraction(sum(padded),len(padded))
                closed=Fraction(0) if query<2 else Fraction(query*(query+1)//2-1,query)
                self.assertEqual(exact,closed)
                weights=[math.exp(-3.4028234663852886e38) if key>query or key==1 else 1. for key in range(tokens)]
                observed=sum(key*weight for key,weight in enumerate(weights))/sum(weights)
                self.assertAlmostEqual(observed,float(exact),places=12)
    def test_zero_bias_counterexample_refuses_implicit_causal1_at17_and128(self):
        for tokens in (17,128):
            full=Fraction(sum(range(tokens)),tokens)
            self.assertEqual(full,Fraction(tokens-1,2))
            self.assertGreater(float(full/8192),0.0001)
            self.assertNotEqual(full,Fraction(0))
    def test_all1through8192_causal_and_padding_shape_domain_without_nn(self):
        # Independent cumulative sums validate the scalar identities for every query
        # in the original bound. This is math, not native shape or NN execution proof.
        total=0
        for query in range(8192):
            total+=query
            self.assertEqual(Fraction(total,query+1),Fraction(query,2))
            padded=Fraction(total-(1 if query>=1 else 0),query+1-(1 if query>=1 else 0))
            self.assertEqual(padded,Fraction(0) if query<2 else Fraction(query*(query+1)//2-1,query))
    def test_literal_large_allocations_and_grouping_remain_distinct(self):
        self.assertEqual((4096+1024+1024)*17*4,417792)
        self.assertEqual((4096+1024+1024)*128*4,3145728)
        self.assertEqual((4096+1024+1024)*8192*4,201326592)
        self.assertEqual([head//4 for head in range(32)],[0,0,0,0,1,1,1,1,2,2,2,2,3,3,3,3,4,4,4,4,5,5,5,5,6,6,6,6,7,7,7,7])

if __name__=='__main__':unittest.main(verbosity=2)