# SPDX-License-Identifier: Apache-2.0
"""Fixed TRAIN-only parent bags from actual Rust unit/tensor output; no model."""
from pathlib import Path
import hashlib
import json
import types
import collections

def require_private_root(value):
    if not value:
        raise ValueError('explicit private root required')
    root = next(p for p in Path(__file__).resolve().parents if (p/'Cargo.toml').is_file() and (p/'AGENTS.md').is_file())
    private = Path(value).resolve()
    if not private.is_relative_to(root/'artifacts'):
        raise ValueError('private root must remain beneath repository artifacts; tracked or external roots refused')
    return private


ROOT = next(p for p in Path(__file__).resolve().parents if (p/'Cargo.toml').is_file() and (p/'AGENTS.md').is_file())
MODELS = Path(__import__('os').environ.get('AIRK_RESEARCH_MODELS', str(ROOT/'artifacts/advanced-split/models'))).resolve()
HERE=Path(__import__('os').environ.get('AIRK_RESEARCH_MODELS', str((next(p for p in Path(__file__).resolve().parents if (p/'Cargo.toml').is_file() and (p/'AGENTS.md').is_file()))/'artifacts/advanced-split/models'))).resolve()/'native-parent-bag-candidate-r1'

def sha(raw):
    return hashlib.sha256(raw).hexdigest()

def read(path, pin):
    raw = path.read_bytes()
    if sha(raw) != pin:
        raise ValueError('fixed input drift: ' + str(path))
    return raw

def overlaps(unit, parent):
    return unit['file'] == parent['file'] and any(
        member['span']['start'] < parent['byte_end'] and member['span']['end'] > parent['byte_start']
        for member in unit['members'])

def build(rows, known, native):
    units = {u['id']: u for u in native['units']}
    parents = {p['parent']['id']: p for p in native['parents']}
    if len(units) != len(native['units']) or len(parents) != 1789 or len(rows) != 315:
        raise ValueError('fixed complete native/master census required')
    projected = []
    for ordinal, row in enumerate(rows):
        positive, negative = parents[row['positive']], parents[row['distractor']]
        alternatives = sorted(known[row['task_id']])
        if row['positive'] not in alternatives or row['distractor'] in alternatives:
            raise ValueError('original positive/negative parent identity changed')
        known_parents = [parents[p]['parent'] for p in alternatives]
        known_keys = {units[u]['embedding_key'] for p in alternatives for u in parents[p]['unit_ids']}
        def eligible_bag(parent):
            ids = sorted(parent['unambiguous_admitted_unit_ids'])
            if any(units[u]['embedding_admission'] != 'embeddable' or units[u]['context_parent_mapping_ambiguous']
                   or not units[u]['body_within_admitted_training_source'] for u in ids):
                raise ValueError('native conservative unit admission changed')
            return ids
        pos_ids, neg_ids = eligible_bag(positive), eligible_bag(negative)
        reasons = []
        if not positive['unambiguous_union_complete_annotation'] or not pos_ids:
            reasons.append('positive_parent_not_fully_covered_by_conservative_native_bag')
        if not negative['unambiguous_union_complete_annotation'] or not neg_ids:
            reasons.append('fixed_negative_parent_not_fully_covered_by_conservative_native_bag')
        if any(units[u]['embedding_key'] in known_keys or any(overlaps(units[u], p) for p in known_parents)
               for u in neg_ids):
            reasons.append('fixed_negative_overlaps_original_known_positive')
        projected.append({'ordinal': ordinal, 'original_row': row, 'known_positive_parent_ids': alternatives,
            'positive_unit_ids': pos_ids, 'negative_unit_ids': neg_ids,
            'status': 'REFUSED' if reasons else 'ADMITTED', 'refusal_reasons': reasons})
    # Freeze every cross-row parent mask now; no loss-time mining or replacements.
    all_bags = {}
    for row in projected:
        if row['status'] == 'ADMITTED':
            for key, ids in [('positive', row['positive_unit_ids']), ('distractor', row['negative_unit_ids'])]:
                parent_id = row['original_row'][key]
                if parent_id in all_bags and all_bags[parent_id] != ids:
                    raise ValueError('same parent has inconsistent unit bag')
                all_bags[parent_id] = ids
    for row in projected:
        alternatives = row['known_positive_parent_ids']
        known_parents = [parents[p]['parent'] for p in alternatives]
        known_keys = {units[u]['embedding_key'] for p in alternatives for u in parents[p]['unit_ids']}
        row['masked_parent_ids'] = sorted(parent_id for parent_id, ids in all_bags.items()
            if parent_id in alternatives or any(units[u]['embedding_key'] in known_keys or
                any(overlaps(units[u], p) for p in known_parents) for u in ids))
    return projected, all_bags

def main():
    require_private_root(__import__('os').environ.get('AIRK_RESEARCH_MODELS'))
    for name in ('HERE','PREP','H','MODELS'):
     if name in globals():require_private_root(globals()[name])
    native_raw = read(MODELS/'native-parent-bag-units-r4/units.private.json',
        '2365e2cac155eed949eaad71e273a077597c985f5d16ad366b05af4d7a630d5e')
    rows_raw = read(MODELS/'a1-visible-prefix-mining-candidate-r1/candidate-training-rows.private.json',
        '314825dccad822874d3405305df789317c8e90a0a0e4cdcdb590acd27a555c48')
    corpus_raw = read(MODELS/'human-corpus-r1/corpus.private.json',
        '6e9d8ade6f799ed2a5e1903de69482db9fcd3cb1731a5fee4a504adede5fca6f')
    requests_raw = read(MODELS/'requests-train-rights/pairs.private.json',
        '916b398de0fc9774041fe174d0e585c862bc68e9aa18fa04028c3126f693a18f')
    helper = Path(__file__).with_name('prepare_sources.py')
    helper_raw = read(helper, 'ecc967fd448d5034d308b7911da13363a3606306a505959a9fe9964b0afc078f')
    module = types.ModuleType('_captured_train_source_owner')
    module.__file__ = str(helper)
    exec(compile(helper_raw, str(helper), 'exec'), module.__dict__)
    corpus = module.train_reader().train_only_corpus(corpus_raw)
    tasks = corpus['tasks'] + json.loads(requests_raw)['tasks']
    known = {}
    for task in tasks:
        if task['split'] != 'train' or task['dataset'] not in ('QASPER', 'requests-original-docstrings'):
            raise ValueError('held-out task reached TRAIN bag preparation')
        known[task['id']] = {location for alt in task['human_alternatives']
            if not alt['unanswerable'] and alt['alignment'] == 'all_original_evidence_maps_to_allowed_text'
            for evidence in alt['evidence'] for location in evidence['allowed_locations']}
    rows, bags = build(json.loads(rows_raw), known, json.loads(native_raw))
    target = HERE/'master.private.json'
    if target.exists():
        raise ValueError('never overwrite frozen bag master')
    result = {'schema':'airk.private-native-parent-bag-master/1', 'rows':rows, 'bags':bags,
        'native_units_sha256':sha(native_raw), 'fixed_parent_master_sha256':sha(rows_raw),
        'all_rows':315, 'admitted_rows':sum(r['status']=='ADMITTED' for r in rows),
        'refusal_counts':dict(collections.Counter(reason for r in rows for reason in r['refusal_reasons'])),
        'training_performed':False, 'weak_parent_bags':True, 'fragment_positive_labels':False}
    target.write_text(json.dumps(result,indent=2)+'\n', encoding='utf-8')
    print(json.dumps({'master_sha256':sha(target.read_bytes()), 'all_rows':315,
        'admitted_rows':result['admitted_rows'], 'refusals':result['refusal_counts'], 'bags':len(bags)}))

if __name__ == '__main__':
    main()
