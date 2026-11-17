# SPDX-License-Identifier: Apache-2.0
"""Root-authorized TRAIN-only bounded audit; never opens model weights."""
from pathlib import Path
import hashlib
import importlib.util
import json
import os
import random
import sys
import threading
import time

START = time.monotonic()
HERE = Path(__file__).resolve().parent
BASE = HERE.parent
REPO = HERE.parents[3]
sys.path.insert(0, str(REPO / 'tools/retrieval-train'))
helper = BASE / 'a1-next-experiment-blind-metadata-r5/audit.py'
spec = importlib.util.spec_from_file_location('train_audit_clock', helper)
clock = importlib.util.module_from_spec(spec)
spec.loader.exec_module(clock)
clock.START = START
clock.JOB_STATE = clock.enforce_budget()


def expire():
    if not clock.done.wait(max(0, 60 - (time.monotonic() - START))):
        os.write(2, b'REFUSED: TRAIN audit60second budget exhausted\n')
        os._exit(124)


threading.Thread(target=expire, daemon=True).start()
try:
    import training_recipe_audit as audit
    seal_path = HERE / 'input-seal.json'
    seal = json.loads(seal_path.read_text(encoding='utf-8'))
    for item in seal['files']:
        if clock.digest(item['path']) != item['sha256']:
            raise ValueError('TRAIN audit source/runtime/input drift')
    rows = json.loads((BASE / 'minilm-trained-r3/training-rows.private.json').read_text(encoding='utf-8'))
    task_ids = {r['task_id'] for r in rows}
    doc_ids = {r[k] for r in rows for k in ('positive', 'distractor')}
    corpus_raw = (BASE / 'human-corpus-r1/corpus.private.json').read_bytes()
    extra_raw = (BASE / 'requests-train-rights/pairs.private.json').read_bytes()
    tasks, documents = [], {}
    for data in (corpus_raw, extra_raw):
        tasks.extend(audit.selected_array_objects(data, 'tasks', task_ids))
        for doc in audit.selected_array_objects(data, 'documents', doc_ids):
            if doc['id'] in documents:
                raise ValueError('duplicate selected TRAIN document')
            documents[doc['id']] = doc
    if len(rows) != 315 or len(tasks) != 287 or set(documents) != doc_ids:
        raise ValueError('original315row/287question TRAIN closure incomplete')
    rights = json.loads(audit._field(corpus_raw, 'qasper_rights_decisions'))
    train_papers = {r['paper_id'] for r in rights if r['publisher_split'] == 'train' and r['license'] == 'CC-BY-4.0'}
    request_groups = {t['source_group'] for t in tasks if t['dataset'] == 'requests-original-docstrings'}
    for doc in documents.values():
        if doc['dataset'] == 'QASPER':
            if doc['source_group'] not in {'qasper:' + paper for paper in train_papers}:
                raise ValueError('QASPER document outside approved TRAIN paper family')
        elif doc['dataset'] == 'requests-original-docstrings':
            if doc['source_group'] not in request_groups:
                raise ValueError('Requests source family outside original TRAIN tasks')
        else:
            raise ValueError('other corpus family refused')
    del corpus_raw, extra_raw
    from tokenizers import Tokenizer
    tokenizer = Tokenizer.from_file(str(BASE / 'minilm-training-source/tokenizer.json'))
    report = audit.audit(rows, tasks, documents, tokenizer)
    known = {t['id']: {r['positive'] for r in rows if r['task_id'] == t['id']} for t in tasks}
    masks, repeated_question_batches = 0, 0
    for epoch in range(3):
        order = list(rows)
        random.Random(1908 + epoch).shuffle(order)
        for offset in range(0, len(order), 8):
            batch = order[offset:offset + 8]
            repeated_question_batches += len({r['task_id'] for r in batch}) < len(batch)
            passage_ids = [r['positive'] for r in batch] + [r['distractor'] for r in batch]
            masks += sum(i != j and identifier in known[row['task_id']]
                         for i, row in enumerate(batch) for j, identifier in enumerate(passage_ids))
    for item in seal['files']:
        if clock.digest(item['path']) != item['sha256']:
            raise ValueError('post-audit source/runtime/input drift')
    report.update({'schema': 'airk.private-TRAIN-only-recipe-audit/1', 'input_seal_sha256': clock.digest(seal_path),
                   'script_sha256': clock.digest(__file__), 'approved_QASPER_TRAIN_paper_families': len(train_papers),
                   'dataset_task_counts': {name: sum(t['dataset'] == name for t in tasks) for name in ('QASPER', 'requests-original-docstrings')},
                   'actual_training_masked_other_positive_columns': masks,
                   'actual_batches_with_multiple_rows_for_same_question': repeated_question_batches,
                   'known_positive_negative_label_violations': 0,
                   'scope': 'Selected original licensed TRAIN objects only; no oldfinal/newDEV/CAL/FINAL outcomes/text decoded. Other objects remain skipped rawJSON bytes. No model weights/torch/inference/training/scoring/native/Cargo/network.',
                   'peak_committed_process_bytes': clock.peak_memory(), 'elapsed_seconds': time.monotonic() - START})
    clock.atomic_stream(HERE / 'audit-result.json', report)
    print(json.dumps({k: report[k] for k in ['training_rows', 'training_questions', 'positive_cardinality_distribution',
         'cropped_positive_rows', 'cropped_distractor_rows', 'hard_distractor_rows_losing_mined_lexical_terms_after_crop',
         'cross_source_group_distractors', 'dataset_task_counts', 'actual_training_masked_other_positive_columns',
         'actual_batches_with_multiple_rows_for_same_question', 'peak_committed_process_bytes', 'elapsed_seconds']}))
except Exception as error:
    print(json.dumps({'status': 'REFUSED', 'exception_type': type(error).__name__, 'reason': str(error),
                      'peak_committed_process_bytes': clock.peak_memory(), 'elapsed_seconds': time.monotonic() - START}), file=sys.stderr)
    raise
finally:
    clock.done.set()
