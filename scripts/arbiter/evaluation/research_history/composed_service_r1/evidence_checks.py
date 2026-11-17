# SPDX-License-Identifier: Apache-2.0
"""Private evidence checks. This module grants no qualification authority."""
import collections
import hashlib
import json
import math
from pathlib import Path

REQUIRED_CASES = ('baseline_identity', 'focus_only_search', 'dual_compact', 'dual_large',
                  'noninterference', 'cancel_drain', 'source_revoke', 'remove_recover',
                  'stage_independence', 'normal_shutdown')

def canonical(value):
    return json.dumps(value, sort_keys=True, ensure_ascii=False, separators=(',', ':'), allow_nan=False)

def artifact(reference):
    path = Path(reference['path'])
    data = path.read_bytes()
    if hashlib.sha256(data).hexdigest() != reference['sha256']:
        raise ValueError('evidence artifact drift: ' + str(path))
    return data

def search_preservation(before, after):
    """Both arguments are FULL canonical SearchResponse objects, not UI projections."""
    left, right = dict(before), dict(after)
    left.pop('focus', None)
    observation = right.pop('focus', None)
    if not isinstance(observation, dict) or observation.get('kind') != 'receipt':
        raise ValueError('actual Focus receipt absent')
    a, b = left.pop('hits'), right.pop('hits')
    if canonical(left) != canonical(right):
        raise ValueError('canonical nonranking fields changed')
    if collections.Counter(map(canonical, a)) != collections.Counter(map(canonical, b)):
        raise ValueError('full evidence/source hit multiset changed')
    receipt = observation['receipt']
    if receipt.get('applied') is not True or not receipt.get('identity') or receipt.get('scored_count', 0) < 1:
        raise ValueError('receipt did not apply actual identified Focus scoring')
    original, ranked = receipt['original_order'], receipt['ranked_order']
    if len(original) != len(a) or len(set(original)) != len(original) or sorted(original) != sorted(ranked):
        raise ValueError('native Focus roster is not an exact permutation')
    expected = [a[original.index(identity)] for identity in ranked]
    if canonical(expected) != canonical(b):
        raise ValueError('returned order differs from actual native receipt')
    return {'full_canonical_preservation': True, 'hits': len(a), 'receipt': receipt}

def observed_race(events, mutation, *, profile_manifest, question_utf8_sha256, dispatch_identity):
    """Admitted-dispatch plus successful return, never a literal in-FFI assertion."""
    related = [e for e in events if e.get('profile_manifest') == profile_manifest
               and e.get('question_utf8_sha256') == question_utf8_sha256
               and e.get('dispatch_identity') == dispatch_identity]
    dispatches = [e for e in related if e['kind'] == 'dispatch_admitted']
    if len(dispatches) != 1:
        raise ValueError('exactly one original dispatch required; no retries')
    dispatch = dispatches[0]
    returns = [e for e in related if e['kind'] == 'score_returned'
               and e['input_digest'] == dispatch['input_digest']]
    if len(returns) != 1:
        raise ValueError('actual original score completion absent')
    returned = returns[0]
    for event in (dispatch, returned):
        if not event.get('charge_identity') or not isinstance(event.get('charge_bytes'), int) or event['charge_bytes'] <= 0:
            raise ValueError('actual resident reservation identity absent')
    if not (dispatch['unix_ms'] <= mutation['started_unix_ms']
            <= mutation['completed_unix_ms'] < returned['unix_ms']):
        raise ValueError('mutation did not complete during observed original score interval')
    if returned['outcome'] != 'success' or returned['finite_raw_scores'] is not True:
        raise ValueError('successful finite native completion absent')
    if not returned.get('raw_scores') or any(isinstance(x, bool) or not isinstance(x, (int, float)) or not math.isfinite(x) for x in returned['raw_scores']):
        raise ValueError('actual finite raw score vector absent')
    if returned['work']['input_digest'] != dispatch['input_digest'] or returned['work']['input_tokens'] != dispatch['tokens']:
        raise ValueError('actual returned native work differs from original compiled input')
    if dispatch['original_deadline_unix_ms'] != returned['original_deadline_unix_ms']:
        raise ValueError('original deadline changed')
    releases = [e for e in related if e['kind'] == 'charge_released'
                and e['input_digest'] == dispatch['input_digest']]
    if len(releases) != 1 or releases[0]['sequence'] <= returned['sequence']:
        raise ValueError('actual post-return charge release absent')
    settled = [e for e in related if e['kind'] == 'job_settled' and e['input_digest'] == dispatch['input_digest']]
    if len(settled) != 1 or not returned['sequence'] < settled[0]['sequence'] < releases[0]['sequence']:
        raise ValueError('actual compute/delivery/permit settlement absent')
    for event in (returned, settled[0], releases[0]):
        for field in ('charge_identity', 'charge_bytes', 'scope', 'task', 'original_deadline_unix_ms'):
            if event[field] != dispatch[field]:
                raise ValueError('original native charge/scope/task identity changed: ' + field)
    return {'dispatch': dispatch, 'mutation': mutation, 'return': returned, 'release': releases[0],
            'scope': 'admitted dispatch through successful native return, not literal inside-FFI observation'}

def finite_resources(snapshot):
    for key in ('observed_rss_bytes', 'memory_reserved_bytes', 'memory_limit_bytes'):
        value = snapshot[key]
        if isinstance(value, bool) or not isinstance(value, (int, float)) or not math.isfinite(value) or value < 0:
            raise ValueError('invalid actual resource measurement: ' + key)
    if snapshot.get('telemetry_complete') is not True:
        raise ValueError('actual resource telemetry incomplete')
    if snapshot['memory_reserved_bytes'] > snapshot['memory_limit_bytes']:
        raise ValueError('native admission reservation exceeded')
    return snapshot

def validate_receipt(receipt):
    if receipt.get('schema') != 'airk.private.actual-composed-service/1' or receipt.get('proof_class') != 'ActualComposed':
        raise ValueError('wrong proof class')
    cases = receipt['cases']
    if set(cases) != set(REQUIRED_CASES):
        raise ValueError('required case roster differs')
    for name, case in cases.items():
        if case['verdict'] != 'pass' or not case['evidence']:
            raise ValueError('composed acceptance incomplete: ' + name)
        for reference in case['evidence']:
            artifact(reference)
    for field in ('profiles', 'fixture_binding', 'owned_execution', 'source_seals', 'normal_shutdown'):
        if not receipt.get(field):
            raise ValueError('required authentic evidence absent: ' + field)
    if len(receipt['profiles']) != 3:
        raise ValueError('exactly three authentic profiles required')
    # The native final promoter independently interprets every proof. This checker
    # intentionally cannot upgrade arbitrary evidence references into qualification.
    return {'structural_completeness': True, 'qualification_granted': False}
