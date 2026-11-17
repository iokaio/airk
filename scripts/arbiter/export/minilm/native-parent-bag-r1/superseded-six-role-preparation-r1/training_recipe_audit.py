# SPDX-License-Identifier: Apache-2.0
"""TRAIN-only token/crop audit; no model weights, inference or recipe change."""
import hashlib
import json
from collections import Counter


def _string_end(data, start):
    at = start + 1
    while at < len(data):
        if data[at] == 92:
            at += 2
        elif data[at] == 34:
            return at + 1
        else:
            at += 1
    raise ValueError('unterminated JSON string')


def _white(data, at):
    while at < len(data) and data[at] in b' \r\n\t':
        at += 1
    return at


def _end(data, at):
    at = _white(data, at)
    if data[at] == 34:
        return _string_end(data, at)
    if data[at] in (91, 123):
        stack = [data[at]]
        at += 1
        while stack:
            if data[at] == 34:
                at = _string_end(data, at)
                continue
            if data[at] in (91, 123):
                stack.append(data[at])
            elif data[at] in (93, 125):
                expected = 91 if data[at] == 93 else 123
                if stack.pop() != expected:
                    raise ValueError('mismatched JSON container')
            at += 1
        return at
    while at < len(data) and data[at] not in b',]} \r\n\t':
        at += 1
    return at


def _field(data, wanted):
    """Decode structural keys only; skip values without deserializing content."""
    at = _white(data, 0)
    if data[at] != 123:
        raise ValueError('object required')
    at += 1
    while True:
        at = _white(data, at)
        if data[at] == 125:
            return None
        end = _string_end(data, at)
        key = json.loads(data[at:end])
        at = _white(data, end)
        if data[at] != 58:
            raise ValueError('missing field colon')
        start = _white(data, at + 1)
        end = _end(data, start)
        if key == wanted:
            return data[start:end]
        at = _white(data, end)
        if data[at] == 44:
            at += 1
        elif data[at] != 125:
            raise ValueError('missing object separator')


def selected_array_objects(data, array_name, wanted_ids):
    """Only selected TRAIN objects are decoded; other strings remain raw bytes."""
    raw = _field(data, array_name)
    if raw is None or raw[0] != 91:
        raise ValueError('selected array missing')
    at = 1
    while True:
        at = _white(raw, at)
        if raw[at] == 93:
            return
        end = _end(raw, at)
        item = raw[at:end]
        identifier = _field(item, 'id')
        if identifier is None:
            raise ValueError('object ID missing')
        identifier = json.loads(identifier)
        if identifier in wanted_ids:
            yield json.loads(item)
        at = _white(raw, end)
        if raw[at] == 44:
            at += 1
        elif raw[at] != 93:
            raise ValueError('missing array separator')


def validate_training_rows(rows, tasks, documents):
    by_task = {t['id']: t for t in tasks}
    if set(by_task) != {r['task_id'] for r in rows}:
        raise ValueError('missing/foreign TRAIN task')
    positives = {}
    for task in tasks:
        if task['split'] != 'train' or task['dataset'] not in ('QASPER', 'requests-original-docstrings'):
            raise ValueError('nonlicensed/nonTRAIN task refused')
        positives[task['id']] = {loc for a in task['human_alternatives']
            if not a['unanswerable'] and a['alignment'] == 'all_original_evidence_maps_to_allowed_text'
            for e in a['evidence'] for loc in e['allowed_locations']}
    for row in rows:
        if row['question'] != by_task[row['task_id']]['question'] or row['positive'] not in positives[row['task_id']]:
            raise ValueError('training question/annotated positive differs')
        if row['distractor'] in positives[row['task_id']]:
            raise ValueError('annotated positive used as distractor')
        for identifier in (row['positive'], row['distractor']):
            doc = documents[identifier]
            if doc['dataset'] not in ('QASPER', 'requests-original-docstrings') or hashlib.sha256(doc['text'].encode()).hexdigest() != doc['paragraph_sha256']:
                raise ValueError('training document identity/rights family differs')
    for task_id, required in positives.items():
        if required != {r['positive'] for r in rows if r['task_id'] == task_id}:
            raise ValueError('original TRAIN positive population changed')
    return positives


def crop_observation(tokenizer, text):
    tokenizer.no_padding()
    tokenizer.no_truncation()
    full = tokenizer.encode(text)
    tokenizer.enable_truncation(max_length=256)
    cropped = tokenizer.encode(text)
    visible_end = max((end for start, end in cropped.offsets), default=0)
    return {'original_tokens_including_specials': len(full.ids), 'encoded_tokens': len(cropped.ids),
            'truncated': len(full.ids) > len(cropped.ids), 'original_utf8_bytes': len(text.encode()),
            'visible_prefix_utf8_bytes': len(text[:visible_end].encode())}, text[:visible_end]


def audit(rows, tasks, documents, tokenizer):
    positives = validate_training_rows(rows, tasks, documents)
    observations, visible = {}, {}
    for identifier, document in sorted(documents.items()):
        observations[identifier], visible[identifier] = crop_observation(tokenizer, document['text'])
    cropped_positive_rows, cropped_negative_rows, lost_mining_overlap = 0, 0, 0
    for row in rows:
        cropped_positive_rows += observations[row['positive']]['truncated']
        cropped_negative_rows += observations[row['distractor']]['truncated']
        words = set(row['question'].lower().split())
        full = words & set(documents[row['distractor']]['text'].lower().split())
        shown = words & set(visible[row['distractor']].lower().split())
        lost_mining_overlap += bool(full - shown)
    return {'training_rows': len(rows), 'training_questions': len(positives),
            'positive_cardinality_distribution': dict(Counter(len(v) for v in positives.values())),
            'cropped_positive_rows': cropped_positive_rows, 'cropped_distractor_rows': cropped_negative_rows,
            'hard_distractor_rows_losing_mined_lexical_terms_after_crop': lost_mining_overlap,
            'cross_source_group_distractors': sum(documents[r['positive']]['source_group'] != documents[r['distractor']]['source_group'] for r in rows),
            'document_crop_metadata': observations,
            'interpretation': 'Token/crop mismatch diagnostic, not a retrieval/source correctness defect or model quality result. No text or labels in output.'}
