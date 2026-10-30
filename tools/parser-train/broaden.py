# SPDX-License-Identifier: Apache-2.0
"""Train phrase-family splits and explicit rejection; evaluate only frozen models.

This preserves protocol-4 results and its fixtures. Public data is read from
hash-verified local pins. Unsupported rows have thirteen zero targets, rather
than introducing a fourteenth runtime label or changing the feature format.
"""
import argparse
import csv
import hashlib
import json
from pathlib import Path
import random
import re
import struct

import numpy as np
from sklearn.linear_model import LogisticRegression
from sklearn.multiclass import OneVsRestClassifier

from train import LABELS, ROOT, matrix
from qualify import MORE_TEMPLATES, ROLES, TEMPLATES, quantize, save_jsonl, vocabulary
from phrasing import EXTRA_NEGATIVES, NEGATIVES, PHRASES, PREFIXES, REQUEST_VERBS, TRAINING_OBJECTS, split_families
from retention import WORK, retain

HERE = Path(__file__).parent
SEED = 84181


def normalized(text):
    return re.sub(r'\s+', ' ', text.strip().lower())


def online():
    lock = json.loads((HERE / 'online.lock.json').read_text(encoding='utf-8'))
    inputs = {}
    for source in lock['sources']:
        for record in source['inputs']:
            file = ROOT / record['local_path']
            if hashlib.sha256(file.read_bytes()).hexdigest() != record['sha256']:
                raise ValueError('Online input differs from its pinned digest')
            inputs[(source['id'], record['path'])] = file
    with inputs[('codesearchnet-queries', 'resources/queries.csv')].open(encoding='utf-8') as stream:
        queries = [row['query'] for row in csv.DictReader(stream)]
    clinc = json.loads(inputs[('clinc150', 'data/data_full.json')].read_text(encoding='utf-8'))
    return queries, clinc


def query_split(query):
    value = int(hashlib.sha256(normalized(query).encode()).hexdigest()[:8], 16) % 100
    return 'train' if value < 70 else 'validation' if value < 85 else 'calibration'


def wrap(template, role, rng):
    if rng.random() < .25:
        return rng.choice(ROLES[role]) + template
    if template.lower().startswith(('what ', 'where ', 'which ', 'how ', 'is ', 'are ', 'has ', 'does ', 'can ', 'could ')):
        return rng.choice(['', '', 'In this project, ']) + template
    return rng.choice(PREFIXES) + template


def synthetic(count, repos, split, queries, negative=False):
    rng = random.Random(SEED + {'train': 0, 'validation': 1, 'calibration': 2}[split] + (10 if negative else 0))
    rows = []
    seen = set()
    topics = [query for query in queries if query_split(query) == split]
    while len(rows) < count:
        label = 'unsupported' if negative else LABELS[len(rows) % len(LABELS)]
        role = list(ROLES)[len(rows) % len(ROLES)]
        repo = rng.choice(repos)
        data = repo['data']
        symbol = rng.choice(data['symbols'] or data['headings'] or ['DocumentSection'])
        path = rng.choice(data['paths'])
        families = split_families(NEGATIVES if negative else PHRASES[label], split)
        if negative and split == 'train':
            families = families + EXTRA_NEGATIVES
        family, variants = rng.choice(families)
        template = rng.choice(variants)
        origin = 'original-phrase-family'
        query = None
        if not negative and split == 'train' and rng.random() < .20:
            template = rng.choice(TEMPLATES[label][:4] + MORE_TEMPLATES[label])
            family = 'legacy-training-' + str((TEMPLATES[label][:4] + MORE_TEMPLATES[label]).index(template))
        if not negative and split == 'train' and rng.random() < .40:
            noun = rng.choice(TRAINING_OBJECTS[label])
            verb = rng.choice(REQUEST_VERBS)
            template = verb + ' ' + noun
            family = 'contract-lexicon-' + str(TRAINING_OBJECTS[label].index(noun))
        if label == 'search' and topics and rng.random() < .30:
            symbol = query = rng.choice(topics)
            origin = 'codesearchnet-query-in-explicit-retrieval-request'
        text = wrap(template.format(s=symbol, p=path), role, rng)
        if '{s}' not in template and '{p}' not in template:
            # Keep no-slot requests distinct without adding evidence or source bodies.
            text += rng.choice([' in the project containing ', ' for the collection containing ']) + path
        key = normalized(text)
        if key in seen or len(text.encode('utf-8')) > 4096:
            continue
        seen.add(key)
        rows.append({'text': text, 'intent': label, 'role': role, 'split': split,
                     'family': label + '/' + family, 'origin': origin,
                     'repository': repo['repository'], 'revision': repo['revision'],
                     'vocabulary_sha256': repo['vocabulary_sha256'],
                     **({'query_sha256': hashlib.sha256(query.encode()).hexdigest(),
                         'query_source': 'codesearchnet-queries'} if query else {})})
    return rows


def public_negatives(clinc, split, count):
    # Generic assistant self-description can mean AIRK help: exclude rather than
    # imposing unrelated benchmark labels on that ambiguous boundary.
    excluded = {'what_can_i_ask_you', 'who_do_you_work_for', 'what_is_your_name',
                'are_you_a_bot', 'how_old_are_you', 'where_are_you_from'}
    source_split = 'train' if split == 'train' else 'test' if split == 'test' else 'val'
    membership = {}
    for official in ('train', 'val', 'test'):
        for text, _ in clinc[official]:
            membership.setdefault(normalized(text), set()).add(official)
    rows = []
    for text, label in clinc[source_split]:
        key = hashlib.sha256(normalized(text).encode()).hexdigest()
        if label in excluded or len(text.encode()) > 4096 or len(membership[normalized(text)]) != 1:
            continue
        if split in ('validation', 'calibration') and (int(key[:8], 16) % 2 == 0) != (split == 'validation'):
            continue
        rows.append({'text': text, 'intent': 'unsupported', 'role': 'public-assistant-task',
                     'split': split, 'origin': 'clinc150-non-repository-task',
                     'source_split': source_split, 'source_intent': label, 'source_id': key})
    rows.sort(key=lambda row: row['source_id'])
    return rows[:count]


def check_splits(splits):
    keys = {}
    families = {}
    for split, rows in splits.items():
        for row in rows:
            key = normalized(row['text'])
            if key in keys and keys[key] != split:
                raise ValueError(f"Normalized utterance overlaps {keys[key]} and {split}; origin {row['origin']}")
            keys[key] = split
            family = row.get('family')
            if family:
                if family in families and families[family] != split:
                    raise ValueError('Phrase family overlaps dataset splits')
                families[family] = split


def scores(rows, weights, bias):
    return np.asarray(matrix([row['text'] for row in rows]) @ weights.astype(np.int32)) + bias


def metrics(rows, values, threshold=0, margin=80):
    truth = np.array([LABELS.index(row['intent']) if row['intent'] in LABELS else -1 for row in rows])
    supported = truth >= 0
    predicted = values.argmax(axis=1)
    ordered = np.sort(values, axis=1)
    accepted = (ordered[:, -1] >= threshold) & (ordered[:, -1] - ordered[:, -2] >= margin)
    correct = predicted == truth
    return {'rows': len(rows), 'supported_rows': int(supported.sum()),
            'intent_accuracy': float(correct[supported].mean()) if supported.any() else None,
            'accepted_precision': float(correct[accepted].mean()) if accepted.any() else None,
            'supported_ask_rate': float((~accepted)[supported].mean()) if supported.any() else None,
            'unsupported_accepted': int((accepted & ~supported).sum()),
            'unsupported_accept_rate': float(accepted[~supported].mean()) if (~supported).any() else None,
            'wrong_accepted': int((accepted & ~correct).sum()),
            'per_role': {role: float(correct[mask].mean()) for role in ROLES
                         if (mask := supported & np.array([row['role'] == role for row in rows])).any()}}


def calibrate(rows, values):
    truth = np.array([LABELS.index(row['intent']) if row['intent'] in LABELS else -1 for row in rows])
    positive = truth >= 0
    ordered = np.sort(values, axis=1)
    top, gap = ordered[:, -1], ordered[:, -1] - ordered[:, -2]
    correct = values.argmax(axis=1) == truth
    candidates = []
    for threshold in np.unique(np.quantile(top, np.linspace(0, 1, 61)).astype(int)):
        for margin in np.unique(np.quantile(gap, np.linspace(0, 1, 61)).astype(int)):
            accepted = (top >= threshold) & (gap >= margin)
            if not accepted.any():
                continue
            precision = correct[accepted].mean()
            false_accept = accepted[~positive].mean()
            if precision >= .995 and false_accept <= .005:
                candidates.append((float(accepted[positive].mean()), float(precision), int(threshold), int(margin)))
    if not candidates:
        return int(top.max()) + 1, 0
    _, _, threshold, margin = max(candidates)
    return threshold, margin


def asset(weights, bias, threshold, margin):
    payload = weights.tobytes() + bias.tobytes()
    return b'AIRKINT1' + struct.pack('<IIIIii', 1, 65536, 13, 1, threshold, margin) + hashlib.sha256('\n'.join(LABELS).encode()).digest() + hashlib.sha256(payload).digest() + payload


def save_report(report, prefix):
    content = json.dumps(report, indent=2) + '\n'
    digest = hashlib.sha256(content.encode()).hexdigest()
    file = WORK / 'reports' / (prefix + '-' + digest[:16] + '.json')
    file.parent.mkdir(parents=True, exist_ok=True)
    if not file.exists():
        file.write_text(content, encoding='utf-8')
    retain(file)
    return file


def evaluate(file):
    report = json.loads(file.read_text(encoding='utf-8'))
    model = ROOT / report['model_path']
    content = model.read_bytes()
    if hashlib.sha256(content).hexdigest() != report['asset_sha256']:
        raise ValueError('Frozen model digest changed')
    weights = np.frombuffer(content, dtype=np.int8, offset=96, count=65536 * 13).reshape(65536, 13)
    bias = np.frombuffer(content, dtype='<i4', offset=96 + 65536 * 13, count=13)
    threshold, margin = report['threshold'], report['margin']
    frozen = json.loads((HERE / 'qualification.json').read_text(encoding='utf-8'))
    training_record = report['datasets']['train']
    training_bytes = (ROOT / training_record['path']).read_bytes()
    if hashlib.sha256(training_bytes).hexdigest() != training_record['sha256']:
        raise ValueError('Frozen training corpus changed')
    training_texts = {normalized(json.loads(line)['text']) for line in training_bytes.splitlines()}
    overlaps = {}
    evaluation = {}
    for name in ('actions/runner', 'simpleiot/simpleiot', 'fastapi/fastapi'):
        record = frozen['datasets'][name]
        raw = (ROOT / record['path']).read_bytes()
        if hashlib.sha256(raw).hexdigest() != record['sha256']:
            raise ValueError('Frozen repository evaluation changed')
        rows = [json.loads(line) for line in raw.splitlines()]
        overlaps[name] = sum(normalized(row['text']) in training_texts for row in rows)
        evaluation[name] = metrics(rows, scores(rows, weights, bias), threshold, margin)
    fixtures = {}
    for name in ('evaluation', 'safety'):
        fixture = HERE / (name + '.json')
        if hashlib.sha256(fixture.read_bytes()).hexdigest() != report['frozen_fixture_sha256'][name]:
            raise ValueError('Frozen regression fixture changed')
        rows = json.loads(fixture.read_text(encoding='utf-8'))
        overlaps[name] = sum(normalized(row['text']) in training_texts for row in rows)
        if overlaps[name]:
            raise ValueError('Retained utterance/safety fixture overlaps fitted text')
        if name == 'safety':
            rows = [{**row, 'intent': 'unsupported', 'role': 'safety'} for row in rows]
        fixtures[name] = metrics(rows, scores(rows, weights, bias), threshold, margin)
    record = report['datasets']['public-test']
    raw = (ROOT / record['path']).read_bytes()
    if hashlib.sha256(raw).hexdigest() != record['sha256']:
        raise ValueError('Frozen public evaluation changed')
    rows = [json.loads(line) for line in raw.splitlines()]
    public = metrics(rows, scores(rows, weights, bias), threshold, margin)
    calibration = report['calibration']
    gates = {'repository_accuracy': all(r['intent_accuracy'] >= .95 for r in evaluation.values()),
             'role_accuracy': all(all(v >= .90 for v in r['per_role'].values()) for r in evaluation.values()),
             'independent_150': fixtures['evaluation']['intent_accuracy'] >= .92,
             'curated_unsupported': fixtures['safety']['unsupported_accepted'] == 0,
             'public_unsupported': public['unsupported_accept_rate'] <= .01,
             'calibration_accepted_precision': (calibration['accepted_precision'] or 0) >= .995,
             'supported_ask_rate': calibration['supported_ask_rate'] <= .10}
    report.update(repository_evaluation=evaluation, regression_fixtures=fixtures, public_unsupported=public,
                  training_text_overlap_audit=overlaps,
                  gates=gates, qualified=all(gates.values()),
                  evaluation_scope='Existing repository/150/safety fixtures are retained regression tests, not fresh unseen tests. CLINC official test rows were not used to fit or calibrate. Agent-authored intent labels still require review.')
    destination = save_report(report, 'broader-qualified')
    (HERE / ('qualification-broader-v2.json' if report['protocol'] == 'broader-phrasing/2' else 'qualification-broader.json')).write_text(json.dumps(report, indent=2) + '\n', encoding='utf-8')
    print(json.dumps({'report': str(destination), 'gates': gates, 'regression_150': fixtures['evaluation'],
                      'safety': fixtures['safety'], 'public': public}), flush=True)


def main():
    parser = argparse.ArgumentParser()
    parser.add_argument('--evaluate', type=Path, help='Evaluate an already frozen development report; no fitting or threshold changes')
    args = parser.parse_args()
    if args.evaluate:
        evaluate(args.evaluate)
        return
    repos = [repo for repo in vocabulary() if repo['split'] == 'train']
    queries, clinc = online()
    splits = {}
    for split, positive, negative, public in [('train', 40000, 6000, 2000), ('validation', 4000, 600, 400), ('calibration', 4000, 600, 400)]:
        splits[split] = synthetic(positive, repos, split, queries) + synthetic(negative, repos, split, queries, True) + public_negatives(clinc, split, public)
    splits['public-test'] = public_negatives(clinc, 'test', 1000)
    check_splits(splits)
    datasets = {split: save_jsonl('broader-' + split, rows) for split, rows in splits.items()}
    print(json.dumps({'saved': {split: len(rows) for split, rows in splits.items()}}), flush=True)
    train, validation, calibration = splits['train'], splits['validation'], splits['calibration']
    x = matrix([row['text'] for row in train])
    y = np.array([[int(row['intent'] == label) for label in LABELS] for row in train])
    val_x = matrix([row['text'] for row in validation])
    learning = []
    selected = None
    for count in (10000, 20000, 40000):
        indices = list(range(count)) + list(range(40000, 40000 + count * 6000 // 40000)) + list(range(46000, 46000 + count * 2000 // 40000))
        for regularization in (0.5, 2.0):
            model = OneVsRestClassifier(LogisticRegression(C=regularization, max_iter=300, solver='liblinear', random_state=SEED), n_jobs=1)
            model.fit(x[indices], y[indices])
            weights, bias = quantize(model)
            values = np.asarray(val_x @ weights.astype(np.int32)) + bias
            result = metrics(validation, values)
            # Only development validation affects selection. Penalize unsafe
            # acceptance rather than selecting the most confident wrong model.
            quality = result['intent_accuracy'] - 2 * result['unsupported_accept_rate']
            entry = {'positive_training_rows': count, 'total_training_rows': len(indices), 'C': regularization, 'selection_score': quality, 'validation': result}
            learning.append(entry)
            print(json.dumps(entry), flush=True)
            if selected is None or quality > selected[0]:
                selected = (quality, count, regularization, weights, bias)
    _, count, regularization, weights, bias = selected
    values = scores(calibration, weights, bias)
    threshold, margin = calibrate(calibration, values)
    content = asset(weights, bias, threshold, margin)
    digest = hashlib.sha256(content).hexdigest()
    model_file = WORK / 'models' / ('intent-' + digest[:16] + '.bin')
    model_file.parent.mkdir(parents=True, exist_ok=True)
    if not model_file.exists():
        model_file.write_bytes(content)
    retain(model_file)
    report = {'schema': 1, 'protocol': 'broader-phrasing/2', 'seed': SEED, 'datasets': datasets,
              'selected_positive_training_rows': count, 'selected_C': regularization,
              'selection': 'Maximum validation intent accuracy minus twice unsupported acceptance at fixed development thresholds; ties retain the earlier smaller candidate. Calibration and final fixtures do not select the model.',
              'learning_curve': learning, 'threshold': threshold, 'margin': margin,
              'calibration': metrics(calibration, values, threshold, margin),
              'asset_sha256': digest, 'asset_bytes': len(content), 'model_path': model_file.relative_to(ROOT).as_posix(),
              'online_lock_sha256': hashlib.sha256((HERE / 'online.lock.json').read_bytes()).hexdigest(),
              'phrasing_sha256': hashlib.sha256((HERE / 'phrasing.py').read_bytes()).hexdigest(),
              'frozen_fixture_sha256': {name: hashlib.sha256((HERE / (name + '.json')).read_bytes()).hexdigest() for name in ('evaluation', 'safety')},
              'runtime_promoted': False, 'scope': 'Intent classification with original phrase-family splits, query-wrapped CodeSearchNet topics, and explicitly unsupported software tasks and CLINC assistant requests. No source bodies or workspace sessions are training inputs.'}
    destination = save_report(report, 'broader-development')
    print(json.dumps({'frozen_report': destination.relative_to(ROOT).as_posix(), 'calibration': report['calibration']}), flush=True)


if __name__ == '__main__':
    main()
