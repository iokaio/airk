# SPDX-License-Identifier: Apache-2.0
"""Synthetic FINAL projection guards only; never opens real FINAL payloads."""
import copy
import hashlib
import importlib.util
from pathlib import Path
import tempfile
import unittest

HERE = Path(__file__).resolve().parent
def load(name):
    spec = importlib.util.spec_from_file_location(name, HERE / (name + '.py'))
    module = importlib.util.module_from_spec(spec)
    spec.loader.exec_module(module)
    return module
fixture, observer = load('fixture'), load('observer')
def sha(text):
    return hashlib.sha256(text.encode()).hexdigest()


def synthetic():
    docs = [
        {'id': 'hotpot:synthetic-doc', 'text': 'λ first.Second.', 'title': 'Synthetic article',
         'source_group': 'wikipedia:Synthetic article', 'dataset': 'HotpotQA', 'url': 'https://example.invalid/article',
         'license': 'SYNTHETIC', 'paragraph_sha256': sha('λ first.Second.')},
        {'id': 'paper:0:0', 'text': 'First paragraph.', 'title': 'Synthetic paper',
         'source_group': 'qasper:paper', 'dataset': 'QASPER', 'url': 'https://example.invalid/paper',
         'license': 'SYNTHETIC', 'paragraph_sha256': sha('First paragraph.')},
        {'id': 'paper:0:1', 'text': 'Second paragraph.', 'title': 'Synthetic paper',
         'source_group': 'qasper:paper', 'dataset': 'QASPER', 'url': 'https://example.invalid/paper',
         'license': 'SYNTHETIC', 'paragraph_sha256': sha('Second paragraph.')}]
    common = {'split': 'test', 'experiment_partition': 'FINAL', 'disagreement': False}
    def alternative(identifier, fact):
        return {'human_annotation_id': identifier, 'unanswerable': False, 'original_evidence_count': 1,
                'original_human_answer': 'SYNTHETIC answer', 'evidence': [fact],
                'alignment': 'all_original_evidence_maps_to_allowed_text'}
    tasks = [{**common, 'id': 'hotpot:q', 'question_id': 'q', 'question': 'SYNTHETIC question',
              'dataset': 'HotpotQA', 'source_group': 'hotpot-question:q',
              'human_alternatives': [alternative('h', {'source_title': 'Synthetic article', 'sentence_index': 1,
                  'source_evidence_sha256': sha('Second.'), 'allowed_locations': ['hotpot:synthetic-doc']})]},
             {**common, 'id': 'qasper:q2', 'question_id': 'q2', 'question': 'SYNTHETIC question2',
              'dataset': 'QASPER', 'source_group': 'qasper:paper',
              'human_alternatives': [alternative('q1', {'source_evidence_sha256': sha('First paragraph.'), 'allowed_locations': ['paper:0:0']}),
                                     alternative('q2', {'source_evidence_sha256': sha('Second paragraph.'), 'allowed_locations': ['paper:0:1']})]}]
    audit = [{'selected': True, 'component_digest': 'a' * 64, 'original_row': {
        'id': 'q', 'question': 'SYNTHETIC question',
        'context': {'title': ['Synthetic article'], 'sentences': [['λ first.', 'Second.']]}}}]
    return {'tasks': tasks, 'documents': docs, 'hotpot_alignment_audit_private': audit}


class ProjectionGuards(unittest.TestCase):
    def build(self, corpus):
        return fixture.build(corpus, Path(tempfile.gettempdir()) / 'synthetic-final-root', {'HotpotQA': 1, 'QASPER': 1})

    def test_exact_utf8_offsets_scope_all_original_alternatives_and_goal_domain(self):
        corpus = synthetic()
        bodies, manifest, rubric = self.build(corpus)
        self.assertEqual(len(bodies), 2)
        self.assertEqual(len(manifest['paragraphs']), 3)
        self.assertEqual([t['id'] for t in rubric], ['hotpot:q', 'qasper:q2'])
        hotpot, qasper = rubric
        fact = hotpot['alternatives'][0]['facts'][0]['locations'][0]
        self.assertEqual(fact['byte_start'], len('λ first.'.encode()))
        self.assertEqual(bodies[fact['file']][fact['byte_start']:fact['byte_end']], b'Second.')
        self.assertEqual([a['human_annotation_id'] for a in qasper['alternatives']], ['q1', 'q2'])
        location = qasper['alternatives'][1]['facts'][0]['locations'][0]
        self.assertEqual(location['byte_start'], len(b'First paragraph.\n\n'))
        self.assertEqual(bodies[location['file']], b'First paragraph.\n\nSecond paragraph.')
        self.assertTrue(hotpot['source_root'].replace('\\', '/').endswith('/sources/hotpot'))
        self.assertTrue(qasper['source_root'].replace('\\', '/').endswith('/sources/qasper/paper'))
        self.assertEqual(qasper['alternatives'][0]['original_alternative'], corpus['tasks'][1]['human_alternatives'][0])
        expected = sorted(['hotpot:q', 'qasper:q2'], key=lambda value: sha('airk.goal.final.v1:' + value))
        self.assertEqual(manifest['goal_ids'], expected)

    def test_missing_support_empty_facts_or_dropped_task_never_becomes_perfect(self):
        for mutate in (
            lambda c: c['tasks'].pop(),
            lambda c: c['tasks'][0]['human_alternatives'][0]['evidence'][0].update(allowed_locations=[]),
            lambda c: c['tasks'][0]['human_alternatives'][0].update(original_evidence_count=0, evidence=[]),
            lambda c: c['tasks'][0]['human_alternatives'][0].update(original_evidence_count=2)):
            corpus = synthetic()
            mutate(corpus)
            with self.assertRaises(ValueError):
                self.build(corpus)

    def test_byte_tamper_duplicate_task_alias_and_foreign_evidence_refuse(self):
        for mutate in (
            lambda c: c['documents'][0].update(text='SYNTHETIC tampered'),
            lambda c: c['tasks'][1].update(id=c['tasks'][0]['id']),
            lambda c: c['documents'][1].update(source_group='qasper:../escape'),
            lambda c: c['tasks'][1]['human_alternatives'][0]['evidence'][0].update(allowed_locations=['unknown'])):
            corpus = synthetic()
            mutate(corpus)
            with self.assertRaises(ValueError):
                self.build(corpus)

    def test_unaligned_original_alternative_is_preserved_without_union_or_replacement(self):
        corpus = synthetic()
        alternative = copy.deepcopy(corpus['tasks'][1]['human_alternatives'][0])
        alternative.update(human_annotation_id='q3', alignment='unsupported_or_excluded_evidence_not_admitted')
        alternative['evidence'][0]['allowed_locations'] = []
        corpus['tasks'][1]['human_alternatives'].append(alternative)
        rubric = self.build(corpus)[2]
        self.assertEqual(len(rubric[1]['alternatives']), 3)
        self.assertEqual(rubric[1]['alternatives'][2]['facts'][0]['locations'], [])
        self.assertEqual(rubric[1]['alternatives'][2]['original_alternative'], alternative)


class SummaryGuards(unittest.TestCase):
    def rows(self, complete=0, state='ready'):
        return [{'id': 'synthetic-' + str(i), 'dataset': 'HotpotQA' if i < 171 else 'QASPER',
                 'evidence_recall_fraction': [complete, 1], 'complete_original_alternative': complete,
                 'semantic_state': state} for i in range(205)]

    def test_empty_results_are_zero_fixed205_not_empty_perfect(self):
        summary = observer.summarize(self.rows())
        self.assertEqual(summary['questions'], 205)
        self.assertEqual(summary['mean_evidence_recall_fraction'], [0, 1])
        self.assertFalse(summary['absolute_quality_pass'])
        self.assertTrue(observer.summarize(self.rows(1))['absolute_quality_pass'])
        self.assertFalse(observer.summarize(self.rows(1, 'unreported'))['absolute_quality_pass'])

    def test_missing_duplicate_wrong_family_or_unknown_fraction_refuses(self):
        for mutate in (lambda r: r.pop(), lambda r: r[-1].update(id=r[0]['id']),
                       lambda r: r[-1].update(dataset='UNKNOWN'),
                       lambda r: r[-1].update(evidence_recall_fraction=[2, 1]),
                       lambda r: r[-1].update(complete_original_alternative=1)):
            rows = self.rows()
            mutate(rows)
            with self.assertRaises(ValueError):
                observer.summarize(rows)



class RetainedExecutionGuards(unittest.TestCase):
    def test_replaced_guard_path_does_not_execute_after_capture(self):
        with tempfile.TemporaryDirectory() as directory:
            path = Path(directory) / 'guard.py'
            marker = Path(directory) / 'FORBIDDEN'
            original = b'VALUE = 17\n'
            path.write_bytes(original)
            retained = path.read_bytes()
            path.write_text('from pathlib import Path\nPath(' + repr(str(marker)) + ').write_text("secret")\nVALUE=99\n')
            module = observer.import_retained('captured_guard', path, retained)
            self.assertEqual(module.VALUE, 17)
            self.assertFalse(marker.exists())

    def test_whole_grading_helper_closure_uses_captured_bytes_not_paths_or_cache(self):
        import sys
        import types
        names = ('synthetic_goal_rubric', 'synthetic_goal_sources', 'synthetic_grade_native')
        with tempfile.TemporaryDirectory() as directory:
            specifications = []
            texts = ('VALUE=11\n', 'VALUE=22\n',
                     'from synthetic_goal_rubric import VALUE as A\nfrom synthetic_goal_sources import VALUE as B\nVALUE=A+B\n')
            marker = Path(directory) / 'FORBIDDEN'
            previous = {name: sys.modules.get(name) for name in names}
            foreign = {}
            try:
                for name, text in zip(names, texts):
                    path = Path(directory) / (name + '.py')
                    path.write_text(text)
                    specifications.append((name, path, path.read_bytes()))
                    path.write_text('from pathlib import Path\nPath(' + repr(str(marker)) + ').write_text("secret")\n')
                    fake = types.ModuleType(name)
                    fake.VALUE = 999
                    sys.modules[name] = fake
                    foreign[name] = fake
                modules = observer.import_helper_closure(specifications)
                self.assertEqual(modules[names[2]].VALUE, 33)
                self.assertFalse(marker.exists())
                for name in names:
                    self.assertIs(sys.modules[name], foreign[name])
            finally:
                for name, original in previous.items():
                    if original is None:
                        sys.modules.pop(name, None)
                    else:
                        sys.modules[name] = original

if __name__ == '__main__':
    unittest.main()
