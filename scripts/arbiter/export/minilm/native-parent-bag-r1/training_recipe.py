# SPDX-License-Identifier: Apache-2.0
"""Opt-in visible-prefix mining; never changes native semantic-unit planning."""
import json

import training_recipe_audit as source


def _objects(data, name):
    raw = source._field(data, name)
    if raw is None or raw[0] != 91:
        raise ValueError('source array missing')
    at = 1
    while True:
        at = source._white(raw, at)
        if raw[at] == 93:
            return
        end = source._end(raw, at)
        yield raw[at:end]
        at = source._white(raw, end)
        if raw[at] == 44:
            at += 1
        elif raw[at] != 93:
            raise ValueError('invalid source array')


def train_only_corpus(data):
    """Inspect scalar scope fields first; decode no held-out text or alternatives."""
    task_ids = set()
    for item in _objects(data, 'tasks'):
        if json.loads(source._field(item, 'split')) == 'train' and json.loads(source._field(item, 'dataset')) in ('QASPER', 'requests-original-docstrings'):
            task_ids.add(json.loads(source._field(item, 'id')))
    tasks = list(source.selected_array_objects(data, 'tasks', task_ids))
    groups = {t['source_group'] for t in tasks}
    doc_ids = set()
    for item in _objects(data, 'documents'):
        if json.loads(source._field(item, 'source_group')) in groups and json.loads(source._field(item, 'dataset')) in ('QASPER', 'requests-original-docstrings'):
            doc_ids.add(json.loads(source._field(item, 'id')))
    documents = list(source.selected_array_objects(data, 'documents', doc_ids))
    return {'tasks': tasks, 'documents': documents}


def visible_prefix(tokenizer, text):
    """Use the real configured tokenizer offsets without mutating its state."""
    if not tokenizer.truncation or tokenizer.truncation.get('max_length') != 256:
        raise ValueError('actual256-token training tokenizer required')
    encoded = tokenizer.encode(text)
    if len(encoded.ids) > 256:
        raise ValueError('encoded input exceeds training token cap')
    end = max((end for _, end in encoded.offsets), default=0)
    if not 0 <= end <= len(text):
        raise ValueError('token offsets outside original text')
    return text[:end]


def negative_order(question, allowed, positives, visible_documents):
    """Same deterministic lexical objective, applied to actual encoded contents."""
    words = set(question.lower().split())
    return sorted((d for d in allowed if d not in positives),
                  key=lambda d: (-len(words & set(visible_documents[d].lower().split())), d))


def assert_training_rows(rows, frozen):
    """Fail before optimizer construction if any executed row differs."""
    if len(rows) != 315 or rows != frozen:
        raise ValueError('executed training rows differ from frozen315 master')
