# SPDX-License-Identifier: Apache-2.0
import math
import unittest
from focus_reference import relevance_score, rank_complete_pool

class FocusReferenceControls(unittest.TestCase):
    def test_independent_binary_logits_and_stability(self):
        self.assertEqual(relevance_score(0, 0), .5)
        self.assertAlmostEqual(relevance_score(0, math.log(3)), .75)
        self.assertAlmostEqual(relevance_score(math.log(3), 0), .25)
        self.assertEqual(relevance_score(-1000, 1000), 1)
        self.assertEqual(relevance_score(1000, -1000), 0)
        self.assertAlmostEqual(relevance_score(500, 501), relevance_score(0, 1))
    def test_complete_pool_retains_original_ids_and_stable_ties(self):
        rows = rank_complete_pool(['opaque-original-a', 'opaque-original-b', 'opaque-original-c'], [[0, 0], [0, math.log(3)], [0, 0]])
        self.assertEqual([row['candidate_id'] for row in rows], ['opaque-original-b', 'opaque-original-a', 'opaque-original-c'])
        self.assertEqual([row['native_rank'] for row in rows], [1, 0, 2])
        self.assertEqual([row['relevance_score'] for row in rows], [.75, .5, .5])
    def test_nonfinite_or_missing_candidate_aborts_condition_without_partial_ranking(self):
        for value in [math.nan, math.inf, -math.inf, True, None, 'not-a-number']:
            with self.subTest(value=value), self.assertRaises(ValueError):
                rank_complete_pool(['first', 'second'], [[0, 1], [value, 0]])
        for ids, logits in [([], []), (['a'], []), (['a', 'a'], [[0, 0], [0, 1]]), (['a'], [[0, 1, 2]]), ([str(i) for i in range(65)], [[0, 0]] * 65)]:
            with self.subTest(ids=ids), self.assertRaises(ValueError):
                rank_complete_pool(ids, logits)

if __name__ == '__main__':
    unittest.main(verbosity=2)
