# SPDX-License-Identifier: Apache-2.0
"""Synthetic-only isolation and exact historical neutral-conversion controls."""
import ast
import copy
import hashlib
import importlib.util
import json
from pathlib import Path
import sys
import tempfile
import unittest
from urllib.parse import quote

HERE = Path(__file__).resolve().parent
REPO = HERE.parents[3]
sys.path.insert(0, str(REPO / 'tools/retrieval-train'))
import human_corpus


def module(name, path):
    spec = importlib.util.spec_from_file_location(name, path)
    value = importlib.util.module_from_spec(spec)
    spec.loader.exec_module(value)
    return value


adapter = module('component_dev_adapter', HERE / 'materialize_dev.py')
clock = module('audit_helpers', HERE.parent / 'a1-next-experiment-blind-metadata-r5/audit.py')


def row(qid, title='Synthetic page', text='Unicode evidence é漢字.', evidence=True):
    return {'id': qid, 'question': 'Synthetic original question ' + qid, 'answer': 'Synthetic answer ' + qid,
            'context': {'title': [title], 'sentences': [[text]]},
            'supporting_facts': {'title': [title] if evidence else [], 'sent_id': [0] if evidence else []}}


class MaterializeTests(unittest.TestCase):
    def test_neutral_conversion_is_exact_original_loop_for_all_alternatives(self):
        old = ast.parse((HERE / 'human_corpus.before.py').read_text(encoding='utf-8'))
        main = next(x for x in old.body if isinstance(x, ast.FunctionDef) and x.name == 'main')
        original = next(x for x in main.body if isinstance(x, ast.For) and ast.unparse(x.iter) == 'hotpot_rows')
        new = ast.parse((REPO / 'tools/retrieval-train/human_corpus.py').read_text(encoding='utf-8'))
        helper = next(x for x in new.body if isinstance(x, ast.FunctionDef) and x.name == 'append_hotpot_rows')
        self.assertEqual(ast.dump(original, include_attributes=False), ast.dump(helper.body[1], include_attributes=False))
        cases = [row('selected'), row('quoted', text='A "quoted" paragraph'), row('unaligned', evidence=False), row('foreign')]
        ns = {'hotpot_rows': cases, 'selected_set': {'selected', 'quoted', 'unaligned'},
              'documents': {}, 'tasks': [], 'hotpot_exclusions': [], 'quote': quote,
              'hash_bytes': lambda value: hashlib.sha256(value).hexdigest()}
        exec(compile(ast.Module(body=[original], type_ignores=[]), '<retained-original-loop>', 'exec'), ns)
        documents, tasks, exclusions = {}, [], []
        human_corpus.append_hotpot_rows(cases, ns['selected_set'], documents, tasks, exclusions)
        expected = [ns['documents'], ns['tasks'], ns['hotpot_exclusions']]
        self.assertEqual(json.dumps([documents, tasks, exclusions], ensure_ascii=False).encode(),
                         json.dumps(expected, ensure_ascii=False).encode())

    def test_default_main_preserves_every_statement_except_exact_helper_call(self):
        old = ast.parse((HERE / 'human_corpus.before.py').read_text(encoding='utf-8'))
        new = ast.parse((REPO / 'tools/retrieval-train/human_corpus.py').read_text(encoding='utf-8'))
        oldmain = next(x for x in old.body if isinstance(x, ast.FunctionDef) and x.name == 'main')
        newmain = next(x for x in new.body if isinstance(x, ast.FunctionDef) and x.name == 'main')
        expected = copy.deepcopy(oldmain)
        index = next(i for i, x in enumerate(expected.body) if isinstance(x, ast.For) and ast.unparse(x.iter) == 'hotpot_rows')
        expected.body[index] = ast.parse('append_hotpot_rows(hotpot_rows, selected_set, documents, tasks, hotpot_exclusions)').body[0]
        self.assertEqual(ast.dump(expected, include_attributes=False), ast.dump(newmain, include_attributes=False))

    def test_real_arrow_filter_never_delivers_final_or_calibration_content(self):
        import pyarrow as pa
        import pyarrow.parquet as pq
        cases = [row('chosen'), row('FINAL', text='FINAL_CONTENT_SENTINEL'), row('CAL', text='CAL_CONTENT_SENTINEL')]
        component = 'a' * 64
        audit = {'components': [{'component_digest': component, 'article_digests': [clock.article('Synthetic page')]}]}
        selected = [{'question_id': 'chosen', 'component_digest': component}]
        with tempfile.TemporaryDirectory(dir=HERE) as directory:
            source = Path(directory) / 'synthetic.parquet'
            pq.write_table(pa.Table.from_pylist(cases), source)
            observed = []
            class Observer:
                @staticmethod
                def append_hotpot_rows(rows, wanted, documents, tasks, exclusions):
                    observed.extend(x['id'] for x in rows)
                    human_corpus.append_hotpot_rows(rows, wanted, documents, tasks, exclusions)
            result = adapter.scan_convert(source, selected, audit, Observer, clock)
        self.assertEqual(observed, ['chosen'])
        encoded = json.dumps(result, ensure_ascii=False)
        self.assertNotIn('FINAL_CONTENT_SENTINEL', encoded)
        self.assertNotIn('CAL_CONTENT_SENTINEL', encoded)
        self.assertEqual(result['tasks'][0]['split'], 'development')
        self.assertEqual(result['tasks'][0]['human_alternatives'][0]['original_human_answer'], 'Synthetic answer chosen')

    def test_fixed_component_ranking_ignores_non_dev_and_refuses_shortage(self):
        components = [{'component_digest': hashlib.sha256(str(i).encode()).hexdigest(),
                       'question_ids': ['q' + str(i), 'other' + str(i)], 'disposition': 'DEV'} for i in range(66)]
        audit = {'components': components + [{'component_digest': '0'*64, 'question_ids': ['FINAL'], 'disposition': 'FINAL_SEALED_UNOPENED'}]}
        chosen = adapter.choose(audit)
        self.assertEqual(len(chosen), 64)
        self.assertEqual(len({x['component_digest'] for x in chosen}), 64)
        self.assertNotIn('FINAL', {x['question_id'] for x in chosen})
        self.assertEqual(chosen, adapter.choose({'components': list(reversed(audit['components']))}))
        with self.assertRaisesRegex(ValueError, 'insufficient'):
            adapter.choose({'components': components[:63]})

    def test_selection_tampering_refuses_before_content_scan(self):
        audit = {'components': [{'component_digest': hashlib.sha256(str(i).encode()).hexdigest(),
                                 'question_ids': ['q' + str(i)], 'disposition': 'DEV'} for i in range(64)]}
        selection = {'protocol_sha256': adapter.PROTOCOL_SHA, 'metadata_result_sha256': adapter.AUDIT_SHA,
                     'selected': adapter.choose(audit)}
        adapter.validate_selection(selection, audit)
        foreign = copy.deepcopy(selection)
        foreign['selected'][0]['question_id'] = 'CAL_OR_FINAL'
        with self.assertRaisesRegex(ValueError, 'differs'):
            adapter.validate_selection(foreign, audit)
        foreign = copy.deepcopy(selection)
        foreign['protocol_sha256'] = '0' * 64
        with self.assertRaisesRegex(ValueError, 'foreign'):
            adapter.validate_selection(foreign, audit)

    def test_selected_missing_duplicate_and_foreign_article_refuse_without_replacement(self):
        import pyarrow as pa
        import pyarrow.parquet as pq
        component = 'b' * 64
        selected = [{'question_id': 'chosen', 'component_digest': component}]
        audit = {'components': [{'component_digest': component, 'article_digests': [clock.article('Synthetic page')]}]}
        with tempfile.TemporaryDirectory(dir=HERE) as directory:
            source = Path(directory) / 'synthetic.parquet'
            for cases, reason in [([row('foreign')], 'missing'), ([row('chosen'), row('chosen')], 'duplicate'),
                                  ([row('chosen', title='Foreign component article')], 'outside')]:
                pq.write_table(pa.Table.from_pylist(cases), source)
                with self.assertRaisesRegex(ValueError, reason):
                    adapter.scan_convert(source, selected, audit, human_corpus, clock)

    def test_quote_alignment_refusal_is_retained_and_never_replaced(self):
        import pyarrow as pa
        import pyarrow.parquet as pq
        selected = [{'question_id': 'chosen', 'component_digest': 'c' * 64}]
        audit = {'components': [{'component_digest': 'c' * 64, 'article_digests': [clock.article('Synthetic page')]}]}
        with tempfile.TemporaryDirectory(dir=HERE) as directory:
            source = Path(directory) / 'synthetic.parquet'
            pq.write_table(pa.Table.from_pylist([row('chosen', text='Original "quoted" evidence')]), source)
            result = adapter.scan_convert(source, selected, audit, human_corpus, clock)
        self.assertEqual([t['question_id'] for t in result['tasks']], ['chosen'])
        self.assertEqual(result['documents'], [])
        self.assertEqual(len(result['hotpot_exclusions']), 1)
        self.assertEqual(result['tasks'][0]['human_alternatives'][0]['original_evidence_count'], 1)
        self.assertEqual(result['tasks'][0]['human_alternatives'][0]['evidence'][0]['allowed_locations'], [])
        self.assertEqual(result['tasks'][0]['human_alternatives'][0]['alignment'], 'unsupported_or_excluded_evidence_not_admitted')


if __name__ == '__main__':
    unittest.main()
