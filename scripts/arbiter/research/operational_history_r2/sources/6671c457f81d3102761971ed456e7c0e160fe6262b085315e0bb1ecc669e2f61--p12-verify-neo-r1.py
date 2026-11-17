# SPDX-License-Identifier: Apache-2.0
"""Verify collected Neo evidence and mandatory exact-target cargo clean."""
import hashlib
import json
from pathlib import Path

base = Path(__file__).resolve().parent
label = 'p12-native-20261106-r1'
def read(suffix):
    return json.loads((base / (label + suffix)).read_text(encoding='utf-8'))
def sha(path):
    return hashlib.sha256(path.read_bytes()).hexdigest()

report, plan, snapshot, cleanup = (read(s) for s in
    ['-qualification.json', '-plan.json', '-snapshot.json', '-cleanup.json'])
expected = {'sha256': '4121692a2cc0272358023f5362ae886eeed4fd06203cea7af23bbfc5f4be86b9', 'files': 960}
assert report['status'] == 'passed' and report['native_source'] == expected
assert report['source_before'] == report['source_after'] == {'files': len(snapshot['files']), 'unchanged': True}
assert report['archive_sha256'] == snapshot['archive_sha256'] == sha(base / (label + '.tar.gz'))
assert report['plan_sha256'] == sha(base / (label + '-plan.json'))
assert len(report['steps']) == len(plan['steps']) == 10
assert [r['name'] for r in report['steps']] == [r['name'] for r in plan['steps']]
for result, selected in zip(report['steps'], plan['steps'], strict=True):
    assert result['exit_code'] == result['actual_exit_code'] == 0 and result['validation_error'] is None
    assert result['passing_tests'] == selected.get('expected_passing_tests')
    assert (base / (label + '-' + result['name'] + '.log')).is_file()
assert sum(r['passing_tests'] or 0 for r in report['steps']) == 19
assert cleanup['test_exit_code'] == cleanup['cargo_clean_exit_code'] == 0
assert cleanup['target_removed'] and cleanup['source_preserved'] and cleanup['timing_copy_error'] is None
assert read('-timings.json')['error'] is None
out = base / 'p12-neo-combined-verification-r1.json'
record = {'status': 'passed', 'native_source': expected, 'current_windows_source': expected,
          'passing_tests': 19, 'successful_commands': 10,
          'batches': [{'label': label, 'cleanup': cleanup}],
          'evidence': {p.name: sha(p) for p in [base / (label + '-qualification.json'), base / (label + '-cleanup.json')]},
          'limits': report['not_run']}
with out.open('x', encoding='utf-8') as f:
    f.write(json.dumps(record, indent=2) + '\n')
print(json.dumps({'status': 'passed', 'tests': 19, 'cargo_clean': 0, 'target_removed': True}))
