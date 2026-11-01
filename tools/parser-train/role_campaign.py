# SPDX-License-Identifier: Apache-2.0
"""Joint whole-family splits and original role coverage for protocol 5.

Only pinned public vocabulary and original fictional templates enter expansion.
Saved split authority covers baseline, conversational and role families together;
identical templates cannot move between splits through another source or role.
"""
from collections import Counter, defaultdict
from functools import lru_cache
import hashlib
import json
import random
import re
from pathlib import PurePosixPath

from conversational_v4 import CONVERSATIONAL, KINDS, TOPICS, UNSUPPORTED
from phrasing import EXTRA_NEGATIVES, NEGATIVES, PHRASES, PREFIXES, REQUEST_VERBS, TRAINING_OBJECTS
from qualify import MORE_TEMPLATES, ROLES, TEMPLATES
from train import LABELS, ROOT

HERE = ROOT / 'tools/parser-train'
SOURCE = HERE / 'role-queries.json'
SPLIT_MAP = HERE / 'role-phrase-splits-v5.json'
SEED = 84183
TOOLS = {'find_definition': 'symbols', 'facts': 'document_facts'}


def write_if_changed(path, content):
    if not path.exists() or path.read_text(encoding='utf-8') != content:
        path.write_text(content, encoding='utf-8')


def canonical(text):
    value = re.sub(r'\s+', ' ', text.strip().lower()).rstrip('.?!')
    prefixes = sorted({prefix.lower() for prefix in PREFIXES if prefix} |
                      {prefix.lower() for values in ROLES.values() for prefix in values}, key=len, reverse=True)
    while (prefix := next((prefix for prefix in prefixes if value.startswith(prefix)), None)):
        value = value[len(prefix):].strip()
    return value.replace('{baseline}', '{token}')


def template_keys(row):
    text = row['text']
    if row['intent'] == 'changes':
        text = text.replace('{s}', '{token}')
    return {canonical(text.replace('{k}', kind)) for kind in KINDS} if '{k}' in text else {canonical(text)}


def bank():
    source = json.loads(SOURCE.read_text(encoding='utf-8'))
    source['source_counts'] = dict(Counter(row['role'] for row in source['queries']))
    rows = []
    for index, row in enumerate(source['queries']):
        rows.append({**row, 'id': f'role/{index}', 'origin': 'original-role-inventory'})
    def add(intent, family, variants):
        for variant, text in enumerate(variants):
            rows.append({'id': f'{family}/{variant}', 'intent': intent, 'family': family,
                         'text': text, 'origin': 'original-baseline-family',
                         'expected_tools': [] if intent == 'unsupported' else [TOOLS.get(intent, intent)]})
    for label in LABELS:
        for family, variants in PHRASES[label]:
            add(label, f'baseline/{label}/phrase/{family}', variants)
        for index, text in enumerate(TEMPLATES[label][:4] + MORE_TEMPLATES[label]):
            add(label, f'baseline/{label}/legacy/{index}', [text])
        for index, noun in enumerate(TRAINING_OBJECTS[label]):
            add(label, f'baseline/{label}/contract/{index}', [verb + ' ' + noun for verb in REQUEST_VERBS])
        for split, families in CONVERSATIONAL.get(label, {}).items():
            for family, variants in families:
                add(label, f'conversational/{label}/{split}/{family}', variants)
    for family, variants in NEGATIVES + EXTRA_NEGATIVES:
        add('unsupported', 'baseline/unsupported/' + family, variants)
    for split, families in UNSUPPORTED.items():
        for family, variants in families:
            add('unsupported', f'conversational/unsupported/{split}/{family}', variants)
    return source, rows


@lru_cache(maxsize=1)
def prepare():
    source, rows = bank()
    parent = {row['family']: row['family'] for row in rows}
    def find(key):
        while parent[key] != key:
            parent[key] = parent[parent[key]]
            key = parent[key]
        return key
    duplicates = []
    seen = {}
    for row in rows:
        for key in template_keys(row):
            if key in seen:
                prior = seen[key]
                if prior['intent'] != row['intent']:
                    raise ValueError(f"Contradictory template labels: {prior['id']} and {row['id']}")
                first, second = find(prior['family']), find(row['family'])
                if first != second:
                    parent[max(first, second)] = min(first, second)
                    duplicates.append({'first': prior['id'], 'second': row['id'], 'canonical_template': key})
            else:
                seen[key] = row
    components = defaultdict(list)
    for family in parent:
        components[find(family)].append(family)
    groups = {}
    assignments = {}
    for representative, members in components.items():
        digest = hashlib.sha256(('joint-role-family-v5:84183:' + '\n'.join(sorted(members))).encode()).hexdigest()
        group = 'joint-v5/' + digest[:16]
        groups[representative] = group
        value = int(digest[:8], 16) % 100
        assignments[group] = 'train' if value < 70 else 'validation' if value < 85 else 'calibration'
    for row in rows:
        row['group'] = groups[find(row['family'])]
    moves = []
    # Cover each authored role/intent in training without choosing partitions
    # from measured predictions. Shared components always move together.
    for role, intent in sorted({(row['role'], row['intent']) for row in rows if 'role' in row}):
        options = sorted({row['group'] for row in rows if row.get('role') == role and row['intent'] == intent})
        if not any(assignments[group] == 'train' for group in options):
            assignments[options[0]] = 'train'
            moves.append({'group': options[0], 'reason': 'train-role-intent', 'role': role, 'intent': intent})
    def preserves_train(group):
        affected = {(row['role'], row['intent']) for row in rows if row['group'] == group and 'role' in row}
        return all(any(row.get('role') == role and row['intent'] == intent and row['group'] != group
                       and assignments[row['group']] == 'train' for row in rows) for role, intent in affected)
    # Every role has material whole-family held-out coverage. Each primitive also
    # has a baseline family in each split, rather than unreported fallback rows.
    for role in ROLES:
        options = sorted({row['group'] for row in rows if row.get('role') == role})
        for split in ('validation', 'calibration'):
            while sum(assignments[group] == split for group in options) < 6:
                eligible = [group for group in options if assignments[group] == 'train' and preserves_train(group)]
                if not eligible:
                    raise ValueError('Insufficient independent role families for held-out coverage')
                group = eligible[0]
                assignments[group] = split
                moves.append({'group': group, 'reason': 'held-role-coverage', 'role': role, 'split': split})
    for label in LABELS + ['unsupported']:
        baseline = sorted({row['group'] for row in rows if row['intent'] == label and 'role' not in row})
        for split in ('train', 'validation', 'calibration'):
            if not any(assignments[group] == split for group in baseline):
                eligible = [group for group in baseline if assignments[group] == 'train' and preserves_train(group)
                            and sum(assignments[item] == 'train' for item in baseline) > 1]
                if not eligible:
                    raise ValueError('Insufficient independent primitive baseline families')
                assignments[eligible[0]] = split
                moves.append({'group': eligible[0], 'reason': 'primitive-coverage', 'intent': label, 'split': split})
    by_id = {}
    for row in rows:
        row['split'] = assignments[row['group']]
        by_id[row['id']] = row
    for index, row in enumerate(source['queries']):
        row['split'] = by_id[f'role/{index}']['split']
        row['split_group'] = by_id[f'role/{index}']['group']
    source['split_policy'] = 'Protocol-5 joint canonical-template union authority is role-phrase-splits-v5.json; all baseline/conversational/role family variants and duplicate templates share one group. Hash70/15/15 assignment and recorded label-only coverage moves precede fitting. Prior inventory-only split proposals are superseded.'
    write_if_changed(SOURCE, json.dumps(source, ensure_ascii=False, indent=2) + '\n')
    record = {'schema': 'airk.parser.role-phrase-splits/1', 'campaign': 'broader-phrasing/5', 'seed': SEED,
              'role_source_sha256': hashlib.sha256(SOURCE.read_bytes()).hexdigest(),
              'canonicalization': 'Lowercase/whitespace, trailing sentence punctuation and known polite/role wrappers removed; token/baseline aliases and kind-placeholder expansions unified.',
              'duplicate_family_links': duplicates, 'contradictory_labels': 0, 'coverage_moves': moves,
              'families': [{'family': family, 'group': groups[find(family)], 'split': assignments[groups[find(family)]]}
                           for family in sorted(parent)],
              'source_rows': len(rows), 'original_role_rows': len(source['queries'])}
    write_if_changed(SPLIT_MAP, json.dumps(record, ensure_ascii=False, indent=2) + '\n')
    return rows


def generated(count, repos, split, queries, negative=False, excluded=()):
    rows = prepare()
    pools = defaultdict(list)
    for row in rows:
        if row['split'] == split:
            pools[(row['intent'], row.get('role'))].append(row)
    rng = random.Random(SEED + {'train': 0, 'validation': 1, 'calibration': 2}[split] + (10 if negative else 0))
    topics = [query for query in queries
              if ('train' if (value := int(hashlib.sha256(re.sub(r'\s+', ' ', query.strip().lower()).encode()).hexdigest()[:8], 16) % 100) < 70 else 'validation' if value < 85 else 'calibration') == split]
    seen = set(excluded)
    result = []
    attempts = 0
    while len(result) < count:
        attempts += 1
        if attempts > count * 200:
            raise ValueError('Insufficient distinct examples for requested role corpus')
        label = 'unsupported' if negative else LABELS[len(result) % len(LABELS)]
        role = list(ROLES)[len(result) % len(ROLES)]
        role_pool = pools[(label, role)]
        role_request = bool(role_pool) and rng.random() < .70
        row = rng.choice(role_pool if role_request else pools[(label, None)])
        repo = rng.choice(repos)
        data = repo['data']
        symbol = rng.choice(data['symbols'] or data['headings'] or ['PacketStore'])
        path = rng.choice(data['paths'])
        token = 'airk-r1:' + '1' * 32 + ':' + str(rng.randrange(1, 100000)) + ':' + '2' * 64
        public_query = None
        if label == 'changes':
            symbol = token
        elif label == 'search' and topics and rng.random() < .30:
            symbol = public_query = rng.choice(topics)
        elif label == 'search' and rng.random() < .35:
            symbol = rng.choice(TOPICS)
        slots = {'s': symbol, 'p': path, 'scope': str(PurePosixPath(path).parent),
                 'k': rng.choice(KINDS), 'token': token, 'baseline': token}
        text = row['text'].format(**slots)
        if rng.random() < .25:
            text = rng.choice(ROLES[role]) + text
        else:
            text = rng.choice(PREFIXES) + text
        if not any('{' + key + '}' in row['text'] for key in slots):
            text += ' in the project containing ' + path
        key = re.sub(r'\s+', ' ', text.strip().lower())
        if key in seen or len(text.encode('utf-8')) > 4096:
            continue
        seen.add(key)
        result.append({'text': text, 'intent': label, 'role': role, 'split': split,
                       'family': row['group'], 'authored_family': row['family'], 'query_id': row['id'],
                       'origin': row['origin'], 'repository': repo['repository'], 'revision': repo['revision'],
                       'vocabulary_sha256': repo['vocabulary_sha256'], 'literal_slots': slots,
                       'expected_tools': row['expected_tools'],
                       'expected_target': (row.get('target') or '').format(**slots),
                       'label_provenance': 'Original agent-authored finite-tool intent; human review pending.',
                       **({'query_source': 'codesearchnet-queries', 'query_sha256': hashlib.sha256(public_query.encode()).hexdigest()} if public_query else {})})
    return result


def coverage(rows):
    return {'role_rows': dict(sorted(Counter(row['role'] for row in rows if row['origin'] == 'original-role-inventory').items())),
            'authored_role_families': dict(sorted(Counter(row['authored_family'] for row in rows if row['origin'] == 'original-role-inventory').items())),
            'source_query_rows': dict(sorted(Counter(row['query_id'] for row in rows if 'query_id' in row).items())),
            'public_negative_rows': sum(row['origin'] == 'clinc150-non-repository-task' for row in rows)}
