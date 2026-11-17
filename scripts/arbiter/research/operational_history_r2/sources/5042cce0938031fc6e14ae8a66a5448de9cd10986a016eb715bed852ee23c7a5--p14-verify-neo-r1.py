# SPDX-License-Identifier: Apache-2.0
"""Verify actual source-bound P14 Neo cases and mandatory cleanup."""
import hashlib
import json
from pathlib import Path
base = Path(__file__).resolve().parent
label = 'p14-native-cold-model-20261106-r1'
def read(suffix):
    return json.loads((base / (label + suffix)).read_text(encoding='utf-8'))
def sha(path):
    return hashlib.sha256(path.read_bytes()).hexdigest()
report, snapshot, plan, cleanup = (read(s) for s in ['-qualification.json', '-snapshot.json', '-plan.json', '-cleanup.json'])
source = {'sha256': 'd4e48a534881ea910bcbadd182e8fb4f6a970c844df6ab1951d7b566cbcb2eeb', 'files': 960}
assert report['status'] == 'passed' and report['native_source'] == source
assert report['source_before'] == report['source_after'] == {'files': len(snapshot['files']), 'unchanged': True}
assert report['archive_sha256'] == snapshot['archive_sha256'] == sha(base / (label + '.tar.gz'))
assert report['plan_sha256'] == sha(base / (label + '-plan.json'))
assert len(report['steps']) == len(plan['steps']) == 3
for result, expected in zip(report['steps'], plan['steps'], strict=True):
    assert result['name'] == expected['name'] and result['actual_exit_code'] == result['exit_code'] == 0
    assert result['validation_error'] is None and result['passing_tests'] == expected.get('expected_passing_tests')
    assert (base / (label + '-' + result['name'] + '.log')).is_file()
assert sum(r['passing_tests'] or 0 for r in report['steps']) == 2
assert cleanup['test_exit_code'] == cleanup['cargo_clean_exit_code'] == 0
assert cleanup['target_removed'] and cleanup['source_preserved'] and cleanup['timing_copy_error'] is None
assert read('-timings.json')['error'] is None
out = base / 'p14-neo-verification-r1.json'
record = {'status': 'passed', 'source': source, 'passing_tests': 2, 'successful_commands': 3,
          'cleanup': cleanup, 'platform': report['platform'],
          'scope': 'Actual restrictive provider cutover and cold bundled-default semantic-ready destination with complete transports; formatting; ARM64 source tests.',
          'limits': report['not_run'],
          'evidence': {p.name: sha(p) for p in [base / (label + '-qualification.json'), base / (label + '-cleanup.json')]}}
with out.open('x', encoding='utf-8') as f:
    f.write(json.dumps(record, indent=2) + '\n')
print(json.dumps({'status': 'passed', 'tests': 2, 'cargo_clean': 0, 'target_removed': True}))
