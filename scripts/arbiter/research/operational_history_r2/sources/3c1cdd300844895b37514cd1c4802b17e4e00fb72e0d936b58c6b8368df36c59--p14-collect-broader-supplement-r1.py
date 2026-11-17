# SPDX-License-Identifier: Apache-2.0
import hashlib
import json
from pathlib import Path

base = Path('D:/code/Github/airk/artifacts/v190')
sha = lambda path: hashlib.sha256(path.read_bytes()).hexdigest()
old_path = base / 'p14-broader-qualification-r3.json'
old = json.loads(old_path.read_text())
assert old['status'] == 'failed' and len(old['steps']) == 9
steps = [dict(row) for row in old['steps'][:8]]
assert all(row['exit_code'] == 0 for row in steps)
source = old['source']
for name, stem in [('usage-inputs', 'p14-usage-stable-native-r2'),
                   ('ignored-vector-performance', 'p14-broader-ignored-vector-performance-r2')]:
    path = base / (stem + '.receipt.json')
    receipt = json.loads(path.read_text())
    assert receipt['exit_code'] == receipt['actual_exit_code'] == 0
    assert receipt['source_before'] == receipt['source_after'] == source and receipt['source_unchanged'] is True
    assert sha(base / (stem + '.log')) == receipt['log_sha256']
    steps.append({'name': name, 'exit_code': 0, 'receipt': str(path), 'receipt_sha256': sha(path)})
for row in steps[:8]:
    receipt = json.loads(Path(row['receipt']).read_text())
    assert receipt['exit_code'] == receipt['actual_exit_code'] == 0 and receipt['source_unchanged'] is True
    assert receipt['source_before'] == receipt['source_after'] == source
    log = Path(row['receipt'].replace('.receipt.json', '.log'))
    assert sha(log) == receipt['log_sha256']
result = {'status': 'selected broader checks passed by exact evidence adoption', 'source': source,
          'steps': steps, 'passing_stages': 10,
          'original_failed_queue': {'path': str(old_path), 'sha256': sha(old_path), 'status': 'failed, preserved'},
          'late_usage_adoption_sha256': sha(base / 'p14-usage-live-adoption-r1.json'),
          'limits': 'No rerun or relabel of prior queue; corpus deadline result and editor qualification separate; P14 not yet accepted'}
with (base / 'p14-broader-qualification-supplement-r1.json').open('x') as output:
    json.dump(result, output, indent=2)
print(json.dumps({'passing_stages': len(steps), 'source': source, 'original_failed_queue_preserved': True}))
