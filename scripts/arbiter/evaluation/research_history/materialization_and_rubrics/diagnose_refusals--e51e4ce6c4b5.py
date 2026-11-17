# SPDX-License-Identifier: Apache-2.0
"""Bounded existing DEV-only conversion diagnosis; metadata output, no scoring."""
from pathlib import Path
import hashlib
import importlib.util
import json
import os
import sys
import threading
import time

START = time.monotonic()
HERE = Path(__file__).resolve().parent
BASE = HERE.parent
REPO = HERE.parents[3]
sys.path.insert(0, str(REPO / 'tools/retrieval-train'))


def module(name, path):
    spec = importlib.util.spec_from_file_location(name, path)
    value = importlib.util.module_from_spec(spec)
    spec.loader.exec_module(value)
    return value


def load(path):
    return json.loads(Path(path).read_text(encoding='utf-8'))


clock = module('bounded_diagnosis_clock', BASE / 'a1-next-experiment-blind-metadata-r5/audit.py')
clock.START = START
clock.JOB_STATE = clock.enforce_budget()


def expire():
    if not clock.done.wait(max(0, 60 - (time.monotonic() - START))):
        os.write(2, b'REFUSED: diagnosis wall budget exhausted\n')
        os._exit(124)


threading.Thread(target=expire, daemon=True).start()
STAGE = 'input-fences'
try:
    seal = load(HERE / 'materialization-input-seal.json')
    seal_sha = clock.digest(HERE / 'materialization-input-seal.json')
    if seal_sha != '66a4e44048c32202b9ed6d1c95042eb90726eab2db07bb7cb3c9936a865ca35b':
        raise ValueError('materialization input seal changed')
    for r in seal['files']:
        if clock.digest(r['path']) != r['sha256']:
            raise ValueError('sealed input drift: ' + r['path'])
    adapter = module('sealed_dev_adapter', HERE / 'materialize_dev.py')
    selection = load(HERE / 'dev-selection.json')
    audit = load(BASE / 'a1-next-experiment-blind-metadata-r5/audit-result.json')
    adapter.validate_selection(selection, audit)
    old_path = BASE / 'a1-component-development-materialized-r1/corpus.private.json'
    if clock.digest(old_path) != 'f6c6616c1de29948deee9f37c70ccaadbb7a5dc71d0cd02c91d207c9ccbac097':
        raise ValueError('original DEV materialization drift')
    original = load(old_path)
    import human_corpus
    import pyarrow
    pyarrow.set_cpu_count(1)
    pyarrow.set_io_thread_count(1)
    observations = []
    totals = {}

    class Observer:
        @staticmethod
        def append_hotpot_rows(rows, wanted, documents, tasks, exclusions):
            for row in rows:
                available = {}
                quoted = set()
                for title, sentences in zip(row['context']['title'], row['context']['sentences']):
                    text = ''.join(sentences)
                    if any(c in text for c in '"“”«»„'):
                        quoted.add(title)
                    else:
                        available[title] = sentences
                categories = []
                for title, position in zip(row['supporting_facts']['title'], row['supporting_facts']['sent_id']):
                    if title in quoted:
                        category = 'original_support_context_quote_excluded'
                    elif title not in available:
                        category = 'original_support_title_absent_from_contexts'
                    elif not isinstance(position, int) or position < 0 or position >= len(available[title]):
                        category = 'original_support_sentence_position_absent'
                    else:
                        category = 'original_support_aligned'
                    categories.append(category)
                    totals[category] = totals.get(category, 0) + 1
                if not categories:
                    categories.append('original_no_support_positions')
                    totals['original_no_support_positions'] = totals.get('original_no_support_positions', 0) + 1
                observations.append({'question_id': row['id'], 'original_support_positions': len(row['supporting_facts']['title']),
                                     'position_categories': categories, 'quote_excluded_contexts': len(quoted),
                                     'all_original_support_aligned': bool(categories) and all(x == 'original_support_aligned' for x in categories)})
            human_corpus.append_hotpot_rows(rows, wanted, documents, tasks, exclusions)

    STAGE = 'exact-selected-DEV-only-filtered-diagnosis'
    converted = adapter.scan_convert(BASE / 'hotpot-dev.parquet', selection['selected'], audit, Observer, clock)
    for key in ['documents', 'tasks', 'hotpot_exclusions']:
        if converted[key] != original[key]:
            raise ValueError('reconversion differs from original frozen DEV output: ' + key)
    aligned = {x['question_id'] for x in observations if x['all_original_support_aligned']}
    expected = {t['question_id'] for t in original['tasks'] if all(a['alignment'] == 'all_original_evidence_maps_to_allowed_text' for a in t['human_alternatives'])}
    if aligned != expected or len(aligned) != 20 or len(observations) != 64:
        raise ValueError('independent support-position category and exact neutral output disagree')
    for r in seal['files']:
        if clock.digest(r['path']) != r['sha256']:
            raise ValueError('post-diagnosis input drift')
    result = {'schema': 'airk.private-component-dev-refusal-diagnosis/1', 'status': 'DEV_DIAGNOSIS_ONLY_NO_RULE_OR_SCORE_CHANGE',
              'protocol_sha256': adapter.PROTOCOL_SHA, 'materialization_input_seal_sha256': seal_sha,
              'corpus_sha256': clock.digest(old_path), 'script_sha256': clock.digest(__file__),
              'questions': sorted(observations, key=lambda x: x['question_id']), 'support_category_totals': totals,
              'fully_aligned_questions': 20, 'refused_questions': 44, 'exact_reconversion_unchanged': True,
              'all150_input_rechecks_match': True, 'peak_committed_process_bytes': clock.peak_memory(),
              'elapsed_seconds': time.monotonic() - START, 'limits': {'seconds': 60, 'committed_bytes': 268435456, 'compute_threads': 1},
              'not_done': 'No scoring/models/training/native/Cargo/network, no CAL/FINAL content returned, no sample refill or rights/alignment change.'}
    STAGE = 'atomic-metadata-output'
    clock.atomic_stream(HERE / 'refusal-diagnosis.json', result)
    print(json.dumps({k: result[k] for k in ['status', 'support_category_totals', 'fully_aligned_questions', 'refused_questions', 'exact_reconversion_unchanged', 'peak_committed_process_bytes', 'elapsed_seconds']}))
except Exception as error:
    print(json.dumps({'status': 'REFUSED', 'stage': STAGE, 'exception_type': type(error).__name__, 'reason': str(error),
                      'peak_committed_process_bytes': clock.peak_memory(), 'elapsed_seconds': time.monotonic() - START}), file=sys.stderr)
    raise
finally:
    clock.done.set()
