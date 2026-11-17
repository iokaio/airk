# SPDX-License-Identifier: Apache-2.0
import hashlib
import json
from pathlib import Path
import subprocess

base = Path(__file__).parent
data = json.loads((base / 'replay-1.8.4-ledger.json').read_text(encoding='utf-8'))
calls = data['calls']
hashes = []
def walk(value):
    if isinstance(value, dict):
        if 'path' in value and 'content_hash' in value:
            path = Path(value['path'])
            hashes.append({'path': str(path), 'expected': value['content_hash'], 'actual': hashlib.sha256(path.read_bytes()).hexdigest()})
        for child in value.values():
            walk(child)
    elif isinstance(value, list):
        for child in value:
            walk(child)
for call in calls:
    walk(call['parsed'])

def refs(prefix, scope):
    hits = [hit for c in calls if (c['id'] == prefix or c['id'].startswith(prefix + '.')) and not c['raw'].get('isError') for hit in c['parsed'].get('evidence', {}).get('hits', [])]
    got = [(h['citation']['path'], h['citation']['start_line']) for h in hits]
    run = subprocess.run(['rg', '-n', '--no-heading', '-w', 'Session', scope], capture_output=True, text=True, encoding='utf-8', check=True)
    oracle = []
    for line in run.stdout.splitlines():
        if Path(scope).is_file():
            number = line.split(':', 1)[0]
            oracle.append((scope, int(number)))
        else:
            path, number, _ = line.split(':', 2)
            oracle.append((path.replace('\\', '/'), int(number)))
    return {'returned': len(got), 'unique': len(set(got)), 'oracle': len(oracle), 'missing': sorted(set(oracle)-set(got)), 'extra': sorted(set(got)-set(oracle))}

result = {'calls': len(calls), 'errors': [{'id': c['id'], 'error': c['parsed'].get('error')} for c in calls if c['raw'].get('isError')], 'hash_observations': len(hashes), 'unique_files': len({h['path'] for h in hashes}), 'hash_mismatches': [h for h in hashes if h['actual'] != h['expected']], 'documentation_references': refs('D5.references', 'docs/mcp-recommendations.md'), 'source_references': refs('S4.references', 'crates/airk-cli/src/'), 'mcp_references': refs('S1.references', 'crates/airk-mcp/src/response.rs')}
result['headings'] = sum(len(c['parsed'].get('items', [])) for c in calls if c['id'].startswith('D2.headings'))
result['live_headings'] = sum(line.startswith('#') for line in Path('docs/mcp-recommendations.md').read_text(encoding='utf-8').splitlines())
result['outline_records'] = sum(len(c['parsed'].get('items', [])) for c in calls if c['id'].startswith('D7.outline'))
result['contains_records'] = sum(len(c['parsed'].get('items', [])) for c in calls if c['id'].startswith('S2.contains'))
result['tiny_symbols'] = sum(len(c['parsed'].get('items', [])) for c in calls if c['id'].startswith('S9.tiny'))
(base / 'replay-1.8.4-validation.json').write_text(json.dumps(result, indent=2) + '\n', encoding='utf-8')
print(json.dumps(result, indent=2))
