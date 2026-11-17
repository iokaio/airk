# SPDX-License-Identifier: Apache-2.0
"""Pure exact-source custody controls: no filesystem fixtures, Git or deletion."""
import copy
import hashlib
import json
import unittest
import cleanup as owner

SOURCE = 'artifacts/reviewed/source.py'
MAP = dict(path='scripts/research/source-map.json', sha256='a'*64, bytes=71)
ORIGINAL = b'print(1)\r\n'
PREFIX = b'# SPDX-License-Identifier: Apache-2.0\n'


def sha(raw):
    return hashlib.sha256(raw).hexdigest()


def encoded(value):
    return json.dumps(value, sort_keys=True).encode()


def pin(path, raw):
    return dict(path=path, sha256=sha(raw), bytes=len(raw))


def inputs(prefixed=True):
    kept = PREFIX + ORIGINAL if prefixed else ORIGINAL
    row = dict(source=SOURCE, destination='scripts/research/source.py.in', source_sha256=sha(ORIGINAL), source_bytes=len(ORIGINAL))
    mapped = dict(source=SOURCE, destination=row['destination'], original_sha256=sha(ORIGINAL), original_bytes=len(ORIGINAL), maintained_sha256=sha(kept), maintained_bytes=len(kept))
    observation = dict(source=SOURCE, sha256=sha(ORIGINAL), bytes=len(ORIGINAL), owner_sid=owner.SID, effective_sid=owner.SID, default_owner_sid=owner.SID, policy_passed=True, file_sddl='O:humanD:private', parents=[dict(path='artifacts/reviewed', owner_sid=owner.SID, sddl='O:humanD:private')])
    review = dict(source=SOURCE, role='retired-authored-source', status='clear', active_consumers=[], consumer_searches=['explicit callers'], verification_transition='permanent inverse plus explicit custody audit', evidence=[MAP])
    return row, dict(files=[mapped]), kept, observation, review


def proof_documents(prefixed=True):
    row, _, kept, observation, review = inputs(prefixed)
    expected = dict(source=SOURCE, source_sha256=sha(ORIGINAL), source_bytes=len(ORIGINAL), source_map=MAP)
    inverse = dict(kind='spdx-prefix' if prefixed else 'identity', prefix_utf8=PREFIX.decode() if prefixed else '')
    replacement = dict(path=row['destination'], commit='b'*40, sha256=sha(kept), bytes=len(kept), git_blob_oid=hashlib.sha1(b'blob '+str(len(kept)).encode()+b'\0'+kept).hexdigest())
    ready = dict(row, source_map=MAP, eligibility='ready', inverse=inverse, replacement=replacement, observation=observation, consumer_review=review)
    pre = dict(schema='airk.source-cleanup-pre-removal/1', human_sid=owner.SID, plan=dict(files=[ready]))
    result = dict(schema='airk.source-cleanup-result/1', source=SOURCE, source_sha256=sha(ORIGINAL), source_bytes=len(ORIGINAL), removed=True, error=None)
    return expected, kept, pre, result


def bundle(kept, pre, result):
    pre_raw = encoded(pre)
    result = dict(result, pre_removal=pin('artifacts/audit/pre.json', pre_raw))
    result_raw = encoded(result)
    return dict(result_raw=result_raw, result_pin=pin('artifacts/audit/result.json', result_raw), pre_removal_raw=pre_raw, replacement_raw=kept)


class CleanupControls(unittest.TestCase):
    def test_exact_identity_and_spdx_candidates(self):
        for prefixed, expected_kind in ((False, 'identity'), (True, 'spdx-prefix')):
            result = owner.candidate(*inputs(prefixed))
            self.assertEqual(result['inverse']['kind'], expected_kind)
            self.assertEqual((result['source_sha256'], result['source_bytes']), (sha(ORIGINAL), len(ORIGINAL)))

    def test_unknown_or_ambiguous_inverse_refuses(self):
        for raw in (b'changed', b'// not a permitted prefix\n'+ORIGINAL):
            with self.assertRaises(ValueError): owner.inverse(raw, sha(ORIGINAL), len(ORIGINAL))

    def test_source_scope_refuses_escape_data_and_metadata(self):
        for value in ('../x.py', 'artifacts/../x.py', 'C:/x.py', 'artifacts\\x.py', 'scripts/x.py', 'artifacts/data.json', 'artifacts/model.safetensors'):
            with self.assertRaises(ValueError): owner.source_path(value)

    def test_candidate_requires_all_human_identities(self):
        for key in ('owner_sid', 'effective_sid', 'default_owner_sid'):
            args = list(inputs()); args[3][key] = 'foreign'
            with self.subTest(key=key), self.assertRaises(ValueError): owner.candidate(*args)

    def test_candidate_requires_policy_and_explicit_consumer_transition(self):
        for key, value in (('policy_passed', False), ('parents', []), ('file_sddl', '')):
            args = list(inputs()); args[3][key] = value
            with self.subTest(key=key), self.assertRaises(ValueError): owner.candidate(*args)
        for key, value in (('role', 'private-fixture'), ('status', 'pending'), ('active_consumers', ['sealed-helper']), ('consumer_searches', []), ('verification_transition', '')):
            args = list(inputs()); args[4][key] = value
            with self.subTest(key=key), self.assertRaises(ValueError): owner.candidate(*args)

    def test_each_parent_retains_human_policy_identity(self):
        for key, value in (('owner_sid', 'foreign'), ('path', ''), ('sddl', '')):
            args = list(inputs()); args[3]['parents'][0][key] = value
            with self.subTest(candidate_parent=key), self.assertRaises(ValueError): owner.candidate(*args)
            expected, kept, pre, result = proof_documents()
            pre['plan']['files'][0]['observation']['parents'][0][key] = value
            with self.subTest(custody_parent=key), self.assertRaises(ValueError): owner.verify_custody(expected, None, bundle(kept, pre, result))


    def test_candidate_rejects_duplicate_map_and_changed_replacement(self):
        args = list(inputs()); args[1]['files'] *= 2
        with self.assertRaises(ValueError): owner.candidate(*args)
        args = list(inputs()); args[2] += b'changed'
        with self.assertRaises(ValueError): owner.candidate(*args)

    def test_present_original_is_exact_and_missing_requires_proof(self):
        expected, _, _, _ = proof_documents()
        self.assertEqual(owner.verify_custody(expected, ORIGINAL), 'present-original')
        with self.assertRaises(ValueError): owner.verify_custody(expected, b'changed')
        with self.assertRaises(ValueError): owner.verify_custody(expected, None)

    def test_missing_original_accepts_bound_identity_and_prefix_proof(self):
        for prefixed in (False, True):
            expected, kept, pre, result = proof_documents(prefixed)
            self.assertEqual(owner.verify_custody(expected, None, bundle(kept, pre, result)), 'proved-removed-original')

    def test_false_foreign_or_error_removal_result_refuses(self):
        for key, value in (('removed', False), ('error', 'interrupted'), ('source', 'artifacts/other.py'), ('source_sha256', '0'*64), ('schema', 'invented')):
            expected, kept, pre, result = proof_documents(); result[key] = value
            with self.subTest(key=key), self.assertRaises(ValueError): owner.verify_custody(expected, None, bundle(kept, pre, result))

    def test_changed_pre_or_receipt_bytes_refuse(self):
        for key in ('pre_removal_raw', 'result_raw'):
            expected, kept, pre, result = proof_documents(); proof = bundle(kept, pre, result); proof[key] += b' '
            with self.subTest(key=key), self.assertRaises(ValueError): owner.verify_custody(expected, None, proof)

    def test_pre_source_map_and_inverse_binding_refuse(self):
        for field, value in (('source_map', dict(MAP, sha256='f'*64)), ('inverse', dict(kind='identity', prefix_utf8='')), ('eligibility', 'deferred')):
            expected, kept, pre, result = proof_documents(); pre['plan']['files'][0][field] = value
            with self.subTest(field=field), self.assertRaises(ValueError): owner.verify_custody(expected, None, bundle(kept, pre, result))
        expected, kept, pre, result = proof_documents(); pre['human_sid'] = 'foreign'
        with self.assertRaises(ValueError): owner.verify_custody(expected, None, bundle(kept, pre, result))

    def test_replacement_and_raw_git_identity_refuse_drift(self):
        expected, kept, pre, result = proof_documents(); proof = bundle(kept, pre, result); proof['replacement_raw'] += b'changed'
        with self.assertRaises(ValueError): owner.verify_custody(expected, None, proof)
        expected, kept, pre, result = proof_documents(); pre['plan']['files'][0]['replacement']['git_blob_oid'] = '0'*40
        with self.assertRaises(ValueError): owner.verify_custody(expected, None, bundle(kept, pre, result))

    def test_pre_observation_preserves_full_identity_and_policy(self):
        for key, value in (('source', 'artifacts/other.py'), ('owner_sid', 'foreign'), ('effective_sid', 'foreign'), ('default_owner_sid', 'foreign'), ('policy_passed', False), ('parents', []), ('file_sddl', '')):
            expected, kept, pre, result = proof_documents(); pre['plan']['files'][0]['observation'][key] = value
            with self.subTest(key=key), self.assertRaises(ValueError): owner.verify_custody(expected, None, bundle(kept, pre, result))


if __name__ == '__main__':
    unittest.main(verbosity=2)
