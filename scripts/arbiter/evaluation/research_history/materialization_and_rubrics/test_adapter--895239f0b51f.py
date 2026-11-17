# SPDX-License-Identifier: Apache-2.0
"""Synthetic guards only: never open real FINAL gold or raw dataset content."""
import copy
import hashlib
import importlib.util
import json
from pathlib import Path
import tempfile
import unittest

SOURCE = Path(__file__).with_name('adapter.py')
SPEC = importlib.util.spec_from_file_location('opaque_final_adapter', SOURCE)
subject = importlib.util.module_from_spec(SPEC)
SPEC.loader.exec_module(subject)


def row(qid, quote=False, support=0):
    return {'id': qid, 'question': 'SYNTHETIC question ' + qid, 'answer': 'SYNTHETIC answer',
        'context': {'title': ['Synthetic article'], 'sentences': [['Alpha.', 'A "quote".' if quote else 'Beta.']]},
        'supporting_facts': {'title': ['Synthetic article'], 'sent_id': [support]}}


def qasper_question(qid, eligible=True):
    answer = {'unanswerable': False, 'evidence': ['Alpha.'], 'extractive_spans': ['Alpha'], 'yes_no': None, 'free_form_answer': ''}
    alternative = {'human_annotation_id': qid + ':a', 'unanswerable': False,
        'original_evidence_count': 1, 'evidence': [{'source_evidence_sha256': hashlib.sha256(b'Alpha.').hexdigest(),
            'allowed_locations': ['synthetic-paper:0:0'] if eligible else []}],
        'alignment': 'all_original_evidence_maps_to_allowed_text' if eligible else 'unsupported_or_excluded_evidence_not_admitted',
        'original_human_record': {'annotation_id': qid + ':a', 'answer': answer}}
    return {'question_id': qid, 'question': 'SYNTHETIC question', 'human_alternatives': [alternative],
        'disagreement': False, 'eligibility': 'eligible' if eligible else 'no_complete_original_alternative'}


def qasper_payload():
    return {'schema': 'airk.private-qasper-fresh-final-sealed/1', 'minimum_met': True,
        'selected_question_ids': ['historical-selection-is-not-authority'], 'attribution': 'SYNTHETIC attribution',
        'papers': [{'paper_id': 'synthetic-paper', 'decision': {'title': 'SYNTHETIC paper', 'license': 'CC-BY-4.0',
            'source_version_basis': 'SYNTHETIC fixture only'},
            'passages': [{'id': 'synthetic-paper:0:0', 'text': 'Alpha.'}],
            'questions': [qasper_question('a'), qasper_question('b'), qasper_question('c', False)]}]}


class SyntheticControls(unittest.TestCase):
    @classmethod
    def setUpClass(cls):
        # Actual pure owner source only, never its main()/raw corpus inputs.
        cls.neutral = subject.load_neutral((subject.ROOT / 'tools/retrieval-train/human_corpus.py').read_bytes())

    def test_existing_frozen_promotion_domain_and_dataset_qualified_order(self):
        expected = hashlib.sha256(b'airk.a1.next-experiment.v1/promotion-final\0hotpot:synthetic').digest()
        self.assertEqual(subject.rank('hotpot', 'synthetic'), (expected, b'hotpot:synthetic'))
        self.assertNotEqual(subject.rank('qasper', 'synthetic'), subject.rank('hotpot', 'synthetic'))

    def test_metadata_projection_skips_unneeded_nested_content_and_only_final_membership(self):
        components = [{'component_digest': '1' * 64, 'question_ids': ['final'], 'article_digests': ['2' * 64], 'disposition': 'FINAL_SEALED_UNOPENED'},
            {'component_digest': '3' * 64, 'question_ids': ['dev'], 'article_digests': ['4' * 64], 'disposition': 'DEV'}]
        raw = json.dumps({'unneeded': {'nested': ['synthetic ] escaped " data']},
            'schema': 'airk.private-a1-blind-metadata-audit/1', 'components': components}).encode()
        projected = subject.metadata_projection(raw)
        self.assertNotIn('unneeded', projected)
        self.assertEqual(set(subject.final_membership(projected, 1, 1)), {'final'})
        components[1]['question_ids'] = ['final']
        with self.assertRaises(ValueError):
            subject.final_membership({'schema': projected['schema'], 'components': components}, 1, 1)

    def test_exact_owner_preserves_original_support_answer_unicode_and_refuses_quote_support(self):
        original = row('x')
        original['context']['sentences'][0][0] = 'café original.'
        before = copy.deepcopy(original)
        task, docs, excluded, eligible = subject.convert_hotpot(original, {'article'}, lambda _: 'article', self.neutral)
        self.assertTrue(eligible)
        self.assertEqual(original, before)
        alternative = task['human_alternatives'][0]
        self.assertEqual(alternative['original_human_answer'], original['answer'])
        self.assertEqual(alternative['original_evidence_count'], 1)
        self.assertEqual(alternative['evidence'][0]['source_evidence_sha256'], hashlib.sha256('café original.'.encode()).hexdigest())
        self.assertEqual(next(iter(docs.values()))['text'], 'café original.Beta.')
        self.assertFalse(excluded)
        self.assertFalse(subject.convert_hotpot(row('quoted', True, 1), {'article'}, lambda _: 'article', self.neutral)[3])

    def test_fixed_rank_first_eligible_no_component_exclusion_or_model_refill(self):
        membership = {qid: ('same-component', {'article'}) for qid in ('a', 'b', 'c', 'd')}
        ordered = sorted(membership, key=lambda qid: subject.rank('hotpot', qid))
        values = {qid: row(qid, quote=(qid == ordered[0]), support=1) for qid in membership}
        def scan(wanted):
            return [values[qid] for qid in reversed(wanted)]
        result = subject.select_hotpot(membership, scan, lambda _: 'article', self.neutral, 2)
        self.assertEqual([task['question_id'] for task in result[1]], ordered[1:3])
        self.assertEqual(result[3], {'projected': 4, 'validated': 4, 'eligible_projected': 3,
            'mechanically_refused_projected': 1, 'considered_for_selection': 3,
            'selected': 2, 'validated_unselected_tail': 1})
        self.assertFalse(result[2][-1]['selected'])
        self.assertFalse(result[2][-1]['considered_for_selection'])
        for value in values.values():
            value['model_score'] = -9999
        self.assertEqual([task['question_id'] for task in subject.select_hotpot(membership, scan, lambda _: 'article', self.neutral, 2)[1]], ordered[1:3])

    def test_entire_projected_tail_is_validated_audited_and_never_enters_selected_documents(self):
        membership = {qid: ('component', {'article'}) for qid in ('a', 'b', 'c')}
        ordered = sorted(membership, key=lambda qid: subject.rank('hotpot', qid))
        values = {qid: row(qid) for qid in membership}
        values[ordered[-1]]['supporting_facts']['title'].append('malformed projected tail')
        with self.assertRaisesRegex(ValueError, 'publisher_support_cardinality'):
            subject.select_hotpot(membership, lambda wanted: [values[qid] for qid in wanted], lambda _: 'article', self.neutral, 1)
        values[ordered[-1]] = row(ordered[-1])
        for number, qid in enumerate(ordered):
            values[qid]['context']['title'] = ['Synthetic article ' + str(number)]
            values[qid]['supporting_facts']['title'] = ['Synthetic article ' + str(number)]
        documents, tasks, audit, counts = subject.select_hotpot(membership,
            lambda wanted: [values[qid] for qid in reversed(wanted)], lambda _: 'article', self.neutral, 1)
        self.assertEqual([task['question_id'] for task in tasks], ordered[:1])
        self.assertEqual([item['selected'] for item in audit], [True, False, False])
        self.assertEqual([item['original_row'] for item in audit], [values[qid] for qid in ordered])
        self.assertEqual({document['title'] for document in documents.values()}, {'Synthetic article 0'})
        self.assertEqual(counts['validated'], 3)
        self.assertEqual(counts['validated_unselected_tail'], 2)

    def test_actual_arrow_predicate_returns_only_final_requested_ids_and_explicit_projection(self):
        import pyarrow
        import pyarrow.parquet as parquet
        pyarrow.set_cpu_count(1)
        pyarrow.set_io_thread_count(1)
        values = [row('FINAL-synthetic'), row('DEV-synthetic'), row('CAL-synthetic')]
        for value in values:
            value['must_not_leave_projection'] = 'SYNTHETIC excluded column'
        with tempfile.TemporaryDirectory() as directory:
            source = Path(directory) / 'synthetic.parquet'
            parquet.write_table(pyarrow.Table.from_pylist(values), source)
            captured = list(subject.filtered_rows(source, ['FINAL-synthetic']))
        self.assertEqual([item['id'] for item in captured], ['FINAL-synthetic'])
        self.assertEqual(set(captured[0]), {'id', 'question', 'answer', 'context', 'supporting_facts'})
        self.assertEqual(captured[0]['context'], row('FINAL-synthetic')['context'])
        self.assertEqual(pyarrow.cpu_count(), 1)
        self.assertEqual(pyarrow.io_thread_count(), 1)

    def test_observer_uses_actual_owned_success_policy_and_rejects_cleanup_refusal(self):
        spec = importlib.util.spec_from_file_location('reviewed_runner', SOURCE.with_name('run-reviewed.py'))
        runner = importlib.util.module_from_spec(spec)
        spec.loader.exec_module(runner)
        spec = importlib.util.spec_from_file_location('actual_job_owner', subject.ROOT / 'scripts/provision-static-onnx-runtime.py')
        owner = importlib.util.module_from_spec(spec)
        spec.loader.exec_module(owner)
        valid = {'exit_code': 0, 'timed_out': False, 'log_oversized': False,
                 'owned_descendants_settled': True, 'owned_job_forced_termination': False}
        self.assertTrue(runner.strict_normal_close(owner, valid))
        self.assertFalse(runner.strict_normal_close(owner, {**valid, 'cleanup_refusal': {'code': 'retained'}}))
        self.assertFalse(runner.strict_normal_close(owner, {**valid, 'owned_job_forced_termination': True}))
        self.assertFalse(runner.strict_normal_close(owner, {key: value for key, value in valid.items() if key != 'owned_job_forced_termination'}))

    def test_missing_duplicate_foreign_rows_article_drift_and_shortfall_refuse(self):
        member = {'a': ('component', {'article'})}
        for scan in (lambda _: [], lambda _: [row('a'), row('a')], lambda _: [row('dev')]):
            with self.assertRaises(ValueError):
                subject.select_hotpot(member, scan, lambda _: 'article', self.neutral, 1)
        with self.assertRaises(ValueError):
            subject.convert_hotpot(row('a'), {'different-article'}, lambda _: 'article', self.neutral)
        with self.assertRaises(ValueError):
            subject.select_hotpot(member, lambda _: [row('a', True, 1)], lambda _: 'article', self.neutral, 1)

    def test_original_support_cardinality_malformed_rows_never_silently_zip(self):
        value = row('a')
        value['supporting_facts']['title'].append('lost support')
        with self.assertRaises(ValueError):
            subject.convert_hotpot(value, {'article'}, lambda _: 'article', self.neutral)
        value = row('a')
        value['supporting_facts']['sent_id'] = [True]
        with self.assertRaises(ValueError):
            subject.convert_hotpot(value, {'article'}, lambda _: 'article', self.neutral)

    def test_qasper_global_rule_uses_all_eligible_and_retains_all_original_alternatives(self):
        payload = qasper_payload()
        question = payload['papers'][0]['questions'][0]
        question['human_alternatives'].append(copy.deepcopy(question['human_alternatives'][0]))
        question['human_alternatives'][1]['human_annotation_id'] = 'independent-second-alternative'
        question['human_alternatives'][1]['original_human_record']['annotation_id'] = 'independent-second-alternative'
        before = copy.deepcopy(payload)
        docs, tasks, rights = subject.qasper_selection(payload, 2, 3, 2)
        self.assertEqual(payload, before)
        self.assertEqual([task['question_id'] for task in tasks], sorted(['a', 'b'], key=lambda qid: subject.rank('qasper', qid)))
        chosen = next(task for task in tasks if task['question_id'] == 'a')
        self.assertEqual([alt['original_human_record'] for alt in chosen['human_alternatives']],
                         [alt['original_human_record'] for alt in question['human_alternatives']])
        self.assertEqual([alt['original_evidence_count'] for alt in chosen['human_alternatives']], [1, 1])
        self.assertEqual(set(docs), {'synthetic-paper:0:0'})
        self.assertEqual(len(rights), 1)

    def test_qasper_cannot_union_incomplete_alternatives_change_counts_or_claim_eligibility(self):
        payload = qasper_payload()
        bad = payload['papers'][0]['questions'][2]
        bad['eligibility'] = 'eligible'
        with self.assertRaises(ValueError):
            subject.qasper_selection(payload, 2, 3, 2)
        payload = qasper_payload()
        payload['papers'][0]['questions'][0]['human_alternatives'][0]['original_evidence_count'] = 0
        with self.assertRaises(ValueError):
            subject.qasper_selection(payload, 2, 3, 2)
        payload = qasper_payload()
        payload['papers'][0]['questions'].append(copy.deepcopy(payload['papers'][0]['questions'][0]))
        with self.assertRaises(ValueError):
            subject.qasper_selection(payload, 2, 4, 2)

    def test_fresh_anchor_no_output_before_owner_proof_no_overwrite(self):
        with tempfile.TemporaryDirectory() as directory:
            root = Path(directory).resolve()
            anchor = root / 'private-anchor'
            anchor.mkdir()
            output = anchor / 'materialized'
            proof = {'synthetic_owner_only': True}
            observed, identity = subject.guard_output(anchor, output, [], lambda _: proof)
            self.assertEqual(observed, proof)
            self.assertEqual(identity, (anchor.stat().st_dev, anchor.stat().st_ino))
            self.assertFalse(output.exists())
            with self.assertRaises(ValueError):
                subject.guard_output(anchor, output, [], lambda _: (_ for _ in ()).throw(ValueError('owner_refused')))
            self.assertFalse(output.exists())
            output.mkdir()
            with self.assertRaises(ValueError):
                subject.guard_output(anchor, output, [], lambda _: proof)



class DiagnosticGuards(unittest.TestCase):
    def test_exception_values_chains_locals_and_forged_code_are_redacted(self):
        secret = 'SYNTHETIC_SECRET_QUESTION_ANSWER_TITLE_ID'
        for kind in (ValueError, KeyError, MemoryError, type(secret, (Exception,), {})):
            namespace = {'exception': kind, 'secret': secret}
            exec(compile('def forged():\n    private_gold = secret\n    raise exception(secret)\n', secret, 'exec'), namespace)
            try:
                try:
                    raise ValueError(secret)
                except ValueError:
                    namespace['forged']()
            except BaseException as error:
                report = subject.safe_failure(error)
            encoded = json.dumps(report)
            self.assertNotIn(secret, encoded)
            self.assertEqual(report['diagnostic_code'], 'unexpected_exception')
            self.assertIsNone(report['location'])
        try:
            subject.refuse(secret)
        except BaseException as error:
            report = subject.safe_failure(error)
        self.assertNotIn(secret, json.dumps(report))
        self.assertEqual(report['exception_class'], 'RuntimeError')
        self.assertEqual(report['location']['filename'], 'adapter.py')
        self.assertEqual(report['location']['function'], 'refuse')
        self.assertIs(type(report['location']['line']), int)

    def test_closed_refusal_codes_and_original_row_boundary_remain(self):
        try:
            subject.refuse('publisher_support_cardinality')
        except BaseException as error:
            report = subject.safe_failure(error)
        self.assertEqual(report['diagnostic_code'], 'publisher_support_cardinality')
        self.assertEqual(report['exception_class'], 'OpaqueRefusal')
        self.assertEqual(set(report), {'complete', 'stage', 'refusal', 'diagnostic_code', 'exception_class', 'location', 'memory_counters'})
        invalid = row('synthetic')
        invalid['context']['title'] *= 2
        invalid['context']['sentences'] *= 2
        with self.assertRaisesRegex(subject.OpaqueRefusal, '^publisher_duplicate_context_titles$'):
            subject.convert_hotpot(invalid, {'article'}, lambda _: 'article', None)
        # A counterfeit typed exception cannot turn its argument into a diagnostic.
        report = subject.safe_failure(subject.OpaqueRefusal('SYNTHETIC_SECRET'))
        self.assertEqual(report['diagnostic_code'], 'unexpected_exception')
        self.assertNotIn('SYNTHETIC_SECRET', json.dumps(report))


    def test_exact_trusted_arrow_and_unicode_classifications_never_emit_messages(self):
        import pyarrow as arrow
        subject.bind_arrow_diagnostics(arrow)
        secret = 'SYNTHETIC_SECRET_HUMAN_TEXT'
        actual = []
        for operation, expected in (
            (lambda: arrow.array([secret], type=arrow.int64()), 'ArrowInvalid'),
            (lambda: arrow.array([1], type=arrow.binary()), 'ArrowTypeError'),
            (lambda: arrow.array([1]).cast(arrow.struct([])), 'ArrowNotImplementedError')):
            try:
                operation()
            except BaseException as error:
                report = subject.safe_failure(error)
            else:
                self.fail('synthetic Arrow operation unexpectedly accepted')
            self.assertEqual(report['exception_class'], expected)
            self.assertNotIn(secret, json.dumps(report))
            actual.append(report['exception_class'])
        # Construct the actual upstream class; do not force a genuine allocation OOM.
        report = subject.safe_failure(arrow.ArrowMemoryError(secret))
        self.assertEqual(report['exception_class'], 'ArrowMemoryError')
        self.assertNotIn(secret, json.dumps(report))
        for error, expected in (
            (UnicodeDecodeError(secret, b'\xff', 0, 1, secret), 'UnicodeDecodeError'),
            (UnicodeEncodeError(secret, secret, 0, 1, secret), 'UnicodeEncodeError'),
            (UnicodeTranslateError(secret, 0, 1, secret), 'UnicodeTranslateError')):
            report = subject.safe_failure(error)
            self.assertEqual(report['exception_class'], expected)
            self.assertNotIn(secret, json.dumps(report))
        fake = type('ArrowInvalid', (Exception,), {})
        self.assertEqual(subject.safe_failure(fake(secret))['exception_class'], 'OtherException')

    def test_memory_counters_only_emit_nonnegative_exact_integers_or_null(self):
        previous = subject.MEMORY_OBSERVERS
        try:
            subject.MEMORY_OBSERVERS = (lambda: 123456, lambda: 789)
            self.assertEqual(subject.safe_failure(MemoryError('SECRET'))['memory_counters'],
                {'peak_committed_process_bytes': 123456, 'arrow_current_pool_bytes': 789})
            for invalid in ('SYNTHETIC_SECRET', True, -1, 1.5, 18446744073709551616):
                subject.MEMORY_OBSERVERS = (lambda: invalid, lambda: (_ for _ in ()).throw(ValueError('SYNTHETIC_SECRET')))
                report = subject.safe_failure(ValueError('SYNTHETIC_SECRET'))
                self.assertEqual(report['memory_counters'],
                    {'peak_committed_process_bytes': None, 'arrow_current_pool_bytes': None})
                self.assertNotIn('SYNTHETIC_SECRET', json.dumps(report))
            import pyarrow as arrow
            subject.MEMORY_OBSERVERS = (None, arrow.total_allocated_bytes)
            report = subject.safe_failure(ValueError('SECRET'))
            self.assertIsNone(report['memory_counters']['peak_committed_process_bytes'])
            self.assertIs(type(report['memory_counters']['arrow_current_pool_bytes']), int)
        finally:
            subject.MEMORY_OBSERVERS = previous

if __name__ == '__main__':
    unittest.main()
