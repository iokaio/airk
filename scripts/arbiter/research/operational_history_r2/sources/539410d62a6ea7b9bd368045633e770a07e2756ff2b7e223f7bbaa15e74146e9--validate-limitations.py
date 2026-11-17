# SPDX-License-Identifier: Apache-2.0
import hashlib
import json
from pathlib import Path
import subprocess

base = Path(__file__).parent
data = json.loads((base / 'limitations-1.8.4-ledger.json').read_text(encoding='utf-8'))
calls = data['calls']
observations = []
def walk(value):
    if isinstance(value, dict):
        if 'path' in value and 'content_hash' in value:
            observations.append({'path': value['path'], 'matches': hashlib.sha256(Path(value['path']).read_bytes()).hexdigest() == value['content_hash']})
        for child in value.values():
            walk(child)
    elif isinstance(value, list):
        for child in value:
            walk(child)
for c in calls:
    walk(c['parsed'])
walk(data['setup'])

errors = [c for c in calls if c['raw'].get('isError')]
retries = [c for c in calls if c['id'].endswith('.retry')]
by_id = {c['id']: c for c in calls}
for c in retries:
    original = by_id[c['id'].removesuffix('.retry')]
    error = original['parsed']['error']
    assert error['query_submitted'] is False and error['retryable'] is True
    assert original['args'] == c['args']

oracle = [int(line.split(':', 1)[0]) for line in subprocess.check_output(['rg', '-n', '-w', 'Session', 'docs/mcp-recommendations.md'], encoding='utf-8').splitlines()]
paging = []
for round in range(1, 4):
    prefix = f'P.r{round}.D5.references.'
    got = [h['citation']['start_line'] for c in calls if c['id'].startswith(prefix) and not c['raw'].get('isError') for h in c['parsed']['evidence']['hits']]
    paging.append({'round': round, 'returned': len(got), 'unique': len(set(got)), 'missing': sorted(set(oracle)-set(got)), 'extra': sorted(set(got)-set(oracle))})

result = {'calls': len(calls), 'successes': len(calls)-len(errors), 'errors': [{'id': c['id'], 'error': c['parsed']['error']} for c in errors], 'retries': len(retries), 'retry_errors': sum(bool(c['raw'].get('isError')) for c in retries), 'hash_observations': len(observations), 'unique_files': len({o['path'] for o in observations}), 'hash_mismatches': [o for o in observations if not o['matches']], 'paging_oracle': paging, 'groups': []}
for prefix in ['A.', 'P.', 'N.', 'B.']:
    rows = [c for c in calls if c['id'].startswith(prefix)]
    result['groups'].append({'prefix': prefix, 'calls': len(rows), 'initial': sum(not c['id'].endswith('.retry') for c in rows), 'errors': sum(bool(c['raw'].get('isError')) for c in rows)})
result['navigation'] = [{'id': c['id'], 'items': len(c['parsed'].get('items', [])), 'targets': [t['target'] for t in c['parsed'].get('suggested_targets', [])], 'truncated': c['parsed'].get('truncated')} for c in calls if c['id'].startswith('N.')]
(base / 'limitations-1.8.4-validation.json').write_text(json.dumps(result, indent=2)+'\n', encoding='utf-8')
print(json.dumps({k:v for k,v in result.items() if k not in ('errors', 'navigation')}, indent=2))
