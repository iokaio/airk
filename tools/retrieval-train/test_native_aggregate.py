# SPDX-License-Identifier: Apache-2.0
import copy
import json
import unittest
from unittest.mock import patch

from aggregate_native_retrieval import aggregate, digest, validate_legacy, FAMILIES, QUERY, LEGACY_BINARY


class AggregateTests(unittest.TestCase):
    def setUp(self):
        self.questions = {'questions': [{'id': str(i), 'source_root': '/paper'} for i in range(205)]}
        self.encoders = [{'family': name, 'model_json_sha256': 'a' * 64} for name in sorted(FAMILIES)]
        self.pins = {name: 'b' * 64 for name in ('question_input_sha256', 'encoder_input_sha256', 'source_manifest_sha256', 'source_roster_sha256')}
        self.pins['source_roster_count'] = 2772
        self.receipts = [{'schema': 'airk.native-retrieval-family/2', 'complete': True,
                          'evaluation': True, 'release_qualified': False, 'family': row['family'],
                          **self.pins, 'query': copy.deepcopy(QUERY), 'model_json_sha256': 'a' * 64,
                          'source_before_sha256': 'b' * 64, 'source_after_sha256': 'b' * 64,
                          'pipeline_digest': 'f' * 64,
                          'binary_sha256': 'c' * 64, 'runtime': {'version': '1.30.0'},
                          'collections': [{'source_root': '/paper', 'publication': {'token': 'real'}, 'pipeline_digest': 'f' * 64, 'model_json_sha256': 'a' * 64, 'indexed_source_roster_sha256': 'b' * 64, 'reused': False}],
                          'records': [{'id': str(i), 'source_root': '/paper', 'primary_vector': {'publication': {'token': 'real'}, 'hits': [], 'retained_unknown_field': i}, 'query_us': 1} for i in range(205)]}
                         for row in self.encoders]

    def run_aggregate(self):
        return aggregate(self.receipts, self.questions, self.encoders, self.pins)

    def test_full_raw_fields_and_legacy_provenance_remain_present(self):
        self.receipts[0]['legacy_provenance'] = {'scope': 'retrospective snapshot only', 'original_sha256': 'd' * 64}
        result = self.run_aggregate()
        self.assertEqual(result['families'], self.receipts)
        self.assertFalse(result['release_qualified'])

    def test_missing_duplicate_or_partial_family_refused(self):
        for mutation in ('missing', 'duplicate', 'incomplete', 'duplicate-question'):
            original = copy.deepcopy(self.receipts)
            if mutation == 'missing': self.receipts.pop()
            if mutation == 'duplicate': self.receipts[-1] = copy.deepcopy(self.receipts[0])
            if mutation == 'incomplete': self.receipts[0]['complete'] = False
            if mutation == 'duplicate-question': self.receipts[0]['records'][-1] = self.receipts[0]['records'][0]
            with self.subTest(mutation=mutation), self.assertRaises(ValueError): self.run_aggregate()
            self.receipts = original

    def test_changed_source_model_scope_or_query_refused(self):
        mutations = [('source_roster_sha256', 'e' * 64), ('source_manifest_sha256', 'e' * 64),
                     ('source_roster_count', 2773), ('model_json_sha256', 'e' * 64),
                     ('source_after_sha256', 'e' * 64), ('pipeline_digest', 'e' * 64),
                     ('binary_sha256', ''), ('runtime', {}), ('collections', [])]
        for key, value in mutations:
            original = copy.deepcopy(self.receipts)
            self.receipts[0][key] = value
            with self.subTest(key=key), self.assertRaises(ValueError): self.run_aggregate()
            self.receipts = original
        self.receipts[0]['query']['max_bytes'] = 4096
        with self.assertRaises(ValueError): self.run_aggregate()
        self.receipts[0]['query'] = copy.deepcopy(QUERY)
        self.receipts[0]['records'][0]['source_root'] = '/different'
        with self.assertRaises(ValueError): self.run_aggregate()

    def test_retrospective_binding_cannot_change_records_or_claim_original_preseal(self):
        receipt = next(row for row in self.receipts if row['family'] == 'trained-v2-u8')
        original = json.dumps({'complete': False, 'families': [copy.deepcopy(receipt)]}).encode()
        receipt['binary_sha256'] = LEGACY_BINARY
        receipt['legacy_provenance'] = {'kind': 'retrospective-unchanged-published-source-binding',
            'original_capture_sha256': digest(original), 'original_binary_sha256': LEGACY_BINARY,
            'original_nextest_run': '659c966a-e2fb-4da2-a6fb-cf2635727176',
            'original_campaign_complete': False, 'original_family_records': 205,
            'original_source_tree_prerun_seal': None, 'original_dirty_source_preseal': None,
            'binding_binary_sha256': 'a' * 64,
            'source_closure_timing': 'before/after THIS retrospective binding, not original execution'}
        with patch('aggregate_native_retrieval.LEGACY_CAPTURE', digest(original)):
            validate_legacy(receipt, original)
            receipt['records'][0]['primary_vector']['retained_unknown_field'] = 999
            with self.assertRaisesRegex(ValueError, 'raw records changed'):
                validate_legacy(receipt, original)
            receipt['records'][0]['primary_vector']['retained_unknown_field'] = 0
            receipt['legacy_provenance']['original_source_tree_prerun_seal'] = 'invented'
            with self.assertRaisesRegex(ValueError, 'invents original pre-run'):
                validate_legacy(receipt, original)


if __name__ == '__main__':
    unittest.main()
